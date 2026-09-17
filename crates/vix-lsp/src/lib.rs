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
use std::time::Instant;

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
    PrepareRename,
    WorkspaceDiagnostics,
    SemanticTokens,
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
    /// The server replied to a request with a JSON-RPC `error` object instead
    /// of a `result` (T123 audit: previously silently dropped, so a failed
    /// rename/code-action/format/… just appeared to do nothing) — the
    /// error's own `message` text, for the host to surface.
    RequestFailed(String),
    /// A `textDocument/prepareRename` result (T134 audit): whether the
    /// cursor's position can be renamed at all, and if so, what to seed the
    /// rename prompt with.
    RenamePrepared(RenamePrepared),
    /// A crashed server for this language was respawned (T134 audit: it
    /// used to just vanish, silently dead for every file that was already
    /// open on it). These files were open on the crashed server and need a
    /// fresh `didOpen` replayed with their *current* content — `Lsp` itself
    /// has no buffer content, only the host does.
    ServerRestarted(Vec<PathBuf>),
    /// A server for this language crashed and either exceeded the
    /// automatic respawn attempt limit, or failed to respawn at all (e.g.
    /// the command no longer exists) — every LSP feature for this language
    /// is dead until the next explicit `did_open` (e.g. closing and
    /// reopening a file).
    ServerCrashed(String),
    /// A `$/progress` update worth showing (T123d) — e.g. a server's
    /// initial index build. Only `"begin"`/`"report"` kinds reach the host;
    /// an `"end"` report has nothing left to say, so it produces no event
    /// (the host's status line is ambient/best-effort already, same as
    /// every other transient status message).
    Progress(String),
    /// A `textDocument/semanticTokens/full` response for the active file
    /// (T123a): every token, already decoded to absolute positions and
    /// resolved against the server's legend. Like `Hover`/`InlayHints`/…,
    /// this carries no path — the host applies it to whichever file was
    /// active when the response arrives, same established limitation those
    /// already have under rapid tab switching.
    SemanticTokens(Vec<vix_lsp_core::SemanticToken>),
}

/// Outcome of `textDocument/prepareRename`, sent before the rename prompt
/// opens so it can be seeded (or skipped) with server-validated information
/// instead of only the host's own word-under-cursor guess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenamePrepared {
    /// Renameable; the server supplied its own placeholder text.
    Placeholder(String),
    /// Renameable; the server left the placeholder to the client's own
    /// default (or a server that doesn't implement `prepareRename` at all —
    /// its absence is not treated as "not renameable", since most servers
    /// support plain `rename` without ever implementing this refinement).
    Default,
    /// The server said this exact position cannot be renamed.
    NotRenameable,
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
    /// This server's semantic-token type legend, negotiated at `initialize`
    /// (T123a) — empty when it doesn't advertise semantic tokens support at
    /// all. `parse_semantic_tokens` resolves a response's numeric type
    /// indices against this.
    semantic_tokens_legend: Vec<String>,
    /// Whether `initialize` has completed and `initialized` been sent.
    ready: bool,
    /// Messages deferred until the server is `ready`.
    queue: Vec<Value>,
    /// When this server reached `ready`, used to tell a genuine crash-loop
    /// (many crashes in quick succession) from an isolated crash after a
    /// long healthy run — see [`MAX_RESTART_ATTEMPTS`].
    ready_since: Option<Instant>,
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
    /// Latest diagnostics keyed by canonical file path, then by the
    /// `language_id` of the server that published them (T123b): a second
    /// server handling the same file (e.g. a type-checker plus a separate
    /// linter, both configured for `.rs`) publishes independently of the
    /// first, so each server's own most recent report must be tracked
    /// separately rather than one clobbering the other's entry outright.
    diagnostics: HashMap<PathBuf, HashMap<String, Vec<Diagnostic>>>,
    /// Consecutive crash-respawn attempts per language, since it last
    /// survived [`STABLE_UPTIME`] without crashing again (T134 audit —
    /// crash recovery). Capped at [`MAX_RESTART_ATTEMPTS`] so a server that
    /// crashes in a tight loop (a bad command, a real bug) doesn't respawn
    /// forever — deliberately *not* reset on every `ready`, since a server
    /// that crashes shortly after each respawn (this cap's whole reason to
    /// exist) would otherwise reach `ready` every time and keep resetting
    /// its own budget, never actually hitting the cap.
    restart_attempts: HashMap<String, u32>,
}

/// How many times [`Lsp::poll`] auto-respawns a crashed server, consecutively
/// (without [`STABLE_UPTIME`] of healthy running in between), before giving
/// up on it until the next explicit `did_open`.
const MAX_RESTART_ATTEMPTS: u32 = 3;

/// How long a respawned server must stay `ready` without crashing again
/// before a further crash is treated as a fresh, isolated incident (its own
/// full [`MAX_RESTART_ATTEMPTS`] budget) rather than a continuation of the
/// same crash loop.
const STABLE_UPTIME: std::time::Duration = std::time::Duration::from_secs(30);

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
            restart_attempts: HashMap::new(),
        }
    }

    /// Whether LSP is on and at least one server is configured.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.enabled && !self.configs.is_empty()
    }

    /// Every config handling `path` (matched by extension) — more than one
    /// server can be configured for the same extension (T123b: e.g. a
    /// type-checker LSP and a separate linter LSP, both watching `.rs`),
    /// and every one of them should see the document and be askable.
    fn configs_for(&self, path: &Path) -> Vec<ServerConfig> {
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            return Vec::new();
        };
        let ext = ext.to_ascii_lowercase();
        self.configs
            .iter()
            .filter(|c| c.extensions.iter().any(|e| e.eq_ignore_ascii_case(&ext)))
            .cloned()
            .collect()
    }

    /// The first matching config handling `path`, if any — for the few
    /// requests that must target one specific server rather than fan out
    /// (see [`Lsp::configs_for`]'s doc and `spec/index.md`'s "Known gaps").
    fn config_for(&self, path: &Path) -> Option<ServerConfig> {
        self.configs_for(path).into_iter().next()
    }

    /// The position encoding for `path`'s server (UTF-16 if none / not
    /// ready). When more than one server handles `path`, this is the first
    /// matching one's encoding — a documented limitation (`spec/index.md`
    /// "Known gaps"): two servers disagreeing on encoding for the same file
    /// is not handled per-server throughout the position-translation call
    /// sites.
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

    /// Diagnostics for `path` from every server handling it, merged
    /// (T123b — each server's own report is tracked separately internally,
    /// so a second server can no longer silently clobber the first's).
    #[must_use]
    pub fn diagnostics_for(&self, path: &Path) -> Vec<Diagnostic> {
        let key = canonical(path);
        self.diagnostics
            .get(&key)
            .map(|by_server| by_server.values().flatten().cloned().collect())
            .unwrap_or_default()
    }

    /// Every file's diagnostics (path, merged list across every server
    /// handling it), for the diagnostics panel. Files with no current
    /// diagnostics are skipped.
    pub fn all_diagnostics(&self) -> impl Iterator<Item = (&PathBuf, Vec<Diagnostic>)> {
        self.diagnostics.iter().filter_map(|(path, by_server)| {
            let merged: Vec<Diagnostic> = by_server.values().flatten().cloned().collect();
            (!merged.is_empty()).then_some((path, merged))
        })
    }

    /// Total diagnostic count across all files and servers (for the status bar).
    #[must_use]
    pub fn diagnostic_count(&self) -> usize {
        self.diagnostics
            .values()
            .flat_map(HashMap::values)
            .map(Vec::len)
            .sum()
    }

    /// Record `diags` as `lang`'s current diagnostics for `path` (T123b):
    /// an empty report clears just that server's entry, not the whole
    /// path, so a second server's diagnostics for the same file survive.
    fn set_diagnostics(&mut self, lang: &str, path: &Path, diags: Vec<Diagnostic>) {
        let by_server = self.diagnostics.entry(path.to_path_buf()).or_default();
        if diags.is_empty() {
            by_server.remove(lang);
        } else {
            by_server.insert(lang.to_string(), diags);
        }
        if by_server.is_empty() {
            self.diagnostics.remove(path);
        }
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

    /// Notify every server handling `path` that it opened, with its current
    /// `text` (T123b: fans out to all of them, not just the first).
    pub fn did_open(&mut self, path: &Path, text: &str) {
        if !self.enabled {
            return;
        }
        let uri = path_to_uri(path);
        for config in self.configs_for(path) {
            let lang = config.language_id.clone();
            let Some(server) = self.ensure_server(&config) else {
                continue;
            };
            if server.docs.contains_key(&uri) {
                continue; // already open
            }
            server.docs.insert(uri.clone(), 1);
            server.send(message::notification(
                "textDocument/didOpen",
                &message::did_open_params(&uri, &lang, 1, text),
            ));
        }
    }

    /// Notify every server handling `path` that its buffer changed
    /// (full-document sync; T123b: fans out to all of them).
    pub fn did_change(&mut self, path: &Path, text: &str) {
        if !self.enabled {
            return;
        }
        let uri = path_to_uri(path);
        for config in self.configs_for(path) {
            let Some(server) = self.servers.get_mut(&config.language_id) else {
                continue;
            };
            let Some(version) = server.docs.get_mut(&uri) else {
                continue;
            };
            *version += 1;
            let v = *version;
            server.send(message::notification(
                "textDocument/didChange",
                &message::did_change_full_params(&uri, v, text),
            ));
        }
    }

    /// Notify every server handling `path` that it closed (T123b: fans out).
    pub fn did_close(&mut self, path: &Path) {
        if !self.enabled {
            return;
        }
        let uri = path_to_uri(path);
        for config in self.configs_for(path) {
            if let Some(server) = self.servers.get_mut(&config.language_id)
                && server.docs.remove(&uri).is_some()
            {
                server.send(message::notification(
                    "textDocument/didClose",
                    &message::did_close_params(&uri),
                ));
            }
        }
    }

    /// Send a feature request for `path` at `(line, character)` to every
    /// server handling it (T123b: fans out — e.g. two servers both asked for
    /// hover text produce two `LspEvent::Hover`s, one per response, as they
    /// arrive). Each response arrives later via [`Lsp::poll`] as the
    /// matching [`LspEvent`].
    fn request(&mut self, path: &Path, method: &str, line: u32, character: u32, kind: Pending) {
        let uri = path_to_uri(path);
        for config in self.configs_for(path) {
            let Some(server) = self.servers.get_mut(&config.language_id) else {
                continue;
            };
            if !server.docs.contains_key(&uri) {
                continue; // only query open documents
            }
            let id = server.alloc_id();
            server.pending.insert(id, kind);
            server.send(message::request(
                id,
                method,
                &message::position_params(&uri, line, character),
            ));
        }
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
    ///
    /// Deliberately **not** fanned out across every server handling `path`
    /// (T123b's scope cut, `spec/index.md` "Known gaps"): `item` is an
    /// opaque payload one specific server produced in step 1
    /// (`request_prepare_call_hierarchy`, which *does* fan out), and
    /// nothing here tracks which one — sending it to every matching server
    /// would mean sending it to servers that never issued it. Targets the
    /// first matching config, same as before T123b; correct when only one
    /// server handles `path` (still the common case), and no worse than
    /// today when more than one does and call hierarchy specifically is
    /// the feature in use.
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

    /// Request all references to the symbol at `(line, character)`, from
    /// every server handling `path` (T123b: fans out).
    pub fn request_references(&mut self, path: &Path, line: u32, character: u32) {
        let uri = path_to_uri(path);
        for config in self.configs_for(path) {
            let Some(server) = self.servers.get_mut(&config.language_id) else {
                continue;
            };
            if !server.docs.contains_key(&uri) {
                continue;
            }
            let id = server.alloc_id();
            server.pending.insert(id, Pending::References);
            server.send(message::request(
                id,
                "textDocument/references",
                &message::reference_params(&uri, line, character, true),
            ));
        }
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
    ///
    /// Deliberately **not** fanned out across every server handling `path`
    /// (T123b's scope cut, `spec/index.md` "Known gaps"): `command` names
    /// one specific server's own command (usually from a code action or
    /// code lens that server attached), and nothing here tracks which
    /// server that was — sending it to every matching server would mean
    /// asking servers that never advertised that command to run it.
    /// Targets the first matching config, same as before T123b.
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
    ///
    /// Deliberately **not** fanned out across every server handling `path`
    /// (T123b's scope cut, `spec/index.md` "Known gaps"): the item being
    /// resolved came from one specific server's completion response (from
    /// `request_completion`, which *does* fan out), and nothing here tracks
    /// which one — sending `data` to every matching server would mean
    /// asking servers that never proposed this item to resolve it. Targets
    /// the first matching config, same as before T123b; correct when only
    /// one server handles `path` (still the common case).
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

    /// Ask whether `(line, character)` can be renamed at all, and for a
    /// server-supplied placeholder to seed the rename prompt with (T134
    /// audit) — send before [`Lsp::request_rename`], not instead of it; the
    /// [`LspEvent::RenamePrepared`] response tells the host whether to open
    /// the prompt, and with what.
    pub fn request_prepare_rename(&mut self, path: &Path, line: u32, character: u32) {
        self.send_request(
            path,
            "textDocument/prepareRename",
            Pending::PrepareRename,
            |uri| message::position_params(uri, line, character),
        );
    }

    /// Pull a full workspace diagnostic report from every running server
    /// (T123d): unlike push (`publishDiagnostics`), this reflects a
    /// server's whole-project analysis, not just files that have actually
    /// been opened/synced. Results merge into the same diagnostics map push
    /// already fills (`handle_response`'s `WorkspaceDiagnostics` case), so
    /// the Problems panel picks them up with no other host wiring.
    pub fn request_workspace_diagnostics(&mut self) {
        let langs: Vec<String> = self.servers.keys().cloned().collect();
        for lang in langs {
            let Some(server) = self.servers.get_mut(&lang) else {
                continue;
            };
            let id = server.alloc_id();
            server.pending.insert(id, Pending::WorkspaceDiagnostics);
            server.send(message::request(
                id,
                "workspace/diagnostic",
                &message::workspace_diagnostic_params(),
            ));
        }
    }

    /// Request `textDocument/semanticTokens/full` for `path` (T123a) — a
    /// second, LSP-driven highlight layer over Tree-sitter's purely
    /// syntactic one, for distinctions Tree-sitter structurally cannot make
    /// (mutable vs. immutable binding, trait-default vs. inherent method,
    /// unused variable/parameter). Only sent to servers that actually
    /// advertised support (a non-empty legend at `initialize`) — a server
    /// with none never gets asked, since `handle_response`'s decode would
    /// have nothing to resolve type indices against anyway. When more than
    /// one matching server supports it (T123b), each response fully
    /// replaces the editor's semantic-token layer as it arrives — the last
    /// one to respond wins, same documented last-write-wins simplification
    /// as every other single-buffer LSP feature (`spec/index.md`).
    pub fn request_semantic_tokens(&mut self, path: &Path) {
        let uri = path_to_uri(path);
        for config in self.configs_for(path) {
            let Some(server) = self.servers.get_mut(&config.language_id) else {
                continue;
            };
            if server.semantic_tokens_legend.is_empty() || !server.docs.contains_key(&uri) {
                continue;
            }
            let id = server.alloc_id();
            server.pending.insert(id, Pending::SemanticTokens);
            server.send(message::request(
                id,
                "textDocument/semanticTokens/full",
                &message::text_document_params(&uri),
            ));
        }
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

    /// Send a request for an open document to every server handling `path`
    /// (T123b: fans out, same as [`Lsp::request`]), building params from its
    /// URI. `params` takes `&self` by `Fn` rather than `FnOnce` since it may
    /// now be called once per matching server.
    fn send_request(
        &mut self,
        path: &Path,
        method: &str,
        kind: Pending,
        params: impl Fn(&str) -> Value,
    ) {
        let uri = path_to_uri(path);
        for config in self.configs_for(path) {
            let Some(server) = self.servers.get_mut(&config.language_id) else {
                continue;
            };
            if !server.docs.contains_key(&uri) {
                continue;
            }
            let id = server.alloc_id();
            server.pending.insert(id, kind);
            server.send(message::request(id, method, &params(&uri)));
        }
    }

    /// Notify every server handling `path` that it was saved (full text), to
    /// trigger re-analysis (T123b: fans out to all of them).
    pub fn did_save(&mut self, path: &Path, text: &str) {
        if !self.enabled {
            return;
        }
        let uri = path_to_uri(path);
        for config in self.configs_for(path) {
            if let Some(server) = self.servers.get_mut(&config.language_id)
                && server.docs.contains_key(&uri)
            {
                server.send(message::notification(
                    "textDocument/didSave",
                    &message::did_save_params(&uri, text),
                ));
            }
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
                            let survived_a_while = server
                                .ready_since
                                .is_some_and(|t| t.elapsed() >= STABLE_UPTIME);
                            self.respawn_after_crash(
                                &lang,
                                &server.docs,
                                survived_a_while,
                                &mut events,
                            );
                        }
                        break;
                    }
                    Incoming::Message(msg) => self.handle(&lang, &msg, &mut events),
                }
            }
        }
        events
    }

    /// Handle one server's crash (T134 audit — this used to just remove the
    /// server and stop, leaving every file that was open on it silently dead
    /// until the user closed and reopened it): respawn the same command, up
    /// to [`MAX_RESTART_ATTEMPTS`] consecutive attempts since it last stayed
    /// up for [`STABLE_UPTIME`] (`survived_a_while`, `true` resets the
    /// budget — a fresh, isolated incident earns its own full budget rather
    /// than inheriting a count from a crash loop long past), and tell the
    /// host which files (`open_docs`, the crashed server's own doc-sync
    /// table) need a fresh `didOpen` replayed with their real, current
    /// content — `Lsp` never holds buffer content itself, only the host
    /// does.
    fn respawn_after_crash(
        &mut self,
        lang: &str,
        open_docs: &HashMap<String, i64>,
        survived_a_while: bool,
        events: &mut Vec<LspEvent>,
    ) {
        if survived_a_while {
            self.restart_attempts.remove(lang);
        }
        let attempts = self.restart_attempts.entry(lang.to_string()).or_insert(0);
        *attempts += 1;
        if *attempts > MAX_RESTART_ATTEMPTS {
            events.push(LspEvent::ServerCrashed(lang.to_string()));
            return;
        }
        let Some(config) = self.configs.iter().find(|c| c.language_id == lang).cloned() else {
            return;
        };
        let Some(new_server) = spawn(&config, self.root_uri.as_deref()) else {
            events.push(LspEvent::ServerCrashed(lang.to_string()));
            return;
        };
        self.servers.insert(lang.to_string(), new_server);
        if !open_docs.is_empty() {
            let paths = open_docs.keys().map(|uri| uri_to_path(uri)).collect();
            events.push(LspEvent::ServerRestarted(paths));
        }
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
                self.set_diagnostics(lang, &path, diags);
                events.push(LspEvent::Diagnostics(path));
            }
            if method == "$/progress"
                && let Some(params) = msg.get("params")
                && let Some(text) = message::parse_progress(params)
            {
                events.push(LspEvent::Progress(text));
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
        // Grabbed now, while `server` is still borrowed -- `Pending::
        // SemanticTokens` below needs it, but only after `self.diagnostics`
        // (a different field) is free to borrow too, past `server`'s own
        // last use.
        let semantic_tokens_legend = server.semantic_tokens_legend.clone();
        let Some(result) = msg.get("result") else {
            // T123 audit: a JSON-RPC `error` object here used to be
            // silently dropped -- a failed rename/code-action/format/…
            // just appeared to do nothing. Surface the server's own
            // message instead. `PrepareRename` is a deliberate exception
            // (T134 audit): most servers implement plain `rename` without
            // ever implementing this refinement, and an unrelated "method
            // not found" error there shouldn't read to the user as "you
            // can't rename this" -- fall back to the client's own default
            // seed instead of surfacing a scary error for an optional step.
            if matches!(kind, Pending::PrepareRename) {
                events.push(LspEvent::RenamePrepared(RenamePrepared::Default));
                return;
            }
            // T123d: pull diagnostics (`workspace/diagnostic`) is a newer
            // LSP 3.17 addition most servers don't implement yet; a "method
            // not found" here is expected and silent, not a user-visible
            // failure -- push (`publishDiagnostics`) already covers the
            // baseline experience regardless.
            // T123a: semantic tokens is likewise a newer, optional LSP 3.17
            // capability -- Tree-sitter's own highlighting already covers
            // the baseline, so a server without support (or a transient
            // failure) shouldn't interrupt the user with an error.
            if matches!(
                kind,
                Pending::WorkspaceDiagnostics | Pending::SemanticTokens
            ) {
                return;
            }
            if let Some(error) = msg.get("error") {
                let text = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("the server returned an error")
                    .to_string();
                events.push(LspEvent::RequestFailed(text));
            }
            return;
        };
        // `null` here is meaningful for `PrepareRename` (T134 audit): the
        // server is explicitly saying this position cannot be renamed, not
        // "no reply" -- handle it before the blanket null-is-nothing below.
        if matches!(kind, Pending::PrepareRename) {
            let outcome = match message::parse_prepare_rename(result) {
                Some(Some(text)) => RenamePrepared::Placeholder(text),
                Some(None) => RenamePrepared::Default,
                None => RenamePrepared::NotRenameable,
            };
            events.push(LspEvent::RenamePrepared(outcome));
            return;
        }
        // T123d: merges straight into `self.diagnostics`, the same map push
        // (`publishDiagnostics`) fills — needs `&mut self`, unlike every
        // other response kind, so it can't go through the self-less
        // `response_to_events`.
        if matches!(kind, Pending::WorkspaceDiagnostics) {
            for (uri, diags) in message::parse_workspace_diagnostics(result) {
                let path = canonical(&uri_to_path(&uri));
                self.set_diagnostics(lang, &path, diags);
                events.push(LspEvent::Diagnostics(path));
            }
            return;
        }
        // T123a: needs the legend captured above (per-server state,
        // unavailable to the self-less `response_to_events`).
        if matches!(kind, Pending::SemanticTokens) {
            let tokens = message::parse_semantic_tokens(result, &semantic_tokens_legend);
            if !tokens.is_empty() {
                events.push(LspEvent::SemanticTokens(tokens));
            }
            return;
        }
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
            server.semantic_tokens_legend = message::parse_semantic_tokens_legend(result);
        }
        server.ready = true;
        server.ready_since = Some(Instant::now());
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
        semantic_tokens_legend: Vec::new(),
        ready: false,
        queue: Vec::new(),
        ready_since: None,
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
    fn configs_for_returns_every_server_configured_for_the_extension() {
        // T123b: a type-checker LSP and a separate linter LSP, both
        // configured for the same extension, must both be findable for one
        // file — not just the first one `configs` happens to list.
        let type_checker = ServerConfig {
            language_id: "rust-analyzer".into(),
            extensions: vec!["rs".into()],
            command: vec!["rust-analyzer".into()],
        };
        let linter = ServerConfig {
            language_id: "rust-clippy".into(),
            extensions: vec!["rs".into()],
            command: vec!["clippy-lsp".into()],
        };
        let unrelated = ServerConfig {
            language_id: "python".into(),
            extensions: vec!["py".into()],
            command: vec!["pylsp".into()],
        };
        let lsp = Lsp::new(
            true,
            vec![type_checker, linter, unrelated],
            Path::new("/proj"),
        );
        let configs = lsp.configs_for(Path::new("/proj/src/main.rs"));
        let langs: Vec<&str> = configs.iter().map(|c| c.language_id.as_str()).collect();
        assert_eq!(langs, ["rust-analyzer", "rust-clippy"]);
        assert!(lsp.handles(Path::new("/proj/src/main.rs")));
    }

    #[test]
    fn diagnostics_from_two_servers_for_the_same_file_coexist() {
        // T123b's motivating scenario: a type-checker and a linter both
        // publish diagnostics for the same file. Neither publish should
        // clobber the other's — both must still be present, merged, until
        // one of them explicitly clears its own report (an empty publish).
        let mut lsp = Lsp::new(true, vec![], Path::new("/proj"));
        let path = PathBuf::from("/proj/src/main.rs");
        let err = Diagnostic {
            range: vix_lsp_core::Range {
                start: vix_lsp_core::Position {
                    line: 0,
                    character: 0,
                },
                end: vix_lsp_core::Position {
                    line: 0,
                    character: 1,
                },
            },
            severity: vix_lsp_core::Severity::Error,
            message: "type error".into(),
            source: None,
            related: Vec::new(),
        };
        let warn = Diagnostic {
            range: vix_lsp_core::Range {
                start: vix_lsp_core::Position {
                    line: 1,
                    character: 0,
                },
                end: vix_lsp_core::Position {
                    line: 1,
                    character: 1,
                },
            },
            severity: vix_lsp_core::Severity::Warning,
            message: "unused variable".into(),
            source: None,
            related: Vec::new(),
        };
        lsp.set_diagnostics("rust-analyzer", &canonical(&path), vec![err.clone()]);
        lsp.set_diagnostics("rust-clippy", &canonical(&path), vec![warn.clone()]);
        let merged = lsp.diagnostics_for(&path);
        assert_eq!(merged.len(), 2);
        assert!(merged.iter().any(|d| d.message == "type error"));
        assert!(merged.iter().any(|d| d.message == "unused variable"));
        assert_eq!(lsp.diagnostic_count(), 2);

        // The linter clearing its own report (e.g. the lint no longer
        // fires) must not touch the type-checker's still-current one.
        lsp.set_diagnostics("rust-clippy", &canonical(&path), vec![]);
        let after = lsp.diagnostics_for(&path);
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].message, "type error");
        assert_eq!(lsp.diagnostic_count(), 1);

        // The type-checker clearing its own report too empties the path
        // entirely, not just its own entry.
        lsp.set_diagnostics("rust-analyzer", &canonical(&path), vec![]);
        assert!(lsp.diagnostics_for(&path).is_empty());
        assert_eq!(lsp.diagnostic_count(), 0);
        assert_eq!(lsp.all_diagnostics().count(), 0);
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
