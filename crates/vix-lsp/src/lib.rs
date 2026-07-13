//! Language Server Protocol client: process management and document sync layered
//! over the pure [`vix_lsp_core`] protocol crate.
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
//! converts them to/from char offsets with [`vix_lsp_core::position`], since only it
//! holds the buffer text.

#![warn(clippy::pedantic)]

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};

use serde_json::{Value, json};

use vix_lsp_core::{Diagnostic, Encoding, frame, message};
use vix_settings::LspServer as ServerConfig;

/// The reserved JSON-RPC id of the `initialize` request.
const INITIALIZE_ID: i64 = 1;

/// What a pending request's response should drive once it arrives.
#[derive(Clone, Copy, Debug)]
enum Pending {
    Hover,
    Definition,
    Completion,
    References,
    Formatting,
    DocumentSymbols,
    WorkspaceSymbols,
    SignatureHelp,
    Rename,
    CodeAction,
    SelectionRange,
    DocumentHighlight,
    FoldingRange,
    CompletionResolve,
    InlayHint,
    LinkedEditing,
    CodeLens,
    PrepareCallHierarchy,
    IncomingCalls,
}

/// A message handed back from a server's stdout reader thread.
enum Incoming {
    Message(Value),
    Exited,
}

/// One file's edits in a workspace/code-action edit: `(file, [(range, new_text)])`.
pub type FileEdits = (PathBuf, Vec<(vix_lsp_core::Range, String)>);
/// One offered code action with its edit: `(title, [per-file edits])`.
pub type CodeAction = (String, Vec<FileEdits>);

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
    Completion(Vec<vix_lsp_core::CompletionItem>),
    /// A references response: every (file, 0-based line, character) location.
    References(Vec<(PathBuf, u32, u32)>),
    /// A `prepareCallHierarchy` result: the symbol item to query calls for.
    CallHierarchyPrepared(serde_json::Value),
    /// A formatting response: text edits (range + replacement) for the active
    /// file, in document order.
    Edits(Vec<(vix_lsp_core::Range, String)>),
    /// A document-symbol response for the active file: `(0-based line, character,
    /// name)`.
    DocumentSymbols(Vec<(u32, u32, String)>),
    /// A workspace-symbol response: `(file, 0-based line, character, name)`.
    WorkspaceSymbols(Vec<(PathBuf, u32, u32, String)>),
    /// A signature-help response: the text to show in a popup.
    SignatureHelp(String),
    /// A rename response: per-file edits to apply.
    WorkspaceEdit(Vec<FileEdits>),
    /// A code-action response: each offered action with its edit.
    CodeActions(Vec<CodeAction>),
    /// A selection-range response: the chain of ranges (innermost first) for the
    /// cursor, used to expand/shrink the selection.
    SelectionRanges(Vec<vix_lsp_core::Range>),
    /// A document-highlight response: ranges of the symbol's occurrences in the
    /// active file.
    Highlights(Vec<vix_lsp_core::Range>),
    /// A folding-range response: foldable `(start_line, end_line)` ranges for the
    /// active file.
    FoldingRanges(Vec<(u32, u32)>),
    /// A completion-resolve response: fuller detail/documentation for the
    /// in-flight completion item.
    CompletionDetail(String),
    /// An inlay-hint response: `(line, character, label)` hints for the active
    /// file (0-based; `character` in the server's encoding units).
    InlayHints(Vec<(u32, u32, String)>),
    /// A linked-editing response: ranges in the active file that should be edited
    /// together (e.g. an open/close tag pair).
    LinkedRanges(Vec<vix_lsp_core::Range>),
    /// A code-lens response: invokable lenses `(line, title, command, arguments)`.
    CodeLenses(Vec<vix_lsp_core::message::CodeLens>),
}

/// One running language server.
struct Server {
    child: Child,
    /// Framed messages to write to the server's stdin, drained by a dedicated
    /// writer thread so a stalled server can never block the UI thread.
    writer: Sender<Vec<u8>>,
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
    /// Hand a framed message to the writer thread (no readiness gate).
    ///
    /// Document sync sends the whole buffer on every change; writing it to the
    /// server's stdin directly from the event loop would block the UI thread
    /// once a stalled server fills the OS pipe buffer. Sending to the writer
    /// channel is non-blocking; a closed channel (writer gone) is ignored.
    fn write_now(&mut self, msg: &Value) {
        let _ = self.writer.send(frame::encode(msg));
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
        self.servers
            .values()
            .any(|s| !s.ready || !s.pending.is_empty())
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
        let Some(config) = self.config_for(path) else {
            return;
        };
        let uri = path_to_uri(path);
        let lang = config.language_id.clone();
        let Some(server) = self.ensure_server(&config) else {
            return;
        };
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
        let Some(config) = self.config_for(path) else {
            return;
        };
        let uri = path_to_uri(path);
        let Some(server) = self.servers.get_mut(&config.language_id) else {
            return;
        };
        let Some(version) = server.docs.get_mut(&uri) else {
            return;
        };
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
        let Some(config) = self.config_for(path) else {
            return;
        };
        let uri = path_to_uri(path);
        if let Some(server) = self.servers.get_mut(&config.language_id)
            && server.docs.remove(&uri).is_some()
        {
            server.send(message::notification(
                "textDocument/didClose",
                &message::did_close_params(&uri),
            ));
        }
    }

    /// Send a feature request for `path` at `(line, character)`; the response
    /// arrives later via [`Lsp::poll`] as the matching [`LspEvent`].
    fn request(&mut self, path: &Path, method: &str, line: u32, character: u32, kind: Pending) {
        let Some(config) = self.config_for(path) else {
            return;
        };
        let uri = path_to_uri(path);
        let Some(server) = self.servers.get_mut(&config.language_id) else {
            return;
        };
        if !server.docs.contains_key(&uri) {
            return; // only query open documents
        }
        let id = server.alloc_id();
        server.pending.insert(id, kind);
        server.send(message::request(
            id,
            method,
            &message::position_params(&uri, line, character),
        ));
    }

    /// Request hover info at `(line, character)`.
    pub fn request_hover(&mut self, path: &Path, line: u32, character: u32) {
        self.request(path, "textDocument/hover", line, character, Pending::Hover);
    }

    /// Request the definition location at `(line, character)`.
    pub fn request_definition(&mut self, path: &Path, line: u32, character: u32) {
        self.request(
            path,
            "textDocument/definition",
            line,
            character,
            Pending::Definition,
        );
    }

    /// Request completion candidates at `(line, character)`.
    pub fn request_completion(&mut self, path: &Path, line: u32, character: u32) {
        self.request(
            path,
            "textDocument/completion",
            line,
            character,
            Pending::Completion,
        );
    }

    /// Request the implementation location(s) at `(line, character)` (jumps to
    /// the first, like definition).
    pub fn request_implementation(&mut self, path: &Path, line: u32, character: u32) {
        self.request(
            path,
            "textDocument/implementation",
            line,
            character,
            Pending::Definition,
        );
    }

    /// Request the type-definition location at `(line, character)`.
    pub fn request_type_definition(&mut self, path: &Path, line: u32, character: u32) {
        self.request(
            path,
            "textDocument/typeDefinition",
            line,
            character,
            Pending::Definition,
        );
    }

    /// Request the declaration location at `(line, character)` (jumps like
    /// definition).
    pub fn request_declaration(&mut self, path: &Path, line: u32, character: u32) {
        self.request(
            path,
            "textDocument/declaration",
            line,
            character,
            Pending::Definition,
        );
    }

    /// Step 1 of call hierarchy: prepare the symbol at `(line, character)`. The
    /// response (`LspEvent::CallHierarchyPrepared`) carries the item to query.
    pub fn request_prepare_call_hierarchy(&mut self, path: &Path, line: u32, character: u32) {
        self.request(
            path,
            "textDocument/prepareCallHierarchy",
            line,
            character,
            Pending::PrepareCallHierarchy,
        );
    }

    /// Step 2 of call hierarchy: request the incoming calls (callers) for the
    /// prepared `item`. The response arrives as `LspEvent::References`.
    pub fn request_incoming_calls(&mut self, path: &Path, item: serde_json::Value) {
        let Some(config) = self.config_for(path) else {
            return;
        };
        let Some(server) = self.servers.get_mut(&config.language_id) else {
            return;
        };
        let id = server.alloc_id();
        server.pending.insert(id, Pending::IncomingCalls);
        let mut params = serde_json::Map::new();
        params.insert("item".to_string(), item); // consumes `item`
        server.send(message::request(
            id,
            "callHierarchy/incomingCalls",
            &serde_json::Value::Object(params),
        ));
    }

    /// Request all references to the symbol at `(line, character)`.
    pub fn request_references(&mut self, path: &Path, line: u32, character: u32) {
        let Some(config) = self.config_for(path) else {
            return;
        };
        let uri = path_to_uri(path);
        let Some(server) = self.servers.get_mut(&config.language_id) else {
            return;
        };
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

    /// Request the document symbols (outline) for `path`.
    pub fn request_document_symbols(&mut self, path: &Path) {
        self.send_request(
            path,
            "textDocument/documentSymbol",
            Pending::DocumentSymbols,
            message::text_document_params,
        );
    }

    /// Request workspace symbols matching `query` (sent to `path`'s server).
    pub fn request_workspace_symbols(&mut self, path: &Path, query: &str) {
        self.send_request(
            path,
            "workspace/symbol",
            Pending::WorkspaceSymbols,
            |_uri| message::workspace_symbol_params(query),
        );
    }

    /// Request signature help at `(line, character)`.
    pub fn request_signature_help(&mut self, path: &Path, line: u32, character: u32) {
        self.request(
            path,
            "textDocument/signatureHelp",
            line,
            character,
            Pending::SignatureHelp,
        );
    }

    /// Request the selection ranges (expand/shrink chain) at `(line, character)`.
    pub fn request_selection_range(&mut self, path: &Path, line: u32, character: u32) {
        self.send_request(
            path,
            "textDocument/selectionRange",
            Pending::SelectionRange,
            |uri| message::selection_range_params(uri, line, character),
        );
    }

    /// Request the occurrences of the symbol at `(line, character)` to highlight.
    pub fn request_document_highlight(&mut self, path: &Path, line: u32, character: u32) {
        self.request(
            path,
            "textDocument/documentHighlight",
            line,
            character,
            Pending::DocumentHighlight,
        );
    }

    /// Request the code lenses for `path`.
    pub fn request_code_lens(&mut self, path: &Path) {
        self.send_request(path, "textDocument/codeLens", Pending::CodeLens, |uri| {
            message::text_document_params(uri)
        });
    }

    /// Execute a server command (`workspace/executeCommand`); the response is
    /// ignored (edits arrive via a server `workspace/applyEdit` request).
    pub fn execute_command(&mut self, path: &Path, command: &str, arguments: &Value) {
        let Some(config) = self.config_for(path) else {
            return;
        };
        let Some(server) = self.servers.get_mut(&config.language_id) else {
            return;
        };
        let id = server.alloc_id();
        server.send(message::request(
            id,
            "workspace/executeCommand",
            &message::execute_command_params(command, arguments),
        ));
    }

    /// Request the linked-editing ranges at `(line, character)`.
    pub fn request_linked_editing(&mut self, path: &Path, line: u32, character: u32) {
        self.request(
            path,
            "textDocument/linkedEditingRange",
            line,
            character,
            Pending::LinkedEditing,
        );
    }

    /// Request inlay hints covering `[start, end)` of `path`.
    pub fn request_inlay_hint(&mut self, path: &Path, start: (u32, u32), end: (u32, u32)) {
        self.send_request(path, "textDocument/inlayHint", Pending::InlayHint, |uri| {
            message::inlay_hint_params(uri, start, end)
        });
    }

    /// Request the foldable line ranges for `path`.
    pub fn request_folding_range(&mut self, path: &Path) {
        self.send_request(
            path,
            "textDocument/foldingRange",
            Pending::FoldingRange,
            message::text_document_params,
        );
    }

    /// Resolve fuller detail/documentation for a completion item (sent to
    /// `path`'s server). `data` is the opaque payload the server round-trips.
    pub fn request_completion_resolve(&mut self, path: &Path, label: &str, data: Option<&Value>) {
        let Some(config) = self.config_for(path) else {
            return;
        };
        let Some(server) = self.servers.get_mut(&config.language_id) else {
            return;
        };
        let id = server.alloc_id();
        server.pending.insert(id, Pending::CompletionResolve);
        server.send(message::request(
            id,
            "completionItem/resolve",
            &message::completion_resolve_params(label, data),
        ));
    }

    /// Request a rename of the symbol at `(line, character)` to `new_name`.
    pub fn request_rename(&mut self, path: &Path, line: u32, character: u32, new_name: &str) {
        self.send_request(path, "textDocument/rename", Pending::Rename, |uri| {
            message::rename_params(uri, line, character, new_name)
        });
    }

    /// Request code actions for the range `[start, end)`, with `diagnostics`
    /// (raw LSP objects overlapping the range) in the request context.
    pub fn request_code_action(
        &mut self,
        path: &Path,
        start: (u32, u32),
        end: (u32, u32),
        diagnostics: &Value,
    ) {
        self.send_request(
            path,
            "textDocument/codeAction",
            Pending::CodeAction,
            |uri| message::code_action_params(uri, start, end, diagnostics),
        );
    }

    /// Request formatting of the whole document `path` (`tab_size`-wide indent).
    pub fn request_formatting(&mut self, path: &Path, tab_size: u32) {
        self.send_request(
            path,
            "textDocument/formatting",
            Pending::Formatting,
            |uri| message::formatting_params(uri, tab_size),
        );
    }

    /// Request formatting of the range `[start, end)` (0-based positions).
    pub fn request_range_formatting(
        &mut self,
        path: &Path,
        start: (u32, u32),
        end: (u32, u32),
        tab_size: u32,
    ) {
        self.send_request(
            path,
            "textDocument/rangeFormatting",
            Pending::Formatting,
            |uri| message::range_formatting_params(uri, start, end, tab_size),
        );
    }

    /// Send a request for an open document, building params from its URI.
    fn send_request(
        &mut self,
        path: &Path,
        method: &str,
        kind: Pending,
        params: impl FnOnce(&str) -> Value,
    ) {
        let Some(config) = self.config_for(path) else {
            return;
        };
        let uri = path_to_uri(path);
        let Some(server) = self.servers.get_mut(&config.language_id) else {
            return;
        };
        if !server.docs.contains_key(&uri) {
            return;
        }
        let id = server.alloc_id();
        server.pending.insert(id, kind);
        server.send(message::request(id, method, &params(&uri)));
    }

    /// Notify the server that `path` was saved (full text), to trigger
    /// re-analysis.
    pub fn did_save(&mut self, path: &Path, text: &str) {
        if !self.enabled {
            return;
        }
        let Some(config) = self.config_for(path) else {
            return;
        };
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
                let Some(server) = self.servers.get(&lang) else {
                    continue;
                };
                let mut v = Vec::new();
                while let Ok(m) = server.rx.try_recv() {
                    v.push(m);
                }
                v
            };
            for incoming in drained {
                match incoming {
                    Incoming::Exited => {
                        // Reap the exited child so it doesn't linger as a zombie
                        // (a crashing/restarting server would otherwise leak one
                        // per incident). The reader saw EOF, so `wait` is prompt.
                        if let Some(mut server) = self.servers.remove(&lang) {
                            let _ = server.child.wait();
                        }
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

        // Server → client request (has both id and method).
        if let (Some(method), true) = (has_method, has_id) {
            if method == "workspace/applyEdit" {
                // Apply the edit on the host and acknowledge optimistically.
                if let Some(params) = msg.get("params") {
                    let edits: Vec<FileEdits> = message::parse_apply_edit(params)
                        .into_iter()
                        .map(|(uri, e)| (uri_to_path(&uri), e))
                        .collect();
                    if !edits.is_empty() {
                        events.push(LspEvent::WorkspaceEdit(edits));
                    }
                }
                if let Some(server) = self.servers.get_mut(lang) {
                    let id = msg.get("id").cloned().unwrap_or(Value::Null);
                    server.write_now(
                        &json!({ "jsonrpc": "2.0", "id": id, "result": { "applied": true } }),
                    );
                }
                return;
            }
            self.reply_to_server_request(lang, msg, method);
            return;
        }
        // Notification (method, no id).
        if let Some(method) = has_method {
            if method == "textDocument/publishDiagnostics"
                && let Some(params) = msg.get("params")
                && let Some((uri, diags)) = message::parse_diagnostics(params)
            {
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
        let Some(server) = self.servers.get_mut(lang) else {
            return;
        };
        let Some(kind) = server.pending.remove(&id) else {
            return;
        };
        let Some(result) = msg.get("result") else {
            return;
        };
        if result.is_null() {
            return;
        }
        Self::response_to_events(kind, result, events);
    }

    /// Parse a (non-null) response `result` for the request `kind` into events.
    /// Split out of [`Lsp::handle_response`] to keep that within the line limit.
    fn response_to_events(kind: Pending, result: &Value, events: &mut Vec<LspEvent>) {
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
                    .map(|l| {
                        (
                            uri_to_path(&l.uri),
                            l.range.start.line,
                            l.range.start.character,
                        )
                    })
                    .collect();
                if !locs.is_empty() {
                    events.push(LspEvent::References(locs));
                }
            }
            Pending::Formatting => {
                let edits = message::parse_text_edits(result);
                if !edits.is_empty() {
                    events.push(LspEvent::Edits(edits));
                }
            }
            _ => Self::response_to_events_more(kind, result, events),
        }
    }

    /// The remaining response arms, split out of [`Lsp::response_to_events`] to
    /// keep each within the line limit.
    fn response_to_events_more(kind: Pending, result: &Value, events: &mut Vec<LspEvent>) {
        match kind {
            Pending::PrepareCallHierarchy => {
                if let Some(item) = message::first_call_hierarchy_item(result) {
                    events.push(LspEvent::CallHierarchyPrepared(item));
                }
            }
            Pending::IncomingCalls => {
                let locs: Vec<(PathBuf, u32, u32)> = message::parse_incoming_calls(result)
                    .into_iter()
                    .map(|l| {
                        (
                            uri_to_path(&l.uri),
                            l.range.start.line,
                            l.range.start.character,
                        )
                    })
                    .collect();
                if !locs.is_empty() {
                    events.push(LspEvent::References(locs));
                }
            }
            Pending::DocumentSymbols => {
                let syms = message::parse_document_symbols(result);
                if !syms.is_empty() {
                    events.push(LspEvent::DocumentSymbols(syms));
                }
            }
            Pending::WorkspaceSymbols => {
                let syms: Vec<(PathBuf, u32, u32, String)> =
                    message::parse_workspace_symbols(result)
                        .into_iter()
                        .map(|(uri, line, ch, name)| (uri_to_path(&uri), line, ch, name))
                        .collect();
                if !syms.is_empty() {
                    events.push(LspEvent::WorkspaceSymbols(syms));
                }
            }
            Pending::SignatureHelp => {
                if let Some(text) = message::parse_signature_help(result) {
                    events.push(LspEvent::SignatureHelp(text));
                }
            }
            Pending::Rename => {
                let edits: Vec<FileEdits> = message::parse_workspace_edit(result)
                    .into_iter()
                    .map(|(uri, e)| (uri_to_path(&uri), e))
                    .collect();
                if !edits.is_empty() {
                    events.push(LspEvent::WorkspaceEdit(edits));
                }
            }
            Pending::CodeAction => {
                let actions: Vec<CodeAction> = message::parse_code_actions(result)
                    .into_iter()
                    .map(|(title, edit)| {
                        let edit: Vec<FileEdits> = edit
                            .into_iter()
                            .map(|(uri, e)| (uri_to_path(&uri), e))
                            .collect();
                        (title, edit)
                    })
                    .collect();
                if !actions.is_empty() {
                    events.push(LspEvent::CodeActions(actions));
                }
            }
            _ => Self::response_to_events_last(kind, result, events),
        }
    }

    /// The remaining `Pending` response kinds (selection ranges, highlights,
    /// folding, completion-resolve, inlay hints, linked editing, code lenses).
    /// Split from [`Server::response_to_events_more`] to keep it within the line
    /// limit.
    fn response_to_events_last(kind: Pending, result: &Value, events: &mut Vec<LspEvent>) {
        match kind {
            Pending::SelectionRange => {
                let ranges = message::parse_selection_ranges(result);
                if !ranges.is_empty() {
                    events.push(LspEvent::SelectionRanges(ranges));
                }
            }
            Pending::DocumentHighlight => {
                let ranges = message::parse_document_highlights(result);
                if !ranges.is_empty() {
                    events.push(LspEvent::Highlights(ranges));
                }
            }
            Pending::FoldingRange => {
                events.push(LspEvent::FoldingRanges(message::parse_folding_ranges(
                    result,
                )));
            }
            Pending::CompletionResolve => {
                if let Some(text) = message::parse_resolved_detail(result) {
                    events.push(LspEvent::CompletionDetail(text));
                }
            }
            Pending::InlayHint => {
                let hints = message::parse_inlay_hints(result);
                if !hints.is_empty() {
                    events.push(LspEvent::InlayHints(hints));
                }
            }
            Pending::LinkedEditing => {
                let ranges = message::parse_linked_editing_ranges(result);
                if ranges.len() > 1 {
                    events.push(LspEvent::LinkedRanges(ranges));
                }
            }
            Pending::CodeLens => {
                let lenses = message::parse_code_lenses(result);
                if !lenses.is_empty() {
                    events.push(LspEvent::CodeLenses(lenses));
                }
            }
            _ => {} // handled in response_to_events
        }
    }

    /// On the `initialize` response: record the encoding, send `initialized`, and
    /// flush everything queued during startup.
    fn finish_initialize(&mut self, lang: &str, msg: &Value) {
        let Some(server) = self.servers.get_mut(lang) else {
            return;
        };
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
        let Some(server) = self.servers.get_mut(lang) else {
            return;
        };
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
            let _ = server.child.wait(); // reap so no zombie is left behind
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
    let (wtx, wrx) = channel::<Vec<u8>>();
    spawn_writer(stdin, wrx);
    let mut server = Server {
        child,
        writer: wtx,
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

/// Drain framed messages off `wrx` and write them to the server's stdin. Runs
/// on its own thread so a stalled server (a full pipe buffer) can never block
/// the UI thread. Exits when the channel closes (the [`Server`] was dropped) or
/// a write fails (the server died).
fn spawn_writer(mut stdin: ChildStdin, wrx: Receiver<Vec<u8>>) {
    std::thread::spawn(move || {
        while let Ok(buf) = wrx.recv() {
            if stdin.write_all(&buf).is_err() || stdin.flush().is_err() {
                return;
            }
        }
    });
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
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&rest[i + 1..i + 3], 16)
        {
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

    #[test]
    #[cfg(unix)]
    fn writer_thread_decouples_the_caller_from_a_stalled_stdin() {
        use std::time::{Duration, Instant};
        // A child that never reads its stdin, so the OS pipe fills after ~64 KiB.
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("sleep 5")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn sh");
        let stdin = child.stdin.take().unwrap();
        let (wtx, wrx) = channel::<Vec<u8>>();
        spawn_writer(stdin, wrx);

        // Queue several MiB. The writer thread blocks on the full pipe, but the
        // caller's sends (what `write_now` does) must never block — the whole
        // point of moving writes off the UI thread.
        let t0 = Instant::now();
        for _ in 0..64 {
            wtx.send(vec![b'x'; 65_536]).unwrap();
        }
        assert!(
            t0.elapsed() < Duration::from_secs(2),
            "sending blocked the caller — writes are not decoupled from stdin"
        );
        let _ = child.kill();
        let _ = child.wait();
    }
}
