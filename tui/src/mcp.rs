//! `vix --mcp` (T731, `spec/mcp-server`): the App shell's side of the
//! read-only Model Context Protocol server.
//!
//! The protocol lives in `vix-mcp-server`; this module only supplies what
//! that pure crate cannot reach — the saved session's open buffers (read
//! without ever creating `session.toml`) and the read-only `SQLite` schema
//! query from `vix-db` — and runs the stdio loop.

use std::io;
use std::path::{Path, PathBuf};
use vix_mcp_server::{Host, Server, Table};

/// The App shell's [`Host`]: open buffers from the saved session, `SQLite`
/// schemas from `vix-db` when the `sql` capability is compiled in.
#[derive(Debug, Default)]
pub struct ShellHost {
    /// Session file to read instead of the user's own (tests); `None` uses
    /// the normal config location.
    pub session_path: Option<PathBuf>,
}

impl Host for ShellHost {
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

    fn sqlite_schema(&self, path: &Path) -> Option<Result<Vec<Table>, vix_mcp_server::HostError>> {
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
}

/// Read one `SQLite` file's schema through `vix-db`, grouping its
/// `(object, kind, column, type)` rows into tables.
#[cfg(feature = "sql")]
fn sqlite_schema(path: &Path) -> Result<Vec<Table>, crate::db::session::SessionError> {
    crate::db::catalog::read_sqlite_schema(path).map(|rows| {
        let mut tables: Vec<Table> = Vec::new();
        for (name, kind, column, data_type) in rows {
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
    let mut server = Server::new(root, ShellHost::default(), env!("CARGO_PKG_VERSION"))?;
    let stdin = io::stdin();
    let stdout = io::stdout();
    vix_mcp_server::serve(&mut server, stdin.lock(), stdout.lock())
}
