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
            .map(|ws| ws.files.iter().map(PathBuf::from).collect())
            .unwrap_or_default()
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
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
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
    crate::db::secret::use_system_keyring();
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
