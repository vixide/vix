//! Database connections: the saved-connection model and connection URLs.
//!
//! A [`Connection`] holds everything needed to reach a database *except* the
//! password — passwords are never serialized; the host prompts for one at
//! connect time and keeps it only in memory. [`url`] renders the connection
//! (plus that in-memory password) as the `sqlite:` / `postgres:` / `mysql:`
//! URL that [`crate::session`] hands to sqlx's `Any` driver.

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde::{Deserialize, Serialize};

/// Which database engine a connection targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// `SQLite` file (bundled driver).
    #[default]
    Sqlite,
    /// `PostgreSQL` server (pure-Rust driver over rustls).
    Postgres,
    /// `MySQL` / `MariaDB` server (pure-Rust driver over rustls).
    Mysql,
}

impl Kind {
    /// Short display label (`sqlite`, `postgres`, `mysql`).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Kind::Sqlite => "sqlite",
            Kind::Postgres => "postgres",
            Kind::Mysql => "mysql",
        }
    }

    /// The next kind, cycling; used by the connection form's kind field.
    #[must_use]
    pub fn next(self) -> Kind {
        match self {
            Kind::Sqlite => Kind::Postgres,
            Kind::Postgres => Kind::Mysql,
            Kind::Mysql => Kind::Sqlite,
        }
    }

    /// Default server port as a string (empty for `SQLite`).
    #[must_use]
    pub fn default_port(self) -> &'static str {
        match self {
            Kind::Sqlite => "",
            Kind::Postgres => "5432",
            Kind::Mysql => "3306",
        }
    }
}

/// One saved database connection. Passwords are deliberately absent: they are
/// prompted for at connect time and held only in memory (see `spec/db`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Connection {
    /// Display name shown in the connections list.
    pub name: String,
    /// Database engine.
    pub kind: Kind,
    /// `SQLite` database file path (unused for server kinds).
    pub file: String,
    /// Server host (unused for `SQLite`).
    pub host: String,
    /// Server port as text; empty uses the engine default.
    pub port: String,
    /// Login user (unused for `SQLite`).
    pub user: String,
    /// Database name to open (unused for `SQLite`).
    pub database: String,
    /// Whether the connection may run writes / DDL. Defaults to `false`
    /// (read-only), so a fresh or legacy connection opens read-only until the
    /// user opts in — writes are refused client-side and, where the engine
    /// supports it, blocked at the database level (see [`read_only_sql`]).
    pub writable: bool,
    /// A shell command whose stdout is the password (e.g. `pass show db/prod`,
    /// `op read op://vault/db/password`). Tried first by the credential
    /// waterfall so the password prompt can be skipped (see [`crate::secret`]).
    pub password_command: String,
    /// Whether to store a prompted password in the OS keyring on a successful
    /// connect, so later connects resolve it without prompting.
    pub store_keyring: bool,
    /// SSH jump host — when set, the connection is forwarded through an
    /// `ssh -N -L` tunnel (see [`crate::tunnel`]); empty disables it.
    pub ssh_host: String,
    /// SSH login user (defaults to the ssh client's default when empty).
    pub ssh_user: String,
    /// SSH port (defaults to 22 when empty).
    pub ssh_port: String,
    /// Path to an SSH identity file (optional; agent auth is used otherwise).
    pub ssh_identity: String,
}

impl Connection {
    /// Whether connecting should prompt for a password (server engines only).
    #[must_use]
    pub fn needs_password(&self) -> bool {
        !matches!(self.kind, Kind::Sqlite)
    }

    /// The access label shown in the form and connections list.
    #[must_use]
    pub fn access_label(&self) -> &'static str {
        if self.writable {
            "read-write"
        } else {
            "read-only"
        }
    }

    /// One-line summary of the target, for the connections list.
    #[must_use]
    pub fn target(&self) -> String {
        match self.kind {
            Kind::Sqlite => self.file.clone(),
            Kind::Postgres | Kind::Mysql => {
                let port = if self.port.is_empty() {
                    self.kind.default_port()
                } else {
                    &self.port
                };
                format!("{}@{}:{}/{}", self.user, self.host, port, self.database)
            }
        }
    }
}

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
            format!("{scheme}://{auth}@{host}:{port}/{}", conn.database)
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
