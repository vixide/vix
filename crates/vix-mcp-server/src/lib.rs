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

use serde_json::{Value, json};
use std::fmt::Write as _;
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
}

/// The protocol state for one stdio session.
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
    pub fn handle(&mut self, message: &Value) -> Option<Value> {
        let Some(object) = message.as_object() else {
            return Some(error(
                &Value::Null,
                INVALID_REQUEST,
                "expected a single JSON-RPC 2.0 object (batches are not supported)",
            ));
        };
        let id = object.get("id").cloned();
        let method = object.get("method").and_then(Value::as_str);
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
                Paths are relative to the workspace root.",
        })
    }

    /// `tools/call`: run a tool; tool failures are `isError` results.
    fn call_tool(&self, params: &Value) -> Result<Value, (i64, String)> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or((INVALID_PARAMS, "missing tool name".to_string()))?;
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
        let (_, text) = workspace::read_text(&self.root, &path).map_err(|e| (INVALID_PARAMS, e.to_string()))?;
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

/// A JSON-RPC error response.
fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// `file://` URI for an absolute path, percent-encoding everything but
/// unreserved characters and `/`.
#[must_use]
pub fn file_uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file://");
    if !text.starts_with('/') {
        out.push('/');
    }
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// The path a `file://` URI names, percent-decoded; `None` for another
/// scheme, a remote host, or bad encoding.
#[must_use]
pub fn uri_path(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    // Only a local file: an empty host or `localhost`.
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    if !rest.starts_with('/') {
        return None;
    }
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = rest.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests;
