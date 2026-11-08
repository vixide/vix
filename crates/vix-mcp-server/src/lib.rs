//! A read-only [Model Context Protocol](https://modelcontextprotocol.io)
//! server core for `vix --mcp` (T731, `spec/mcp-server`).
//!
//! JSON-RPC 2.0, one message per line over stdio. A [`Server`] owns the
//! canonical workspace root and answers `initialize`, `ping`, `tools/list`,
//! `tools/call`, `resources/list` and `resources/read`. Everything it reads
//! goes through [`workspace`], which keeps every path inside the root and
//! refuses secret files. What only the App shell can supply — the saved
//! session's open buffers and the read-only SQLite schema query — comes in
//! through the [`Host`] trait, so this crate stays free of `confy`, `sqlx`
//! and terminal I/O and is tested with a fake host.
//!
//! # Layout
//!
//! - this file: the JSON-RPC framing, method dispatch, `initialize` and
//!   resources, and the `file://` URI helpers;
//! - [`tools`]: the tool catalog (`tools/list`) and each tool's behavior;
//! - [`workspace`]: the path-confinement and secret-file policy every read
//!   goes through (the security boundary of the crate).
//!
//! The server is single-threaded and synchronous: it handles one line at a
//! time and never writes anything except replies. Owning spec:
//! `crates/vix-mcp-server/spec/index.md` and the repo-level
//! `spec/mcp-server`.
//!
//! # Example
//!
//! ```
//! use std::path::{Path, PathBuf};
//! use vix_mcp_server::{Host, Server, Table, HostError};
//!
//! struct NoHost;
//! impl Host for NoHost {
//!     fn open_buffers(&self, _root: &Path) -> Vec<PathBuf> { Vec::new() }
//!     fn sqlite_schema(&self, _p: &Path) -> Option<Result<Vec<Table>, HostError>> { None }
//! }
//!
//! let dir = std::env::temp_dir();
//! let mut server = Server::new(&dir, NoHost, "0.0.0").unwrap();
//! let reply = server
//!     .handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#)
//!     .unwrap();
//! assert!(reply.contains(r#""result":{}"#));
//! ```

use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

pub mod tools;
pub mod workspace;

/// The MCP protocol version this server speaks natively.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// Every protocol version the server accepts and echoes back.
pub const SUPPORTED_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// JSON-RPC: the line is not valid JSON.
pub const PARSE_ERROR: i64 = -32700;
/// JSON-RPC: not a valid request object (or a batch).
pub const INVALID_REQUEST: i64 = -32600;
/// JSON-RPC: no such method.
pub const METHOD_NOT_FOUND: i64 = -32601;
/// JSON-RPC: bad parameters.
pub const INVALID_PARAMS: i64 = -32602;
/// A request other than `initialize`/`ping` arrived before `initialize`.
pub const NOT_INITIALIZED: i64 = -32002;

/// One column of a database table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    /// Column name.
    pub name: String,
    /// Declared type, as the database reports it (may be empty in `SQLite`).
    pub data_type: String,
}

/// One table or view of a database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    /// Table or view name.
    pub name: String,
    /// `"table"` or `"view"`.
    pub kind: String,
    /// Columns in declaration order.
    pub columns: Vec<Column>,
}

/// An error the [`Host`] reports; only its `Display` text reaches the client.
pub type HostError = Box<dyn std::error::Error + Send + Sync>;

/// A saved DB workbench connection the user shared with MCP clients (T783),
/// as `db_connections` shows it: never its host, user or password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedConnection {
    /// The connection's saved name; the other DB tools take it.
    pub name: String,
    /// `sqlite`, `postgres` or `mysql`.
    pub kind: String,
    /// `local`, `development`, `testing`, `staging` or `production`.
    pub environment: String,
    /// The database name (for `SQLite`, the file's name without its folder).
    pub database: String,
}

/// A capped result set: column names, rows as text, and whether rows were
/// cut off.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rows {
    /// Column names, in result order (empty when there are no rows).
    pub columns: Vec<String>,
    /// The rows, each cell as text (`NULL` is empty).
    pub rows: Vec<Vec<String>>,
    /// Whether the result had more rows than were returned.
    pub truncated: bool,
}

/// What `db_tables` returns for one shared connection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DbTables {
    /// The schema listed.
    pub schema: String,
    /// Every schema that holds an object.
    pub schemas: Vec<String>,
    /// `(name, kind)` of each table, view and routine in `schema`.
    pub objects: Vec<(String, String)>,
    /// Whether the catalog was cut off.
    pub truncated: bool,
}

/// What `db_describe` returns for one table: the engine's own catalog
/// reports, each as rows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DbDescription {
    /// The schema (empty for `MySQL`).
    pub schema: String,
    /// The table or view.
    pub table: String,
    /// Columns: names, types, nullability, defaults.
    pub columns: Rows,
    /// The primary-key columns, in key order.
    pub primary_key: Rows,
    /// Indexes.
    pub indexes: Rows,
    /// Foreign keys leaving the table.
    pub foreign_keys: Rows,
    /// Constraints.
    pub constraints: Rows,
}

/// What the App shell supplies to the protocol core.
pub trait Host {
    /// The files Vix last had open in the workspace at canonical `root`
    /// (any form; the server canonicalizes and filters them to the root).
    fn open_buffers(&self, root: &Path) -> Vec<PathBuf>;

    /// The tables and views of the `SQLite` database at `path`, opened
    /// read-only. `None` when this build cannot read databases at all.
    ///
    /// # Errors
    ///
    /// Any error whose `Display` text is shown to the client as the
    /// database's error.
    fn sqlite_schema(&self, path: &Path) -> Option<Result<Vec<Table>, HostError>>;

    /// The saved connections the user shared with MCP clients (T783), in
    /// saved order. `None` when this build cannot read databases at all.
    /// The default shares nothing.
    ///
    /// # Errors
    ///
    /// Any error reading the user's settings.
    fn shared_connections(&self) -> Option<Result<Vec<SharedConnection>, HostError>> {
        None
    }

    /// `db_tables` on the shared connection named `connection`: the objects
    /// of `schema` (or its default schema). `None` without database support.
    ///
    /// The host must refuse a connection that is not shared exactly as one
    /// that does not exist, log the call before opening anything, and open
    /// the connection read-only without prompting.
    ///
    /// # Errors
    ///
    /// Any error whose `Display` text is shown to the client.
    fn db_tables(
        &self,
        connection: &str,
        schema: Option<&str>,
    ) -> Option<Result<DbTables, HostError>> {
        let _ = (connection, schema);
        None
    }

    /// `db_describe` on a shared connection (see [`Host::db_tables`] for the
    /// host's duties).
    ///
    /// # Errors
    ///
    /// Any error whose `Display` text is shown to the client.
    fn db_describe(
        &self,
        connection: &str,
        schema: Option<&str>,
        table: &str,
    ) -> Option<Result<DbDescription, HostError>> {
        let _ = (connection, schema, table);
        None
    }

    /// `db_query` on a shared connection: one read-only statement, at most
    /// 100 rows (see [`Host::db_tables`] for the host's duties; the host
    /// also checks the statement).
    ///
    /// # Errors
    ///
    /// Any error whose `Display` text is shown to the client, including a
    /// refused statement.
    fn db_query(&self, connection: &str, sql: &str) -> Option<Result<Rows, HostError>> {
        let _ = (connection, sql);
        None
    }
}

/// The protocol state for one stdio session.
///
/// The only mutable state is the `initialized` flag: per the MCP lifecycle,
/// every method except `initialize` and `ping` is refused with
/// [`NOT_INITIALIZED`] until `initialize` has been answered.
#[derive(Debug)]
pub struct Server<H: Host> {
    /// Canonical workspace root.
    root: PathBuf,
    /// The App shell's data source.
    host: H,
    /// Whether `initialize` has been answered.
    initialized: bool,
    /// The server's own version, reported in `serverInfo`.
    version: String,
}

impl<H: Host> Server<H> {
    /// A server for the workspace at `root` (canonicalized here), reporting
    /// `version` in `serverInfo`.
    ///
    /// # Errors
    ///
    /// Returns the I/O error when `root` cannot be canonicalized, or
    /// [`io::ErrorKind::InvalidInput`] when it is not a directory.
    pub fn new(root: &Path, host: H, version: &str) -> io::Result<Self> {
        let root = root.canonicalize()?;
        if !root.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} is not a directory", root.display()),
            ));
        }
        Ok(Self {
            root,
            host,
            initialized: false,
            version: version.to_string(),
        })
    }

    /// The canonical workspace root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Handle one line of input, returning the line to write back (without
    /// its newline), or `None` for a notification or a blank line.
    ///
    /// Invalid JSON yields a [`PARSE_ERROR`] reply with a `null` id, as
    /// JSON-RPC requires when the id cannot be recovered.
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        if line.trim().is_empty() {
            return None;
        }
        let reply = match serde_json::from_str::<Value>(line) {
            Ok(message) => self.handle(&message)?,
            Err(e) => error(&Value::Null, PARSE_ERROR, &format!("parse error: {e}")),
        };
        Some(reply.to_string())
    }

    /// Handle one decoded message, returning the response, or `None` for a
    /// notification.
    ///
    /// Batches (JSON arrays) are not supported and get [`INVALID_REQUEST`].
    pub fn handle(&mut self, message: &Value) -> Option<Value> {
        let Some(object) = message.as_object() else {
            return Some(error(
                &Value::Null,
                INVALID_REQUEST,
                "expected a single JSON-RPC 2.0 object (batches are not supported)",
            ));
        };
        // A message without an `id` is a notification by definition.
        let id = object.get("id").cloned();
        let method = object.get("method").and_then(Value::as_str);
        // JSON-RPC ids may be strings or integers (null and fractions are
        // rejected, since MCP forbids them).
        let valid = object.get("jsonrpc").and_then(Value::as_str) == Some("2.0")
            && method.is_some()
            && id
                .as_ref()
                .is_none_or(|i| i.is_string() || i.is_i64() || i.is_u64());
        let Some(id) = id else {
            // A notification (including `notifications/initialized`): never
            // answered, even when malformed.
            return None;
        };
        let Some(method) = method.filter(|_| valid) else {
            return Some(error(&id, INVALID_REQUEST, "invalid JSON-RPC 2.0 request"));
        };
        let params = object.get("params").cloned().unwrap_or(Value::Null);
        Some(match self.dispatch(method, &params) {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Err((code, message)) => error(&id, code, &message),
        })
    }

    /// Route one request to its method.
    fn dispatch(&mut self, method: &str, params: &Value) -> Result<Value, (i64, String)> {
        match method {
            "initialize" => Ok(self.initialize(params)),
            "ping" => Ok(json!({})),
            // Guard arm: everything below needs a completed handshake.
            _ if !self.initialized => Err((NOT_INITIALIZED, "server not initialized".into())),
            "tools/list" => Ok(tools::list()),
            "tools/call" => self.call_tool(params),
            "resources/list" => Ok(self.list_resources()),
            "resources/read" => self.read_resource(params),
            other => Err((METHOD_NOT_FOUND, format!("method not found: {other}"))),
        }
    }

    /// `initialize`: agree a protocol version and describe the server.
    fn initialize(&mut self, params: &Value) -> Value {
        self.initialized = true;
        let requested = params.get("protocolVersion").and_then(Value::as_str);
        // Echo the client's version when we know it; otherwise offer our own
        // and let the client decide whether to continue (MCP negotiation).
        let version = requested
            .filter(|v| SUPPORTED_VERSIONS.contains(v))
            .unwrap_or(PROTOCOL_VERSION);
        json!({
            "protocolVersion": version,
            "capabilities": {
                "tools": {"listChanged": false},
                "resources": {"listChanged": false, "subscribe": false},
            },
            "serverInfo": {"name": "vix", "title": "Vix", "version": self.version},
            "instructions": "Read-only view of one Vix workspace: the files Vix last had open, \
                their symbols and outline, offline diagnostics, and SQLite schemas. \
                Paths are relative to the workspace root. The db_connections, db_tables, \
                db_describe and db_query tools reach only the saved database connections \
                the user shared with MCP clients, read-only, and every call is logged.",
        })
    }

    /// `tools/call`: run a tool; tool failures are `isError` results.
    fn call_tool(&self, params: &Value) -> Result<Value, (i64, String)> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or((INVALID_PARAMS, "missing tool name".to_string()))?;
        // Unknown tool names are protocol errors; a *known* tool failing is
        // reported in-band as `isError`, so the model can see and react.
        if !tools::list()["tools"]
            .as_array()
            .is_some_and(|all| all.iter().any(|t| t["name"] == name))
        {
            return Err((INVALID_PARAMS, format!("unknown tool: {name}")));
        }
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        Ok(match tools::call(&self.root, &self.host, name, &args) {
            Ok(result) => json!({
                "content": [{"type": "text", "text": result.to_string()}],
                // Both forms: text for clients that only show content,
                // structured data for those that parse it.
                "structuredContent": result,
                "isError": false,
            }),
            Err(error) => json!({
                "content": [{"type": "text", "text": error.to_string()}],
                "isError": true,
            }),
        })
    }

    /// `resources/list`: one `file://` resource per open buffer.
    fn list_resources(&self) -> Value {
        let resources: Vec<Value> = tools::open_buffers(&self.root, &self.host)
            .iter()
            .map(|path| {
                json!({
                    "uri": file_uri(path),
                    "name": workspace::relative(&self.root, path),
                    "mimeType": "text/plain",
                })
            })
            .collect();
        json!({"resources": resources})
    }

    /// `resources/read`: the text of a `file://` URI inside the root.
    fn read_resource(&self, params: &Value) -> Result<Value, (i64, String)> {
        let uri = params
            .get("uri")
            .and_then(Value::as_str)
            .ok_or((INVALID_PARAMS, "missing uri".to_string()))?;
        let path = uri_path(uri).ok_or((INVALID_PARAMS, format!("not a file:// URI: {uri}")))?;
        let (_, text) =
            workspace::read_text(&self.root, &path).map_err(|e| (INVALID_PARAMS, e.to_string()))?;
        Ok(json!({"contents": [{"uri": uri, "mimeType": "text/plain", "text": text}]}))
    }
}

/// Serve `server` over `input`/`output` until `input` ends: one JSON-RPC
/// message per line in, one per line out, flushed after each reply.
///
/// # Errors
///
/// Returns the first read or write error.
pub fn serve<H: Host, R: BufRead, W: Write>(
    server: &mut Server<H>,
    input: R,
    mut output: W,
) -> io::Result<()> {
    for line in input.lines() {
        if let Some(reply) = server.handle_line(&line?) {
            writeln!(output, "{reply}")?;
            output.flush()?;
        }
    }
    Ok(())
}

/// A JSON-RPC error response. `id` is `null` when the request id could not
/// be determined.
fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// `file://` URI for an absolute path, percent-encoding everything but
/// unreserved characters and `/`. On Windows a verbatim prefix (`\\?\`) is
/// dropped, backslashes are separators and a drive path gets a leading `/`
/// (`file:///C:/…`); see [`vix_lsp_core::uri`].
///
/// ```
/// use std::path::Path;
/// assert_eq!(
///     vix_mcp_server::file_uri(Path::new("/a b/c.rs")),
///     "file:///a%20b/c.rs"
/// );
/// ```
#[must_use]
pub fn file_uri(path: &Path) -> String {
    vix_lsp_core::uri::file_uri_native(path)
}

/// The path a `file://` URI names, percent-decoded; `None` for another
/// scheme, a remote host, or bad encoding.
///
/// The inverse of [`file_uri`]. The result is *not* confined to the
/// workspace; callers must pass it through [`workspace`] before reading.
///
/// ```
/// assert_eq!(
///     vix_mcp_server::uri_path("file:///a%20b/c.rs").as_deref(),
///     Some("/a b/c.rs")
/// );
/// assert_eq!(vix_mcp_server::uri_path("https://x/y"), None);
/// ```
#[must_use]
pub fn uri_path(uri: &str) -> Option<String> {
    // Only a local file: an empty host or `localhost`.
    let (host, _) = vix_lsp_core::uri::split_file_uri(uri)?;
    if !(host.is_empty() || host == "localhost") {
        return None;
    }
    vix_lsp_core::uri::uri_path(uri, cfg!(windows))
}

#[cfg(test)]
mod tests;
