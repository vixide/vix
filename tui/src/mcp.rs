//! `vix --mcp` (T731, `spec/mcp-server`): the App shell's side of the
//! read-only Model Context Protocol server.
//!
//! The protocol lives in `vix-mcp-server`; this module only supplies what
//! that pure crate cannot reach — the saved session's open buffers (read
//! without ever creating `session.toml`), the read-only `SQLite` schema
//! query from `vix-db`, and the saved DB connections the user shared with
//! MCP clients (T783: read from `config.toml` without creating it, opened
//! read-only through `vix_db::mcp_share`, every call logged to
//! [`DB_LOG_NAME`] next to the session file) — and runs the stdio loop.

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

use std::io;
use std::path::{Path, PathBuf};
use vix_mcp_server::{Host, HostError, Server, Table};

/// The App shell's [`Host`]: open buffers from the saved session, `SQLite`
/// schemas from `vix-db` when the `sql` capability is compiled in.
///
/// Kept for compatibility: it is [`McpHost`] with only the session file
/// overridable (shared DB connections come from the user's own settings).
#[derive(Debug, Default)]
pub struct ShellHost {
    /// Session file to read instead of the user's own (tests); `None` uses
    /// the normal config location.
    pub session_path: Option<PathBuf>,
}

impl ShellHost {
    /// The [`McpHost`] this host behaves as.
    fn as_mcp_host(&self) -> McpHost {
        McpHost {
            session_path: self.session_path.clone(),
            ..McpHost::default()
        }
    }
}

impl Host for ShellHost {
    fn open_buffers(&self, root: &Path) -> Vec<PathBuf> {
        self.as_mcp_host().open_buffers(root)
    }

    fn sqlite_schema(&self, path: &Path) -> Option<Result<Vec<Table>, HostError>> {
        self.as_mcp_host().sqlite_schema(path)
    }

    fn shared_connections(
        &self,
    ) -> Option<Result<Vec<vix_mcp_server::SharedConnection>, HostError>> {
        self.as_mcp_host().shared_connections()
    }

    fn db_tables(
        &self,
        connection: &str,
        schema: Option<&str>,
    ) -> Option<Result<vix_mcp_server::DbTables, HostError>> {
        self.as_mcp_host().db_tables(connection, schema)
    }

    fn db_describe(
        &self,
        connection: &str,
        schema: Option<&str>,
        table: &str,
    ) -> Option<Result<vix_mcp_server::DbDescription, HostError>> {
        self.as_mcp_host().db_describe(connection, schema, table)
    }

    fn db_query(
        &self,
        connection: &str,
        sql: &str,
    ) -> Option<Result<vix_mcp_server::Rows, HostError>> {
        self.as_mcp_host().db_query(connection, sql)
    }
}

/// The file name of the DB tool log (T783), next to the session file.
pub const DB_LOG_NAME: &str = "mcp-db.log";

/// The size at which the DB tool log is rotated: the current file becomes
/// `mcp-db.log.1` (replacing an older one) and a new file starts, so the
/// two together stay near twice this size.
pub const DB_LOG_MAX_BYTES: u64 = 1024 * 1024;

/// The longest SQL text one log entry keeps, in characters; longer text is
/// cut and ends with `…`.
pub const DB_LOG_SQL_MAX_CHARS: usize = 8 * 1024;

/// The [`Host`] `vix --mcp` serves with: open buffers from the saved
/// session, `SQLite` schemas, and the saved DB connections the user shared
/// (T783), when the `sql` capability is compiled in.
///
/// Every path defaults to the user's own (the config directory); tests set
/// them to scratch files.
#[derive(Debug, Default, Clone)]
#[non_exhaustive]
pub struct McpHost {
    /// Session file to read instead of the user's own.
    pub session_path: Option<PathBuf>,
    /// Settings file (`config.toml`) to read the shared connections from
    /// instead of the user's own. It is read, never created or written.
    pub settings_path: Option<PathBuf>,
    /// DB tool log to append to instead of [`DB_LOG_NAME`] next to the
    /// session file.
    pub db_log_path: Option<PathBuf>,
}

impl McpHost {
    /// The DB tool log: [`McpHost::db_log_path`], else [`DB_LOG_NAME`] in
    /// the session file's directory (the config directory).
    #[must_use]
    pub fn db_log(&self) -> Option<PathBuf> {
        if let Some(path) = &self.db_log_path {
            return Some(path.clone());
        }
        // `session.toml` and `config.toml` share the config directory.
        let beside = self
            .session_path
            .clone()
            .or_else(crate::settings::Settings::config_path)?;
        Some(beside.with_file_name(DB_LOG_NAME))
    }
}

impl Host for McpHost {
    /// Files open in the saved session for the workspace rooted at `root`.
    /// Uses the load-existing path so a missing `session.toml` is not created
    /// (this mode is read-only); no recorded workspace yields an empty list.
    fn open_buffers(&self, root: &Path) -> Vec<PathBuf> {
        let session = match &self.session_path {
            Some(path) => crate::session::Session::load_existing_from(path),
            None => crate::session::Session::load_existing(),
        };
        session
            .workspace(&root.to_string_lossy())
            .map_or_default(|ws| ws.files.iter().map(PathBuf::from).collect())
    }

    /// The schema of the `SQLite` file at `path`, or `None` when this build
    /// has no `sql` capability (the server then reports the tool as
    /// unavailable rather than failing).
    fn sqlite_schema(&self, path: &Path) -> Option<Result<Vec<Table>, HostError>> {
        #[cfg(feature = "sql")]
        {
            Some(sqlite_schema(path).map_err(Into::into))
        }
        // Without the `sql` capability there is no database reader.
        #[cfg(not(feature = "sql"))]
        {
            let _ = path;
            None
        }
    }

    // The shared-connection tools exist only with the `sql` capability;
    // without it the trait's defaults report it missing.
    #[cfg(feature = "sql")]
    fn shared_connections(
        &self,
    ) -> Option<Result<Vec<vix_mcp_server::SharedConnection>, HostError>> {
        let _ = self.log(&shared::entry("db_connections", None));
        Some(shared::load_settings(self).map(|settings| shared::list(&settings)))
    }

    #[cfg(feature = "sql")]
    fn db_tables(
        &self,
        connection: &str,
        schema: Option<&str>,
    ) -> Option<Result<vix_mcp_server::DbTables, HostError>> {
        let mut entry = shared::entry("db_tables", Some(connection));
        shared::note(&mut entry, "schema", schema);
        Some(shared::with_connection(self, connection, &entry, |conn| {
            crate::db::mcp_share::tables(conn, schema).map(|t| vix_mcp_server::DbTables {
                schema: t.schema,
                schemas: t.schemas,
                objects: t.objects,
                truncated: t.truncated,
            })
        }))
    }

    #[cfg(feature = "sql")]
    fn db_describe(
        &self,
        connection: &str,
        schema: Option<&str>,
        table: &str,
    ) -> Option<Result<vix_mcp_server::DbDescription, HostError>> {
        let mut entry = shared::entry("db_describe", Some(connection));
        shared::note(&mut entry, "schema", schema);
        shared::note(&mut entry, "table", Some(table));
        Some(shared::with_connection(self, connection, &entry, |conn| {
            crate::db::mcp_share::describe(conn, schema, table).map(|d| {
                vix_mcp_server::DbDescription {
                    schema: d.schema,
                    table: d.table,
                    columns: shared::rows(d.columns),
                    primary_key: shared::rows(d.primary_key),
                    indexes: shared::rows(d.indexes),
                    foreign_keys: shared::rows(d.foreign_keys),
                    constraints: shared::rows(d.constraints),
                }
            })
        }))
    }

    #[cfg(feature = "sql")]
    fn db_query(
        &self,
        connection: &str,
        sql: &str,
    ) -> Option<Result<vix_mcp_server::Rows, HostError>> {
        let mut entry = shared::entry("db_query", Some(connection));
        shared::note(&mut entry, "sql", Some(&shared::clip(sql)));
        Some(shared::with_connection(self, connection, &entry, |conn| {
            crate::db::mcp_share::query(conn, sql).map(shared::rows)
        }))
    }
}

#[cfg(feature = "sql")]
impl McpHost {
    /// Append one JSON line to the DB tool log, rotating it at
    /// [`DB_LOG_MAX_BYTES`]. The log's directory is never created: it is
    /// the config directory, which exists whenever a connection is shared.
    ///
    /// # Errors
    ///
    /// Returns the I/O error when there is no log location or it cannot be
    /// written.
    fn log(&self, entry: &serde_json::Value) -> io::Result<()> {
        use std::io::Write as _;
        let path = self.db_log().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "no config directory for the log")
        })?;
        if std::fs::metadata(&path).is_ok_and(|m| m.len() >= DB_LOG_MAX_BYTES) {
            let mut old = path.clone().into_os_string();
            old.push(".1");
            std::fs::rename(&path, old)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        // The log holds SQL, which can hold data: owner-only, like the
        // session file.
        crate::fileops::restrict_to_owner(&path);
        writeln!(file, "{entry}")
    }
}

/// The shared-connection side of [`McpHost`] (T783).
#[cfg(feature = "sql")]
mod shared {
    use super::{HostError, McpHost};
    use crate::db::connect::Connection;
    use crate::db::mcp_share::ShareError;
    use crate::settings::Settings;
    use serde_json::{Value, json};

    /// The user's settings, read without creating `config.toml`: a missing
    /// file shares nothing.
    pub(super) fn load_settings(host: &McpHost) -> Result<Settings, HostError> {
        let Some(path) = host.settings_path.clone().or_else(Settings::config_path) else {
            return Ok(Settings::default());
        };
        if !path.is_file() {
            return Ok(Settings::default());
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        toml::from_str(&text).map_err(|e| format!("cannot parse {}: {e}", path.display()).into())
    }

    /// The saved connection named `name`, if it is shared.
    fn find<'a>(settings: &'a Settings, name: &str) -> Option<&'a Connection> {
        if settings.db_mcp_shared.get(name) != Some(&true) {
            return None;
        }
        settings.db_connections.iter().find(|c| c.name == name)
    }

    /// Every shared connection, in saved order (a name saved twice shows
    /// once, as the first): name, kind, environment and database only.
    pub(super) fn list(settings: &Settings) -> Vec<vix_mcp_server::SharedConnection> {
        let mut out: Vec<vix_mcp_server::SharedConnection> = Vec::new();
        for conn in &settings.db_connections {
            if settings.db_mcp_shared.get(&conn.name) != Some(&true)
                || out.iter().any(|c| c.name == conn.name)
            {
                continue;
            }
            let database = if conn.kind == crate::db::connect::Kind::Sqlite {
                std::path::Path::new(&conn.file)
                    .file_name()
                    .map_or_default(|n| n.to_string_lossy().into_owned())
            } else {
                conn.database.clone()
            };
            out.push(vix_mcp_server::SharedConnection {
                name: conn.name.clone(),
                kind: conn.kind.label().to_string(),
                environment: settings
                    .db_environments
                    .get(&conn.name)
                    .copied()
                    .unwrap_or_default()
                    .id()
                    .to_string(),
                database,
            });
        }
        out
    }

    /// Run `read` on the shared connection named `name`: an unshared or
    /// unknown name is refused (and logged if the log can be written); a
    /// shared one is logged first, and refused if the log cannot be
    /// written, so no call goes unrecorded.
    pub(super) fn with_connection<T>(
        host: &McpHost,
        name: &str,
        entry: &Value,
        read: impl FnOnce(&Connection) -> Result<T, ShareError>,
    ) -> Result<T, HostError> {
        let settings = match load_settings(host) {
            Ok(settings) => settings,
            Err(e) => {
                let _ = host.log(entry);
                return Err(e);
            }
        };
        let Some(conn) = find(&settings, name) else {
            let _ = host.log(entry);
            return Err(format!(
                "no shared connection named {name} (db_connections lists the shared ones)"
            )
            .into());
        };
        host.log(entry)
            .map_err(|e| format!("cannot write the DB tool log, so the call was not run: {e}"))?;
        read(conn).map_err(Into::into)
    }

    /// A log entry: the time (UTC), the tool and the connection.
    pub(super) fn entry(tool: &str, connection: Option<&str>) -> Value {
        let mut entry = json!({
            "time": jiff::Timestamp::now().strftime("%Y-%m-%dT%H:%M:%SZ").to_string(),
            "tool": tool,
        });
        note(&mut entry, "connection", connection);
        entry
    }

    /// Add `key` to a log entry when there is a value.
    pub(super) fn note(entry: &mut Value, key: &str, value: Option<&str>) {
        if let Some(value) = value {
            entry[key] = json!(value);
        }
    }

    /// `sql` cut to [`super::DB_LOG_SQL_MAX_CHARS`] characters for the log.
    pub(super) fn clip(sql: &str) -> String {
        let max = super::DB_LOG_SQL_MAX_CHARS;
        if sql.chars().count() <= max {
            sql.to_string()
        } else {
            let mut cut: String = sql.chars().take(max).collect();
            cut.push('…');
            cut
        }
    }

    /// `vix-db`'s capped rows as the protocol crate's.
    pub(super) fn rows(rows: crate::db::mcp_share::Rows) -> vix_mcp_server::Rows {
        vix_mcp_server::Rows {
            columns: rows.columns,
            rows: rows.rows,
            truncated: rows.truncated,
        }
    }
}

/// Read one `SQLite` file's schema through `vix-db`, grouping its
/// `(object, kind, column, type)` rows into tables.
#[cfg(feature = "sql")]
fn sqlite_schema(path: &Path) -> Result<Vec<Table>, crate::db::session::SessionError> {
    crate::db::catalog::read_sqlite_schema(path).map(|rows| {
        let mut tables: Vec<Table> = Vec::new();
        for (name, kind, column, data_type) in rows {
            // The catalog rows arrive grouped by object, so a change of
            // (name, kind) from the previous row starts a new table.
            if tables
                .last()
                .is_none_or(|t| t.name != name || t.kind != kind)
            {
                tables.push(Table {
                    name,
                    kind,
                    columns: Vec::new(),
                });
            }
            if let Some(table) = tables.last_mut() {
                table.columns.push(vix_mcp_server::Column {
                    name: column,
                    data_type,
                });
            }
        }
        tables
    })
}

/// The workspace root for `vix --mcp [DIR]`: `DIR` when it is given, else
/// the current directory.
///
/// # Errors
///
/// Returns the I/O error when the current directory cannot be read.
pub fn root_for(files: &[PathBuf]) -> io::Result<PathBuf> {
    match files.first() {
        Some(dir) => Ok(dir.clone()),
        None => std::env::current_dir(),
    }
}

/// `vix --mcp [DIR]`: serve until stdin closes. On failure, report it on
/// stderr in the `--locale` language (stdout carries protocol messages only)
/// and return `false` so the binary exits non-zero.
///
/// Runs before `config.toml` is loaded, on purpose: loading it would create
/// a default file, and this mode writes nothing.
#[must_use]
pub fn run(files: &[PathBuf], locale: Option<&str>) -> bool {
    if let Some(locale) = locale {
        rust_i18n::set_locale(locale);
    }
    // A shared server connection (T783) may take its password from the OS
    // keyring; nothing is read from it unless an MCP client uses one.
    crate::use_system_keyring();
    match root_for(files).and_then(|root| serve_stdio(&root)) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("{}", t!("msg.mcp_server_failed", error = e));
            false
        }
    }
}

/// Serve MCP over this process's stdin/stdout until stdin closes.
///
/// # Errors
///
/// Returns an error when `root` is not a readable directory, or on a stdio
/// read/write failure.
pub fn serve_stdio(root: &Path) -> io::Result<()> {
    let mut server = Server::new(root, McpHost::default(), env!("CARGO_PKG_VERSION"))?;
    let stdin = io::stdin();
    let stdout = io::stdout();
    vix_mcp_server::serve(&mut server, stdin.lock(), stdout.lock())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::cov4::temp;

    #[test]
    fn root_is_the_given_directory_or_the_current_one() {
        let dir = temp("mcp-root");
        assert_eq!(root_for(std::slice::from_ref(&dir)).unwrap(), dir);
        assert_eq!(
            root_for(&[]).unwrap(),
            std::env::current_dir().unwrap(),
            "no argument means the current directory"
        );
    }

    #[test]
    fn serving_a_missing_root_is_an_error() {
        let missing = temp("mcp-missing").join("nope");
        assert!(serve_stdio(&missing).is_err());
    }

    /// A session file recording `files` open in the workspace at `root`.
    fn session_with(dir: &Path, root: &Path, files: &[&str]) -> PathBuf {
        let path = dir.join("session.toml");
        let mut session = crate::session::Session::default();
        session.set_workspace(crate::session::WorkspaceSession {
            root: root.to_string_lossy().into_owned(),
            files: files.iter().map(ToString::to_string).collect(),
            ..crate::session::WorkspaceSession::default()
        });
        session.save_to(&path).unwrap();
        path
    }

    #[test]
    fn open_buffers_come_from_the_saved_session_only() {
        let dir = temp("mcp-session");
        let root = dir.join("ws");
        let path = session_with(&dir, &root, &["/a/one.rs", "/a/two.rs"]);
        let host = ShellHost {
            session_path: Some(path),
        };
        assert_eq!(
            host.open_buffers(&root),
            vec![PathBuf::from("/a/one.rs"), PathBuf::from("/a/two.rs")]
        );
        assert!(
            host.open_buffers(&dir.join("other")).is_empty(),
            "a workspace the session does not know has no buffers"
        );
        let missing = ShellHost {
            session_path: Some(dir.join("absent.toml")),
        };
        assert_eq!(missing.open_buffers(&root), Vec::<PathBuf>::new());
        assert!(
            !dir.join("absent.toml").exists(),
            "reading never creates the session file"
        );
    }

    #[test]
    fn the_db_log_sits_beside_the_session_unless_overridden() {
        let dir = temp("mcp-logpath");
        let beside = McpHost {
            session_path: Some(dir.join("session.toml")),
            ..McpHost::default()
        };
        assert_eq!(beside.db_log(), Some(dir.join(DB_LOG_NAME)));
        let explicit = McpHost {
            session_path: Some(dir.join("session.toml")),
            db_log_path: Some(dir.join("custom.log")),
            ..McpHost::default()
        };
        assert_eq!(explicit.db_log(), Some(dir.join("custom.log")));
    }

    #[cfg(not(feature = "sql"))]
    #[test]
    fn without_sql_the_database_tools_are_reported_unavailable() {
        let host = McpHost::default();
        assert!(host.sqlite_schema(Path::new("x.db")).is_none());
        assert!(host.shared_connections().is_none());
        assert!(host.db_tables("c", None).is_none());
        assert!(host.db_describe("c", None, "t").is_none());
        assert!(host.db_query("c", "select 1").is_none());
        // The shell host delegates to the same defaults.
        let shell = ShellHost::default();
        assert!(shell.sqlite_schema(Path::new("x.db")).is_none());
        assert!(shell.shared_connections().is_none());
        assert!(shell.db_tables("c", None).is_none());
        assert!(shell.db_describe("c", None, "t").is_none());
        assert!(shell.db_query("c", "select 1").is_none());
    }

    #[cfg(feature = "sql")]
    mod sql {
        use super::*;
        use crate::settings::Settings;

        /// A `SQLite` file with a parent/child schema and two parent rows.
        fn database(dir: &Path) -> PathBuf {
            let path = dir.join("app.db");
            let url = format!("sqlite://{}?mode=rwc", path.display());
            let mut session = crate::db::session::Session::connect(&url, &[]).unwrap();
            for sql in [
                "CREATE TABLE parent (id INTEGER PRIMARY KEY, name TEXT)",
                "CREATE TABLE child (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES parent(id))",
                "CREATE VIEW names AS SELECT name FROM parent",
                "INSERT INTO parent VALUES (1,'alpha'),(2,'beta')",
            ] {
                session.run(sql).unwrap();
            }
            path
        }

        /// A host whose settings share `shared` (and save `saved` too).
        fn host_sharing(dir: &Path, db: &Path, shared: &[&str], saved: &[&str]) -> McpHost {
            let mut settings = Settings::default();
            for name in saved {
                settings
                    .db_connections
                    .push(crate::db::connect::Connection {
                        name: (*name).to_string(),
                        kind: crate::db::connect::Kind::Sqlite,
                        file: db.to_string_lossy().into_owned(),
                        ..crate::db::connect::Connection::default()
                    });
            }
            for name in shared {
                settings.db_mcp_shared.insert((*name).to_string(), true);
            }
            let settings_path = dir.join("config.toml");
            std::fs::write(&settings_path, toml::to_string(&settings).unwrap()).unwrap();
            McpHost {
                session_path: Some(dir.join("session.toml")),
                settings_path: Some(settings_path),
                db_log_path: Some(dir.join("mcp-db.log")),
            }
        }

        fn log_lines(host: &McpHost) -> Vec<serde_json::Value> {
            std::fs::read_to_string(host.db_log().unwrap())
                .unwrap_or_default()
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect()
        }

        #[test]
        fn sqlite_schema_groups_columns_into_tables_and_views() {
            let dir = temp("mcp-schema");
            let db = database(&dir);
            let tables = McpHost::default()
                .sqlite_schema(&db)
                .expect("sql capability is built")
                .unwrap();
            let parent = tables.iter().find(|t| t.name == "parent").unwrap();
            assert_eq!(
                parent
                    .columns
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>(),
                ["id", "name"]
            );
            let child = tables.iter().find(|t| t.name == "child").unwrap();
            assert_eq!(child.columns.len(), 2, "one table per object");
            assert_ne!(
                tables.iter().find(|t| t.name == "names").unwrap().kind,
                parent.kind,
                "a view is a different kind of object"
            );
            assert!(
                McpHost::default()
                    .sqlite_schema(&dir.join("missing-dir").join("x.db"))
                    .unwrap()
                    .is_err()
            );
        }

        #[test]
        fn only_shared_connections_are_listed_once_each_with_no_secrets() {
            let dir = temp("mcp-list");
            let db = database(&dir);
            let host = host_sharing(&dir, &db, &["app", "dup"], &["app", "hidden", "dup", "dup"]);
            let listed = host.shared_connections().unwrap().unwrap();
            let names: Vec<&str> = listed.iter().map(|c| c.name.as_str()).collect();
            assert_eq!(names, ["app", "dup"], "unshared hidden, duplicate once");
            assert_eq!(
                listed[0].database, "app.db",
                "SQLite shows the file name only"
            );
            assert_eq!(listed[0].environment, "local");
            let entries = log_lines(&host);
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0]["tool"], "db_connections");
            assert!(entries[0]["time"].as_str().unwrap().ends_with('Z'));
        }

        #[test]
        fn no_settings_file_shares_nothing_and_a_bad_one_is_an_error() {
            let dir = temp("mcp-nosettings");
            let host = McpHost {
                settings_path: Some(dir.join("config.toml")),
                db_log_path: Some(dir.join("mcp-db.log")),
                ..McpHost::default()
            };
            assert_eq!(host.shared_connections().unwrap().unwrap(), Vec::new());
            assert!(
                !dir.join("config.toml").exists(),
                "a missing config is never created"
            );
            std::fs::write(dir.join("config.toml"), "db_connections = [[[").unwrap();
            let err = host.shared_connections().unwrap().unwrap_err();
            assert!(err.to_string().contains("cannot parse"), "{err}");
            let bad = host.db_tables("app", None).unwrap().unwrap_err();
            assert!(bad.to_string().contains("cannot parse"), "{bad}");
            assert_eq!(
                log_lines(&host).last().unwrap()["tool"],
                "db_tables",
                "a refused call is still logged"
            );
        }

        #[test]
        fn shared_tables_describe_and_query_are_logged_before_they_run() {
            let dir = temp("mcp-tools");
            let db = database(&dir);
            let host = host_sharing(&dir, &db, &["app"], &["app"]);

            let tables = host.db_tables("app", None).unwrap().unwrap();
            assert!(tables.objects.iter().any(|(name, _)| name == "parent"));
            assert!(tables.objects.iter().any(|(name, _)| name == "names"));
            assert!(!tables.truncated);

            let described = host
                .db_describe("app", Some("main"), "parent")
                .unwrap()
                .unwrap();
            assert_eq!(described.table, "parent");
            assert_ne!(described.columns.rows, Vec::<Vec<String>>::new());
            let missing = host
                .db_describe("app", None, "no_such_table")
                .unwrap()
                .unwrap_err();
            assert_ne!(missing.to_string(), "");

            let rows = host
                .db_query("app", "SELECT id, name FROM parent ORDER BY id")
                .unwrap()
                .unwrap();
            assert_eq!(rows.columns, ["id", "name"]);
            assert_eq!(rows.rows, [["1", "alpha"], ["2", "beta"]]);
            let refused = host
                .db_query("app", "DELETE FROM parent")
                .unwrap()
                .unwrap_err();
            assert!(!refused.to_string().is_empty(), "writes are refused");
            let still = host
                .db_query("app", "SELECT count(*) FROM parent")
                .unwrap()
                .unwrap();
            assert_eq!(still.rows, [["2"]], "the refused delete changed nothing");

            let tools: Vec<String> = log_lines(&host)
                .iter()
                .map(|e| e["tool"].as_str().unwrap().to_string())
                .collect();
            assert_eq!(
                tools,
                [
                    "db_tables",
                    "db_describe",
                    "db_describe",
                    "db_query",
                    "db_query",
                    "db_query"
                ]
            );
            let entries = log_lines(&host);
            assert_eq!(entries[0]["connection"], "app");
            assert_eq!(entries[1]["schema"], "main");
            assert_eq!(entries[1]["table"], "parent");
            assert_eq!(entries[3]["sql"], "SELECT id, name FROM parent ORDER BY id");
        }

        #[test]
        fn an_unshared_or_unknown_connection_is_refused_but_logged() {
            let dir = temp("mcp-refuse");
            let db = database(&dir);
            let host = host_sharing(&dir, &db, &["app"], &["app", "private"]);
            for name in ["private", "ghost"] {
                let err = host.db_query(name, "SELECT 1").unwrap().unwrap_err();
                assert!(
                    err.to_string()
                        .contains(&format!("no shared connection named {name}")),
                    "{err}"
                );
            }
            let entries = log_lines(&host);
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0]["connection"], "private");
        }

        #[test]
        fn a_call_is_not_run_when_its_log_cannot_be_written() {
            let dir = temp("mcp-nolog");
            let db = database(&dir);
            let mut host = host_sharing(&dir, &db, &["app"], &["app"]);
            // The "log" is a directory, so appending fails.
            host.db_log_path = Some(dir);
            let err = host.db_query("app", "SELECT 1").unwrap().unwrap_err();
            assert!(err.to_string().contains("was not run"), "{err}");
        }

        #[test]
        fn the_log_rotates_at_its_size_limit() {
            let dir = temp("mcp-rotate");
            let db = database(&dir);
            let host = host_sharing(&dir, &db, &["app"], &["app"]);
            let log = host.db_log().unwrap();
            let filler = usize::try_from(DB_LOG_MAX_BYTES).unwrap();
            std::fs::write(&log, "x".repeat(filler)).unwrap();
            host.db_query("app", "SELECT 1").unwrap().unwrap();
            let mut rotated = log.into_os_string();
            rotated.push(".1");
            assert_eq!(
                std::fs::metadata(&rotated).unwrap().len(),
                DB_LOG_MAX_BYTES,
                "the full file moved aside"
            );
            assert_eq!(log_lines(&host).len(), 1, "a fresh log starts");
        }

        #[test]
        fn long_sql_is_clipped_in_the_log_with_an_ellipsis() {
            let short = "SELECT 1";
            assert_eq!(shared::clip(short), short);
            let long = format!("SELECT '{}'", "a".repeat(DB_LOG_SQL_MAX_CHARS));
            let clipped = shared::clip(&long);
            assert_eq!(clipped.chars().count(), DB_LOG_SQL_MAX_CHARS + 1);
            assert!(clipped.ends_with('…'));
            let mut entry = shared::entry("db_query", None);
            shared::note(&mut entry, "sql", None);
            assert!(entry.get("sql").is_none(), "no value, no key");
            assert!(entry.get("connection").is_none());
        }

        #[test]
        fn the_shell_host_delegates_schema_reads_to_the_mcp_host() {
            let dir = temp("mcp-shell");
            let db = database(&dir);
            let shell = ShellHost {
                session_path: Some(dir.join("session.toml")),
            };
            assert!(shell.sqlite_schema(&db).unwrap().is_ok());
        }

        /// The shell host reads the user's own settings, so this only runs
        /// when the config directory is a throwaway one.
        #[test]
        fn the_shell_host_delegates_the_shared_connection_tools_and_refuses_unknown_ones() {
            let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) else {
                return;
            };
            if !Settings::config_path().is_some_and(|p| p.starts_with(&xdg)) {
                return;
            }
            let dir = temp("mcp-shell-shared");
            let shell = ShellHost {
                session_path: Some(dir.join("session.toml")),
            };
            // A default config shares nothing.
            assert!(shell.shared_connections().unwrap().is_ok());
            // Unknown connections are refused (and logged beside the session).
            assert!(
                shell
                    .db_tables("r4-no-such-connection", None)
                    .unwrap()
                    .is_err()
            );
            assert!(
                shell
                    .db_describe("r4-no-such-connection", Some("main"), "t")
                    .unwrap()
                    .is_err()
            );
            assert!(
                shell
                    .db_query("r4-no-such-connection", "select 1")
                    .unwrap()
                    .is_err()
            );
            let log = std::fs::read_to_string(dir.join(DB_LOG_NAME)).unwrap();
            let tools: Vec<String> = log
                .lines()
                .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
                .filter_map(|v| v["tool"].as_str().map(str::to_string))
                .collect();
            assert_eq!(
                tools,
                ["db_connections", "db_tables", "db_describe", "db_query"]
            );
        }
    }
}
