//! Language Server Protocol client: process management and document sync layered
//! over the pure [`crate::lsp_core`] protocol crate.
//!
//! This module owns the IO the protocol crate deliberately avoids: it launches a
//! configured server per language, reads its framed stdout on a background thread
//! (into an `mpsc` channel, like the run-command feature), writes requests to its
//! stdin, and tracks open-document versions and in-flight requests. [`Lsp::poll`]
//! is drained once per event-loop iteration and returns [`LspEvent`]s for the host
//! to act on (refresh diagnostics, show a hover, jump to a definition, open the
//! completion list).
//!
//! Positions cross this boundary as raw LSP `(line, character)` pairs — the host
//! converts them to/from char offsets with [`crate::lsp_core::position`], since only it
//! holds the buffer text.

#![warn(clippy::pedantic)]

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};

use serde_json::{json, Value};

use crate::settings::LspServer as ServerConfig;
use crate::lsp_core::{frame, message, Diagnostic, Encoding};

/// The reserved JSON-RPC id of the `initialize` request.
const INITIALIZE_ID: i64 = 1;

/// What a pending request's response should drive once it arrives.
#[derive(Clone, Copy, Debug)]
enum Pending {
    Hover,
    Definition,
    Completion,
    References,
}

/// A message handed back from a server's stdout reader thread.
enum Incoming {
    Message(Value),
    Exited,
}

/// Something a server told us, surfaced to the host from [`Lsp::poll`].
pub enum LspEvent {
    /// Diagnostics for `path` were (re)published; the host should rebuild marks.
    Diagnostics(PathBuf),
    /// A hover response: the text to show, for the request the user just made.
    Hover(String),
    /// A go-to-definition response: jump to this file/line/character (0-based).
    Definition {
        /// Target file.
        path: PathBuf,
        /// Zero-based target line.
        line: u32,
        /// Target column, in the server's encoding units.
        character: u32,
    },
    /// A completion response: candidates to offer.
    Completion(Vec<crate::lsp_core::CompletionItem>),
    /// A references response: every (file, 0-based line, character) location.
    References(Vec<(PathBuf, u32, u32)>),
}

/// One running language server.
struct Server {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Incoming>,
    /// Next id for a client→server request.
    next_id: i64,
    /// In-flight requests awaiting a response.
    pending: HashMap<i64, Pending>,
    /// Open documents, `uri` → last sent version.
    docs: HashMap<String, i64>,
    /// Position encoding negotiated at `initialize` (default UTF-16).
    encoding: Encoding,
    /// Whether `initialize` has completed and `initialized` been sent.
    ready: bool,
    /// Messages deferred until the server is `ready`.
    queue: Vec<Value>,
}

impl Server {
    /// Write a framed message to the server's stdin now (no readiness gate).
    fn write_now(&mut self, msg: &Value) {
        let _ = self.stdin.write_all(&frame::encode(msg));
        let _ = self.stdin.flush();
    }

    /// Send `msg`, or queue it until `initialize` completes.
    fn send(&mut self, msg: Value) {
        if self.ready {
            self.write_now(&msg);
        } else {
            self.queue.push(msg);
        }
    }

    /// Allocate the next request id.
    fn alloc_id(&mut self) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

/// The LSP client: configured servers, the ones currently running, and the latest
/// diagnostics per file.
pub struct Lsp {
    enabled: bool,
    configs: Vec<ServerConfig>,
    /// Running servers keyed by language id.
    servers: HashMap<String, Server>,
    /// `rootUri` sent at initialize (the workspace root).
    root_uri: Option<String>,
    /// Latest diagnostics keyed by canonical file path.
    diagnostics: HashMap<PathBuf, Vec<Diagnostic>>,
}

impl Lsp {
    /// Build a client from the persisted settings and the workspace root.
    #[must_use]
    pub fn new(enabled: bool, configs: Vec<ServerConfig>, root: &Path) -> Self {
        Lsp {
            enabled,
            configs,
            servers: HashMap::new(),
            root_uri: Some(path_to_uri(root)),
            diagnostics: HashMap::new(),
        }
    }

    /// Whether LSP is on and at least one server is configured.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.enabled && !self.configs.is_empty()
    }

    /// The config handling `path` (matched by extension), if any.
    fn config_for(&self, path: &Path) -> Option<ServerConfig> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        self.configs
            .iter()
            .find(|c| c.extensions.iter().any(|e| e.eq_ignore_ascii_case(&ext)))
            .cloned()
    }

    /// The position encoding for `path`'s server (UTF-16 if none / not ready).
    #[must_use]
    pub fn encoding_for(&self, path: &Path) -> Encoding {
        self.config_for(path)
            .and_then(|c| self.servers.get(&c.language_id))
            .map_or(Encoding::Utf16, |s| s.encoding)
    }

    /// Whether a server handles `path` (so the host should prefer LSP features).
    #[must_use]
    pub fn handles(&self, path: &Path) -> bool {
        self.enabled && self.config_for(path).is_some()
    }

    /// Diagnostics for `path`, or an empty slice.
    #[must_use]
    pub fn diagnostics_for(&self, path: &Path) -> &[Diagnostic] {
        let key = canonical(path);
        self.diagnostics.get(&key).map_or(&[], Vec::as_slice)
    }

    /// Every file's diagnostics (path, list), for the diagnostics panel. Files
    /// with no current diagnostics are skipped.
    pub fn all_diagnostics(&self) -> impl Iterator<Item = (&PathBuf, &Vec<Diagnostic>)> {
        self.diagnostics.iter().filter(|(_, d)| !d.is_empty())
    }

    /// Total diagnostic count across all files (for the status bar).
    #[must_use]
    pub fn diagnostic_count(&self) -> usize {
        self.diagnostics.values().map(Vec::len).sum()
    }

    /// Whether any server is still starting up or has a request in flight, so the
    /// event loop should tick faster to deliver the response promptly.
    #[must_use]
    pub fn busy(&self) -> bool {
        self.servers.values().any(|s| !s.ready || !s.pending.is_empty())
    }

    /// Launch (if needed) and return the server for `lang`, or `None` if it could
    /// not be spawned.
    fn ensure_server(&mut self, config: &ServerConfig) -> Option<&mut Server> {
        if !self.servers.contains_key(&config.language_id) {
            let server = spawn(config, self.root_uri.as_deref())?;
            self.servers.insert(config.language_id.clone(), server);
        }
        self.servers.get_mut(&config.language_id)
    }

    /// Notify the server that `path` opened, with its current `text`.
    pub fn did_open(&mut self, path: &Path, text: &str) {
        if !self.enabled {
            return;
        }
        let Some(config) = self.config_for(path) else { return };
        let uri = path_to_uri(path);
        let lang = config.language_id.clone();
        let Some(server) = self.ensure_server(&config) else { return };
        if server.docs.contains_key(&uri) {
            return; // already open
        }
        server.docs.insert(uri.clone(), 1);
        server.send(message::notification(
            "textDocument/didOpen",
            &message::did_open_params(&uri, &lang, 1, text),
        ));
    }

    /// Notify the server that `path`'s buffer changed (full-document sync).
    pub fn did_change(&mut self, path: &Path, text: &str) {
        if !self.enabled {
            return;
        }
        let Some(config) = self.config_for(path) else { return };
        let uri = path_to_uri(path);
        let Some(server) = self.servers.get_mut(&config.language_id) else { return };
        let Some(version) = server.docs.get_mut(&uri) else { return };
        *version += 1;
        let v = *version;
        server.send(message::notification(
            "textDocument/didChange",
            &message::did_change_full_params(&uri, v, text),
        ));
    }

    /// Notify the server that `path` closed.
    pub fn did_close(&mut self, path: &Path) {
        if !self.enabled {
            return;
        }
        let Some(config) = self.config_for(path) else { return };
        let uri = path_to_uri(path);
        if let Some(server) = self.servers.get_mut(&config.language_id)
            && server.docs.remove(&uri).is_some() {
                server.send(message::notification(
                    "textDocument/didClose",
                    &message::did_close_params(&uri),
                ));
            }
    }

    /// Send a feature request for `path` at `(line, character)`; the response
    /// arrives later via [`Lsp::poll`] as the matching [`LspEvent`].
    fn request(&mut self, path: &Path, method: &str, line: u32, character: u32, kind: Pending) {
        let Some(config) = self.config_for(path) else { return };
        let uri = path_to_uri(path);
        let Some(server) = self.servers.get_mut(&config.language_id) else { return };
        if !server.docs.contains_key(&uri) {
            return; // only query open documents
        }
        let id = server.alloc_id();
        server.pending.insert(id, kind);
        server.send(message::request(id, method, &message::position_params(&uri, line, character)));
    }

    /// Request hover info at `(line, character)`.
    pub fn request_hover(&mut self, path: &Path, line: u32, character: u32) {
        self.request(path, "textDocument/hover", line, character, Pending::Hover);
    }

    /// Request the definition location at `(line, character)`.
    pub fn request_definition(&mut self, path: &Path, line: u32, character: u32) {
        self.request(path, "textDocument/definition", line, character, Pending::Definition);
    }

    /// Request completion candidates at `(line, character)`.
    pub fn request_completion(&mut self, path: &Path, line: u32, character: u32) {
        self.request(path, "textDocument/completion", line, character, Pending::Completion);
    }

    /// Request the implementation location(s) at `(line, character)` (jumps to
    /// the first, like definition).
    pub fn request_implementation(&mut self, path: &Path, line: u32, character: u32) {
        self.request(path, "textDocument/implementation", line, character, Pending::Definition);
    }

    /// Request the type-definition location at `(line, character)`.
    pub fn request_type_definition(&mut self, path: &Path, line: u32, character: u32) {
        self.request(path, "textDocument/typeDefinition", line, character, Pending::Definition);
    }

    /// Request all references to the symbol at `(line, character)`.
    pub fn request_references(&mut self, path: &Path, line: u32, character: u32) {
        let Some(config) = self.config_for(path) else { return };
        let uri = path_to_uri(path);
        let Some(server) = self.servers.get_mut(&config.language_id) else { return };
        if !server.docs.contains_key(&uri) {
            return;
        }
        let id = server.alloc_id();
        server.pending.insert(id, Pending::References);
        server.send(message::request(
            id,
            "textDocument/references",
            &message::reference_params(&uri, line, character, true),
        ));
    }

    /// Notify the server that `path` was saved (full text), to trigger
    /// re-analysis.
    pub fn did_save(&mut self, path: &Path, text: &str) {
        if !self.enabled {
            return;
        }
        let Some(config) = self.config_for(path) else { return };
        let uri = path_to_uri(path);
        if let Some(server) = self.servers.get_mut(&config.language_id)
            && server.docs.contains_key(&uri)
        {
            server.send(message::notification(
                "textDocument/didSave",
                &message::did_save_params(&uri, text),
            ));
        }
    }

    /// Drain every server's inbox, updating diagnostics and collecting events.
    /// Called once per event-loop iteration; cheap when nothing is in flight.
    pub fn poll(&mut self) -> Vec<LspEvent> {
        let mut events = Vec::new();
        let langs: Vec<String> = self.servers.keys().cloned().collect();
        for lang in langs {
            // Drain this server's channel into owned messages first, so the
            // borrow ends before we touch `self.diagnostics`.
            let drained: Vec<Incoming> = {
                let Some(server) = self.servers.get(&lang) else { continue };
                let mut v = Vec::new();
                while let Ok(m) = server.rx.try_recv() {
                    v.push(m);
                }
                v
            };
            for incoming in drained {
                match incoming {
                    Incoming::Exited => {
                        self.servers.remove(&lang);
                        break;
                    }
                    Incoming::Message(msg) => self.handle(&lang, &msg, &mut events),
                }
            }
        }
        events
    }

    /// Handle one decoded message from server `lang`.
    fn handle(&mut self, lang: &str, msg: &Value, events: &mut Vec<LspEvent>) {
        let has_method = msg.get("method").and_then(Value::as_str);
        let has_id = msg.get("id").is_some();

        // Server → client request (has both id and method): reply minimally.
        if let (Some(method), true) = (has_method, has_id) {
            self.reply_to_server_request(lang, msg, method);
            return;
        }
        // Notification (method, no id).
        if let Some(method) = has_method {
            if method == "textDocument/publishDiagnostics"
                && let Some(params) = msg.get("params")
                    && let Some((uri, diags)) = message::parse_diagnostics(params) {
                        let path = canonical(&uri_to_path(&uri));
                        if diags.is_empty() {
                            self.diagnostics.remove(&path);
                        } else {
                            self.diagnostics.insert(path.clone(), diags);
                        }
                        events.push(LspEvent::Diagnostics(path));
                    }
            return;
        }
        // Response to one of our requests (id, no method).
        if let Some(id) = msg.get("id").and_then(Value::as_i64) {
            self.handle_response(lang, id, msg, events);
        }
    }

    fn handle_response(&mut self, lang: &str, id: i64, msg: &Value, events: &mut Vec<LspEvent>) {
        if id == INITIALIZE_ID {
            self.finish_initialize(lang, msg);
            return;
        }
        let Some(server) = self.servers.get_mut(lang) else { return };
        let Some(kind) = server.pending.remove(&id) else { return };
        let Some(result) = msg.get("result") else { return };
        if result.is_null() {
            return;
        }
        match kind {
            Pending::Hover => {
                if let Some(text) = message::parse_hover(result) {
                    events.push(LspEvent::Hover(text));
                }
            }
            Pending::Definition => {
                if let Some(loc) = message::parse_definition(result) {
                    events.push(LspEvent::Definition {
                        path: uri_to_path(&loc.uri),
                        line: loc.range.start.line,
                        character: loc.range.start.character,
                    });
                }
            }
            Pending::Completion => {
                let items = message::parse_completion(result);
                if !items.is_empty() {
                    events.push(LspEvent::Completion(items));
                }
            }
            Pending::References => {
                let locs: Vec<(PathBuf, u32, u32)> = message::parse_locations(result)
                    .into_iter()
                    .map(|l| (uri_to_path(&l.uri), l.range.start.line, l.range.start.character))
                    .collect();
                if !locs.is_empty() {
                    events.push(LspEvent::References(locs));
                }
            }
        }
    }

    /// On the `initialize` response: record the encoding, send `initialized`, and
    /// flush everything queued during startup.
    fn finish_initialize(&mut self, lang: &str, msg: &Value) {
        let Some(server) = self.servers.get_mut(lang) else { return };
        if let Some(result) = msg.get("result") {
            server.encoding = message::parse_position_encoding(result);
        }
        server.ready = true;
        server.write_now(&message::notification("initialized", &json!({})));
        let queued = std::mem::take(&mut server.queue);
        for m in queued {
            server.write_now(&m);
        }
    }

    /// Reply to a server-initiated request so it does not stall. We accept no
    /// dynamic capabilities and supply empty configuration.
    fn reply_to_server_request(&mut self, lang: &str, msg: &Value, method: &str) {
        let Some(server) = self.servers.get_mut(lang) else { return };
        let id = msg.get("id").cloned().unwrap_or(Value::Null);
        let result = if method == "workspace/configuration" {
            let n = msg
                .get("params")
                .and_then(|p| p.get("items"))
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            Value::Array(vec![Value::Null; n])
        } else {
            Value::Null
        };
        server.write_now(&json!({ "jsonrpc": "2.0", "id": id, "result": result }));
    }

    /// Politely shut down every server (best-effort; called on exit).
    pub fn shutdown(&mut self) {
        for (_, mut server) in self.servers.drain() {
            server.write_now(&message::request(server.next_id, "shutdown", &Value::Null));
            server.write_now(&message::notification("exit", &Value::Null));
            let _ = server.child.kill();
        }
    }
}

/// Spawn a server process and its stdout reader thread.
fn spawn(config: &ServerConfig, root_uri: Option<&str>) -> Option<Server> {
    let (program, args) = config.command.split_first()?;
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdin = child.stdin.take()?;
    let stdout = child.stdout.take()?;
    let (tx, rx) = channel();
    spawn_reader(stdout, tx);
    let mut server = Server {
        child,
        stdin,
        rx,
        next_id: INITIALIZE_ID + 1,
        pending: HashMap::new(),
        docs: HashMap::new(),
        encoding: Encoding::Utf16,
        ready: false,
        queue: Vec::new(),
    };
    let init = message::request(
        INITIALIZE_ID,
        "initialize",
        &message::initialize_params(Some(std::process::id()), root_uri),
    );
    server.write_now(&init);
    Some(server)
}

/// Read framed messages off `stdout` and forward each decoded value to `tx`.
fn spawn_reader(mut stdout: std::process::ChildStdout, tx: Sender<Incoming>) {
    std::thread::spawn(move || {
        let mut decoder = frame::Decoder::new();
        let mut chunk = [0u8; 8192];
        loop {
            match stdout.read(&mut chunk) {
                Ok(0) | Err(_) => {
                    let _ = tx.send(Incoming::Exited);
                    return;
                }
                Ok(n) => {
                    decoder.push(&chunk[..n]);
                    while let Some(msg) = decoder.pop() {
                        if tx.send(Incoming::Message(msg)).is_err() {
                            return; // host dropped the receiver
                        }
                    }
                }
            }
        }
    });
}

/// Canonicalize a path for diagnostic keying, falling back to the path as-is.
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Build a `file://` URI from a filesystem path, percent-encoding each segment.
#[must_use]
pub fn path_to_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    let s = path.to_string_lossy();
    for byte in s.bytes() {
        match byte {
            b'/' => uri.push('/'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                uri.push(byte as char);
            }
            _ => {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                uri.push('%');
                uri.push(HEX[(byte >> 4) as usize] as char);
                uri.push(HEX[(byte & 0x0f) as usize] as char);
            }
        }
    }
    uri
}

/// Turn a `file://` URI back into a filesystem path (percent-decoded).
#[must_use]
pub fn uri_to_path(uri: &str) -> PathBuf {
    let rest = uri.strip_prefix("file://").unwrap_or(uri);
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&rest[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        out.push(bytes[i]);
        i += 1;
    }
    PathBuf::from(String::from_utf8_lossy(&out).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_round_trips_paths_with_spaces() {
        let p = PathBuf::from("/tmp/my dir/file.rs");
        let uri = path_to_uri(&p);
        assert_eq!(uri, "file:///tmp/my%20dir/file.rs");
        assert_eq!(uri_to_path(&uri), p);
    }

    #[test]
    fn config_matches_by_extension_case_insensitively() {
        let cfg = ServerConfig {
            language_id: "rust".into(),
            extensions: vec!["rs".into()],
            command: vec!["rust-analyzer".into()],
        };
        let lsp = Lsp::new(true, vec![cfg], Path::new("/proj"));
        assert!(lsp.config_for(Path::new("/proj/src/main.RS")).is_some());
        assert!(lsp.config_for(Path::new("/proj/readme.md")).is_none());
        assert!(lsp.handles(Path::new("/proj/a.rs")));
    }

    #[test]
    fn disabled_or_unconfigured_is_inactive() {
        assert!(!Lsp::new(false, vec![], Path::new("/")).is_active());
        assert!(!Lsp::new(true, vec![], Path::new("/")).is_active());
    }
}
