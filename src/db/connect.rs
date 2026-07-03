//! Database connections: the saved-connection model and connection URLs.
//!
//! A [`Connection`] holds everything needed to reach a database *except* the
//! password — passwords are never serialized; the host prompts for one at
//! connect time and keeps it only in memory. [`url`] renders the connection
//! (plus that in-memory password) as the `sqlite:` / `postgres:` / `mysql:`
//! URL that [`crate::db::session`] hands to sqlx's `Any` driver.

use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
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
}

impl Connection {
    /// Whether connecting should prompt for a password (server engines only).
    #[must_use]
    pub fn needs_password(&self) -> bool {
        !matches!(self.kind, Kind::Sqlite)
    }

    /// One-line summary of the target, for the connections list.
    #[must_use]
    pub fn target(&self) -> String {
        match self.kind {
            Kind::Sqlite => self.file.clone(),
            Kind::Postgres | Kind::Mysql => {
                let port = if self.port.is_empty() { self.kind.default_port() } else { &self.port };
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
    match conn.kind {
        Kind::Sqlite => format!("sqlite:{}?mode=rwc", conn.file),
        Kind::Postgres | Kind::Mysql => {
            let scheme = if conn.kind == Kind::Postgres { "postgres" } else { "mysql" };
            let port = if conn.port.is_empty() { conn.kind.default_port() } else { &conn.port };
            let user = utf8_percent_encode(&conn.user, NON_ALPHANUMERIC);
            let auth = if password.is_empty() {
                user.to_string()
            } else {
                format!("{user}:{}", utf8_percent_encode(password, NON_ALPHANUMERIC))
            };
            format!("{scheme}://{auth}@{}:{port}/{}", conn.host, conn.database)
        }
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
        assert_eq!(url(&conn, "hunter:2@x"), "postgres://joel:hunter%3A2%40x@db.example.com:5432/orders");
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
        let mut c = Connection { kind: Kind::Sqlite, file: "a.db".into(), ..Connection::default() };
        assert_eq!(c.target(), "a.db");
        c.kind = Kind::Postgres;
        c.host = "h".into();
        c.user = "u".into();
        c.database = "d".into();
        assert_eq!(c.target(), "u@h:5432/d");
        assert!(c.needs_password());
    }

    #[test]
    fn kind_cycles_through_all_engines() {
        assert_eq!(Kind::Sqlite.next(), Kind::Postgres);
        assert_eq!(Kind::Postgres.next(), Kind::Mysql);
        assert_eq!(Kind::Mysql.next(), Kind::Sqlite);
    }
}
