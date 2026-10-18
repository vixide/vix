//! Database connections: the saved-connection model and connection URLs.
//!
//! A [`Connection`] holds everything needed to reach a database *except* the
//! password — passwords are never serialized; the host prompts for one at
//! connect time and keeps it only in memory. [`url`] renders the connection
//! (plus that in-memory password) as the `sqlite:` / `postgres:` / `mysql:`
//! URL that [`crate::session`] hands to sqlx's `Any` driver.

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use std::fmt::Write as _;

pub use vix_settings::{Connection, Kind};

/// The sqlx connection URL for `conn`. The password appears only in this
/// in-memory string, percent-encoded alongside the user; it is never written
/// anywhere. `SQLite` uses `mode=rwc` so a fresh path creates the file, like
/// the `sqlite3` CLI would.
#[must_use]
pub fn url(conn: &Connection, password: &str) -> String {
    server_url(conn, password, &conn.host, {
        if conn.port.is_empty() {
            conn.kind.default_port()
        } else {
            &conn.port
        }
    })
}

/// Like [`url`], but pointed at `127.0.0.1:local_port` — the local end of an
/// SSH tunnel (see [`crate::tunnel`]). Server engines only.
#[must_use]
pub fn url_via_local(conn: &Connection, password: &str, local_port: u16) -> String {
    let port = local_port.to_string();
    server_url(conn, password, "127.0.0.1", &port)
}

/// Build the server (`postgres` / `mysql`) URL against an explicit `host` and
/// `port`; `SQLite` ignores both and targets its file.
fn server_url(conn: &Connection, password: &str, host: &str, port: &str) -> String {
    match conn.kind {
        Kind::Sqlite => format!("sqlite:{}?mode=rwc", conn.file),
        Kind::Postgres | Kind::Mysql => {
            let scheme = if conn.kind == Kind::Postgres {
                "postgres"
            } else {
                "mysql"
            };
            let user = utf8_percent_encode(&conn.user, NON_ALPHANUMERIC);
            let auth = if password.is_empty() {
                user.to_string()
            } else {
                format!("{user}:{}", utf8_percent_encode(password, NON_ALPHANUMERIC))
            };
            let mut url = format!("{scheme}://{auth}@{host}:{port}/{}", conn.database);
            // Restrict the TLS mode to the alphabet its known values use
            // (`require`, `verify-full`, `VERIFY_IDENTITY`, …); this preserves
            // hyphens/underscores while preventing a crafted value from injecting
            // extra query parameters (`&`, `=`) into the URL.
            let value: String = conn
                .sslmode
                .trim()
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if !value.is_empty() {
                // Postgres reads `sslmode`; sqlx-mysql reads `ssl-mode`.
                let key = if conn.kind == Kind::Postgres {
                    "sslmode"
                } else {
                    "ssl-mode"
                };
                let _ = write!(url, "?{key}={value}");
            }
            url
        }
    }
}

/// The statement that puts a session into (or out of) read-only mode at the
/// database level, or `None` for engines without a session-level switch.
///
/// `SQLite`'s `query_only` pragma and `PostgreSQL`'s
/// `default_transaction_read_only` both make the server itself reject writes —
/// defense in depth behind the client-side
/// [`crate::editor::is_write_statement`] guard. `MySQL` has no equivalent
/// that survives autocommit, so it relies on the client guard alone.
#[must_use]
pub fn read_only_sql(kind: Kind, read_only: bool) -> Option<String> {
    match kind {
        Kind::Sqlite => Some(format!(
            "PRAGMA query_only = {}",
            if read_only { "ON" } else { "OFF" }
        )),
        Kind::Postgres => Some(format!(
            "SET default_transaction_read_only = {}",
            if read_only { "on" } else { "off" }
        )),
        Kind::Mysql => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_url_targets_the_file_and_may_create_it() {
        let conn = Connection {
            name: "app".into(),
            kind: Kind::Sqlite,
            file: "/tmp/app.db".into(),
            ..Connection::default()
        };
        assert_eq!(url(&conn, ""), "sqlite:/tmp/app.db?mode=rwc");
        assert!(!conn.needs_password(), "sqlite never takes a password");
    }

    #[test]
    fn postgres_url_encodes_credentials_and_defaults_the_port() {
        let conn = Connection {
            name: "prod".into(),
            kind: Kind::Postgres,
            host: "db.example.com".into(),
            user: "joel".into(),
            database: "orders".into(),
            ..Connection::default()
        };
        assert_eq!(
            url(&conn, "hunter:2@x"),
            "postgres://joel:hunter%3A2%40x@db.example.com:5432/orders"
        );
        assert_eq!(url(&conn, ""), "postgres://joel@db.example.com:5432/orders");
    }

    #[test]
    fn mysql_url_uses_explicit_port() {
        let conn = Connection {
            name: "local".into(),
            kind: Kind::Mysql,
            host: "localhost".into(),
            port: "3307".into(),
            user: "root".into(),
            database: "test".into(),
            ..Connection::default()
        };
        assert_eq!(url(&conn, "pw"), "mysql://root:pw@localhost:3307/test");
    }

    #[test]
    fn sslmode_is_appended_per_engine_and_omitted_when_empty() {
        // Default (empty) preserves the current behaviour: no TLS query param.
        let mut pg = Connection {
            kind: Kind::Postgres,
            host: "h".into(),
            user: "u".into(),
            database: "d".into(),
            ..Connection::default()
        };
        assert_eq!(url(&pg, ""), "postgres://u@h:5432/d");

        // Postgres renders `sslmode=`.
        pg.sslmode = "verify-full".into();
        assert_eq!(url(&pg, ""), "postgres://u@h:5432/d?sslmode=verify-full");

        // MySQL renders `ssl-mode=`.
        let my = Connection {
            kind: Kind::Mysql,
            host: "h".into(),
            user: "u".into(),
            database: "d".into(),
            sslmode: "VERIFY_IDENTITY".into(),
            ..Connection::default()
        };
        assert_eq!(url(&my, ""), "mysql://u@h:3306/d?ssl-mode=VERIFY_IDENTITY");

        // A tunnelled connection carries the mode through to the local endpoint.
        assert_eq!(
            url_via_local(&pg, "", 55000),
            "postgres://u@127.0.0.1:55000/d?sslmode=verify-full"
        );

        // A crafted value cannot inject extra query parameters.
        pg.sslmode = "require&search_path=evil".into();
        assert_eq!(
            url(&pg, ""),
            "postgres://u@h:5432/d?sslmode=requiresearch_pathevil"
        );
    }

    #[test]
    fn connection_target_summarizes_per_kind() {
        let mut c = Connection {
            kind: Kind::Sqlite,
            file: "a.db".into(),
            ..Connection::default()
        };
        assert_eq!(c.target(), "a.db");
        c.kind = Kind::Postgres;
        c.host = "h".into();
        c.user = "u".into();
        c.database = "d".into();
        assert_eq!(c.target(), "u@h:5432/d");
        assert!(c.needs_password());
    }

    #[test]
    fn connections_are_read_only_until_opted_in() {
        let c = Connection::default();
        assert!(!c.writable, "a fresh connection is read-only by default");
        assert_eq!(c.access_label(), "read-only");
        let w = Connection {
            writable: true,
            ..Connection::default()
        };
        assert_eq!(w.access_label(), "read-write");
    }

    #[test]
    fn read_only_sql_is_per_engine() {
        assert_eq!(
            read_only_sql(Kind::Sqlite, true).as_deref(),
            Some("PRAGMA query_only = ON")
        );
        assert_eq!(
            read_only_sql(Kind::Sqlite, false).as_deref(),
            Some("PRAGMA query_only = OFF")
        );
        assert_eq!(
            read_only_sql(Kind::Postgres, true).as_deref(),
            Some("SET default_transaction_read_only = on")
        );
        assert_eq!(
            read_only_sql(Kind::Mysql, true),
            None,
            "mysql relies on the client guard"
        );
    }

    #[test]
    fn kind_cycles_through_all_engines() {
        assert_eq!(Kind::Sqlite.next(), Kind::Postgres);
        assert_eq!(Kind::Postgres.next(), Kind::Mysql);
        assert_eq!(Kind::Mysql.next(), Kind::Sqlite);
    }
}
