//! LSP and DAP: language-server-driven features (inlay hints, document/
//! selection-range highlights, diagnostics, hover, completion -- including
//! the two non-LSP data sources that reuse the same completion popup, an
//! Org-roam `[[` node-title completer and an Org-contacts `mailto:`/link
//! completer --, go-to-definition, format-on-demand, workspace edits,
//! linked edit, signature help) and DAP debugging (breakpoints, adapter
//! lookup, start/stop, step markers, the debug-prompt accept handler).
//!
//! Moved out of `app.rs` verbatim (T141, slice 6); named `lsp_dap` rather
//! than a bare `lsp`/`dap` to stay unambiguous (no direct collision found
//! -- `app.rs` only ever references `crate::lsp`/`crate::dap` fully
//! qualified or via function-body-local `crate::lsp_core` imports -- but
//! a short, generic name felt like inviting a future one). `build_core`
//! (constructs the LSP client at startup) and `menu_hover` (menu tooltips,
//! an unrelated same-named function) both stayed in `app.rs`.

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

use crate::geo::Geo;
use std::path::{Path, PathBuf};

use vix_input::{KeyCode, KeyEvent};

use super::{
    App, Focus, Prompt, PromptKind, apply_edits_to_text, char_to_lsp_pos, lsp_pos_to_char,
    severity_color,
};
use crate::editor::SEARCH_MARK;
use crate::workspace_search::{Flags as WorkspaceFlags, Hit, WorkspaceSearch};

/// A 0-based LSP line or column as an index; a value past `usize` cannot occur
/// and would name a position past the end of any buffer.
fn lsp_index(n: u32) -> usize {
    usize::try_from(n).unwrap_or(usize::MAX)
}

impl App {
    /// Dispatch a `run.*` debugger action. Returns `true` if `action` was handled.
    pub(super) fn run_dap_action(&mut self, action: &str) -> bool {
        match action {
            "run.start" => self.start_debugger(crate::dap::StartMode::Launch),
            "run.attach" => self.start_debugger(crate::dap::StartMode::Attach),
            "run.stop" => self.stop_debugger(),
            "run.toggle_breakpoint" => self.toggle_breakpoint(),
            "run.edit_breakpoint" => self.begin_edit_breakpoint(),
            "run.exception_filters" => self.open_exception_filters(),
            "run.function_breakpoint" => self.begin_function_breakpoint(),
            "run.data_breakpoint" => self.begin_data_breakpoint(),
            "run.instruction_breakpoint" => self.begin_instruction_breakpoint(),
            "run.continue" => self.resume_debugger(crate::dap::Dap::continue_),
            "run.step_over" => self.resume_debugger(crate::dap::Dap::step_over),
            "run.step_into" => self.resume_debugger(crate::dap::Dap::step_into),
            "run.step_out" => self.resume_debugger(crate::dap::Dap::step_out),
            "run.pause" => self.dap.pause(),
            "run.panel" => self.visible.toggle(crate::app::Visible::DEBUG_PANEL),
            "run.focus_panel" => self.focus_debug_panel(),
            "run.repl" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::DebugRepl,
                    t!("prompt.debug_repl").to_string(),
                ));
            }
            "run.watch" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::DebugWatch,
                    t!("prompt.debug_watch").to_string(),
                ));
            }
            _ => return false,
        }
        true
    }

    /// Request inlay hints for the whole document `path` (when display is on).
    fn request_inlay_hints(&mut self, path: &Path) {
        if !self.visible.contains(crate::app::Visible::INLAY_HINTS) || !self.lsp.handles(path) {
            return;
        }
        let lines = self
            .editor
            .active_tab()
            .map_or(0, |t| t.editor.code_ref().len_lines());
        let end = u32::try_from(lines).unwrap_or(u32::MAX);
        self.lsp.request_inlay_hint(path, (0, 0), (end, 0));
    }

    /// Store inlay hints on the active buffer, converting each LSP `character`
    /// (encoding units) to a char column within its line.
    pub(super) fn apply_inlay_hints(&mut self, hints: &[(u32, u32, String)]) {
        if !self.visible.contains(crate::app::Visible::INLAY_HINTS) {
            return;
        }
        let Some(path) = self.active_path() else {
            return;
        };
        let enc = self.lsp.encoding_for(&path);
        let Some(t) = self.editor.active_tab_mut() else {
            return;
        };
        let converted: Vec<(usize, usize, String)> = {
            let code = t.editor.code_ref();
            hints
                .iter()
                .filter_map(|&(line, character, ref label)| {
                    let line_idx = lsp_index(line);
                    if line_idx >= code.len_lines() {
                        return None;
                    }
                    let abs = lsp_pos_to_char(code, line, character, enc);
                    let col = abs.minus(code.line_to_char(line_idx));
                    Some((line_idx, col, label.clone()))
                })
                .collect()
        };
        t.editor.set_inlay_hints(converted);
    }

    /// Store LSP semantic tokens on the active buffer as a second highlight
    /// layer over Tree-sitter's purely syntactic one (T123a): each token's
    /// `(line, character, length)` converts to an absolute `(start char, end
    /// char)` span, and its server-reported type name maps onto one of the
    /// theme's existing syntax capture names (`semantic_token_capture`) --
    /// v1 deliberately only covers the types the bundled themes actually
    /// define a color for (`comment`/`keyword`/`number`/`string`); richer
    /// distinctions Tree-sitter truly cannot make (mutable vs. immutable
    /// binding, deprecated, unused) need new theme slots first, filed as
    /// T123a's own follow-up rather than done here (see `crates/vix-lsp/
    /// spec/index.md`).
    pub(super) fn apply_semantic_tokens(&mut self, tokens: &[crate::lsp_core::SemanticToken]) {
        let Some(path) = self.active_path() else {
            return;
        };
        let enc = self.lsp.encoding_for(&path);
        let Some(t) = self.editor.active_tab_mut() else {
            return;
        };
        let converted: Vec<(usize, usize, String)> = {
            let code = t.editor.code_ref();
            tokens
                .iter()
                .filter_map(|tok| {
                    let capture = semantic_token_capture(&tok.token_type)?;
                    let line_idx = lsp_index(tok.line);
                    if line_idx >= code.len_lines() {
                        return None;
                    }
                    let start = lsp_pos_to_char(code, tok.line, tok.character, enc);
                    let end = lsp_pos_to_char(code, tok.line, tok.character.plus(tok.length), enc);
                    (start < end).then_some((start, end, capture.to_string()))
                })
                .collect()
        };
        t.editor.set_semantic_tokens(converted);
    }

    /// Toggle inlay-hint display: clear them when turning off, refetch when on.
    pub(super) fn toggle_inlay_hints(&mut self) {
        self.visible.toggle(crate::app::Visible::INLAY_HINTS);
        if self.visible.contains(crate::app::Visible::INLAY_HINTS) {
            if let Some(path) = self.active_path() {
                self.request_inlay_hints(&path);
            }
            self.status = t!("status.inlay_on").to_string();
        } else {
            if let Some(t) = self.editor.active_tab_mut() {
                t.editor.set_inlay_hints(Vec::new());
            }
            self.status = t!("status.inlay_off").to_string();
        }
    }

    /// Highlight the document-highlight occurrences in the active buffer (reuses
    /// the search-mark layer).
    pub(super) fn apply_document_highlights(&mut self, ranges: &[crate::lsp_core::Range]) {
        let Some(path) = self.active_path() else {
            return;
        };
        let enc = self.lsp.encoding_for(&path);
        let Some(t) = self.editor.active_tab_mut() else {
            return;
        };
        let marks: Vec<(usize, usize, &str)> = {
            let code = t.editor.code_ref();
            ranges
                .iter()
                .map(|r| {
                    let s = lsp_pos_to_char(code, r.start.line, r.start.character, enc);
                    let e = lsp_pos_to_char(code, r.end.line, r.end.character, enc);
                    (s.min(e), s.max(e), SEARCH_MARK)
                })
                .collect()
        };
        t.editor.set_marks(marks);
        self.status = t!("status.highlights_n", n = ranges.len()).to_string();
    }

    /// Apply a selection-range chain: expand to the smallest range strictly
    /// larger than the current selection, or shrink to the largest range strictly
    /// inside it.
    pub(super) fn apply_selection_range(&mut self, ranges: &[crate::lsp_core::Range]) {
        let expand = self.expand_selection_dir.take().unwrap_or(true);
        let Some(path) = self.active_path() else {
            return;
        };
        let enc = self.lsp.encoding_for(&path);
        let Some(t) = self.editor.active_tab_mut() else {
            return;
        };
        let (cur_s, cur_e) = {
            let sel = t.editor.get_selection();
            let cursor = t.editor.get_cursor();
            sel.map_or((cursor, cursor), |s| {
                (s.start.min(s.end), s.start.max(s.end))
            })
        };
        // Resolve the chain to char offsets.
        let resolved: Vec<(usize, usize)> = {
            let code = t.editor.code_ref();
            ranges
                .iter()
                .map(|r| {
                    let s = lsp_pos_to_char(code, r.start.line, r.start.character, enc);
                    let e = lsp_pos_to_char(code, r.end.line, r.end.character, enc);
                    (s.min(e), s.max(e))
                })
                .collect()
        };
        let target = if expand {
            // Smallest range that strictly contains the current selection.
            resolved
                .iter()
                .filter(|(s, e)| *s <= cur_s && *e >= cur_e && (*s < cur_s || *e > cur_e))
                .min_by_key(|(s, e)| e.minus(*s))
        } else {
            // Largest range strictly inside the current selection.
            resolved
                .iter()
                .filter(|(s, e)| *s >= cur_s && *e <= cur_e && (*s > cur_s || *e < cur_e))
                .max_by_key(|(s, e)| e.minus(*s))
        };
        if let Some(&(s, e)) = target {
            t.editor
                .set_selection(Some(crate::editor_core::selection::Selection {
                    start: s,
                    end: e,
                }));
            t.editor.set_cursor(e);
        }
    }

    /// The debug adapter configured for `path`'s file extension, if any.
    fn adapter_for(&self, path: &Path) -> Option<crate::dap::DebugAdapter> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())?
            .to_ascii_lowercase();
        self.settings
            .debug_adapters
            .iter()
            .find(|a| a.extensions.iter().any(|e| e == &ext))
            .cloned()
    }

    /// Toggle a breakpoint on the cursor's line in the active file.
    pub(super) fn toggle_breakpoint(&mut self) {
        let Some((path, line)) = self
            .editor
            .active_tab()
            .and_then(|t| t.path.clone().map(|p| (p, t.editor.cursor_line().plus(1))))
        else {
            self.status = t!("status.blame_no_file").to_string();
            return;
        };
        let set = self.breakpoints.entry(path.clone()).or_default();
        if set.remove(&line).is_none() {
            set.insert(line, crate::dap::SourceBreakpoint::new(line));
        }
        self.push_breakpoints(&path);
        self.refresh_debug_markers();
    }

    /// Send `path`'s breakpoints to the running adapter, if any.
    pub(super) fn push_breakpoints(&mut self, path: &Path) {
        if self.dap.is_active() {
            let bps: Vec<crate::dap::SourceBreakpoint> = self
                .breakpoints
                .get(path)
                .map_or_default(|m| m.values().cloned().collect());
            self.dap.set_breakpoints(&path.to_string_lossy(), &bps);
        }
    }

    /// Start debugging the active file with its configured adapter, sending
    /// `launch` or (Run → Attach, T617) `attach`.
    pub(super) fn start_debugger(&mut self, mode: crate::dap::StartMode) {
        let Some(path) = self.editor.active_tab().and_then(|t| t.path.clone()) else {
            self.status = t!("status.debug_no_file").to_string();
            return;
        };
        let Some(adapter) = self.adapter_for(&path) else {
            self.status = t!("status.debug_no_adapter").to_string();
            return;
        };
        let bps: std::collections::HashMap<String, Vec<crate::dap::SourceBreakpoint>> = self
            .breakpoints
            .iter()
            .map(|(p, l)| {
                (
                    p.to_string_lossy().into_owned(),
                    l.values().cloned().collect(),
                )
            })
            .collect();
        // A remote adapter starts in the workspace root, not the remote
        // home (T716); a local one keeps Vix's own working directory.
        let remote = !crate::authority::current().is_local();
        self.dap.set_workdir(remote.then(|| self.root.clone()));
        if self.dap.start(&adapter, &path.to_string_lossy(), bps, mode) {
            self.dap_view.clear();
            self.visible.set(crate::app::Visible::DEBUG_PANEL, true);
            self.status = t!("status.debug_started").to_string();
        } else {
            self.status = t!("status.debug_failed").to_string();
        }
    }

    /// Stop the active debug session and clear its state.
    pub(super) fn stop_debugger(&mut self) {
        self.dap.stop();
        self.dap_stopped = None;
        self.dap_view.clear();
        if self.focus == Focus::Debug {
            self.focus = Focus::Editor;
        }
        self.refresh_debug_markers();
        self.status = t!("status.debug_stopped").to_string();
    }

    /// Push the current breakpoints and stopped line into the active tab's editor
    /// so the gutter shows them.
    pub(super) fn refresh_debug_markers(&mut self) {
        let stopped = self.dap_stopped.clone();
        let Some(path) = self.editor.active_tab().and_then(|t| t.path.clone()) else {
            return;
        };
        let lines: Vec<usize> = self
            .breakpoints
            .get(&path)
            .map_or_default(|s| s.keys().map(|l| l.minus(1)).collect());
        let debug_line = stopped.filter(|(p, _)| *p == path).map(|(_, l)| l.minus(1));
        let inline = self.debug_inline_values(&path);
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_breakpoints(lines);
            t.editor.set_debug_line(debug_line);
            if !inline.is_empty()
                || !t
                    .editor
                    .eol_texts(crate::editor_core::virtual_text::VirtualTextLayer::DebugValues)
                    .is_empty()
            {
                t.editor.set_eol_texts(
                    crate::editor_core::virtual_text::VirtualTextLayer::DebugValues,
                    inline,
                );
            }
        }
    }

    /// The breakpoint lines (1-based) set on the active file. For tests/tools.
    #[must_use]
    pub fn active_breakpoints(&self) -> Vec<usize> {
        self.editor
            .active_tab()
            .and_then(|t| t.path.as_ref())
            .and_then(|p| self.breakpoints.get(p))
            .map_or_default(|s| s.keys().copied().collect())
    }

    /// The active file's breakpoint on 1-based `line`, with its condition,
    /// hit condition and log message (T617). For tests/tools.
    #[must_use]
    pub fn active_breakpoint(&self, line: usize) -> Option<crate::dap::SourceBreakpoint> {
        self.editor
            .active_tab()
            .and_then(|t| t.path.as_ref())
            .and_then(|p| self.breakpoints.get(p))
            .and_then(|s| s.get(&line).cloned())
    }

    /// Open `path` and move the cursor to `line` (1-based) for a debug stop.
    pub(super) fn jump_to_debug_location(&mut self, path: &str, line: usize) {
        let p = PathBuf::from(path);
        if crate::authority::current().is_file(&p) {
            self.with_jump(|s| {
                s.open_path(&p, false);
                let area = s.editor_view();
                s.editor.goto(line, None, area);
                s.focus = Focus::Editor;
            });
        }
    }

    /// Push the active document to its language server when it has changed since
    /// the last sync (sending `didOpen` the first time, then `didChange`).
    pub(super) fn lsp_sync_active(&mut self) {
        let Some(path) = self.active_path() else {
            return;
        };
        if !self.lsp.handles(&path) {
            return;
        }
        let Some(rev) = self.editor.active_tab().map(|t| t.editor.revision()) else {
            return;
        };
        let last = self.lsp_synced.get(&path).copied();
        if last == Some(rev) {
            return;
        }
        let Some(text) = self.editor.active_tab().map(|t| t.editor.get_content()) else {
            return;
        };
        if last.is_none() {
            self.lsp.did_open(&path, &text);
            self.lsp.request_folding_range(&path); // fetch foldable ranges once on open
            self.request_inlay_hints(&path);
            // A (re)opened document gets its semantic tokens afresh.
            self.semantic_requested = None;
        } else {
            // Incremental for a server that negotiated it (T615).
            self.lsp.did_change(&path, &text);
        }
        // Semantic tokens (T123a) are re-requested after every change, not
        // just on open -- a token's classification (e.g. "unused") can
        // change as the user types -- but debounced, by
        // `poll_semantic_tokens` (T615).
        self.lsp_synced.insert(path, rev);
    }

    /// Tell the language server a file closed and forget its sync state.
    pub(super) fn lsp_close(&mut self, path: &Path) {
        if self.lsp_synced.remove(path).is_some() {
            self.lsp.did_close(path);
        }
    }

    /// Whether a language-server request is in flight or starting up (the event
    /// loop ticks faster then, so responses feel prompt).
    #[must_use]
    pub fn lsp_busy(&self) -> bool {
        // A pending debounced auto-completion (T612) needs the fast tick to
        // fire on time.
        self.lsp.busy() || self.completion_due.is_some() || self.semantic_due.is_some()
    }

    /// Rebuild the active editor's diagnostic underline marks from the latest
    /// diagnostics for its file.
    pub(super) fn refresh_diagnostic_marks(&mut self) {
        let Some(path) = self.active_path() else {
            return;
        };
        if !self.lsp.handles(&path) {
            if let Some(t) = self.editor.active_tab_mut() {
                t.editor.clear_diagnostic_marks();
            }
            return;
        }
        let enc = self.lsp.encoding_for(&path);
        let ranges: Vec<(crate::lsp_core::Range, crate::lsp_core::Severity)> = self
            .lsp
            .diagnostics_for(&path)
            .iter()
            .map(|d| (d.range, d.severity))
            .collect();
        let Some(t) = self.editor.active_tab_mut() else {
            return;
        };
        let marks = {
            let code = t.editor.code_ref();
            ranges
                .iter()
                .map(|(range, sev)| {
                    let start = lsp_pos_to_char(code, range.start.line, range.start.character, enc);
                    let mut end = lsp_pos_to_char(code, range.end.line, range.end.character, enc);
                    if end <= start {
                        end = start.plus(1); // make a zero-width diagnostic visible
                    }
                    (start, end, severity_color(*sev))
                })
                .collect::<Vec<_>>()
        };
        t.editor.set_diagnostic_marks(marks);
    }

    /// Open `path` and move the cursor to LSP `(line, character)` (0-based).
    pub(super) fn lsp_jump(&mut self, path: &Path, line: u32, character: u32) {
        let path = path.to_path_buf();
        self.with_jump(|s| {
            s.open_path(&path, false);
            s.focus = Focus::Editor;
            let enc = s.lsp.encoding_for(&path);
            let (target_line, target_col) = {
                let Some(t) = s.editor.active_tab() else {
                    return;
                };
                let code = t.editor.code_ref();
                let ln = lsp_index(line).min(code.len_lines().minus(1));
                let line_start = code.line_to_char(ln);
                let line_text = code.slice(line_start, line_start.plus(code.line_len(ln)));
                let col = crate::lsp_core::position::col_to_char(&line_text, character, enc);
                (ln.plus(1), col.plus(1))
            };
            let area = s.editor_view();
            s.editor.goto(target_line, Some(target_col), area);
        });
    }

    /// Restart the language server(s) for the active file (T576): the host
    /// replays `didOpen` for each file that was open on them on the next
    /// sync tick.
    pub(super) fn lsp_restart(&mut self) {
        let Some(path) = self.active_path() else {
            return;
        };
        if !self.lsp.handles(&path) {
            self.status = t!("status.lsp_inactive").to_string();
            return;
        }
        let reopened = self.lsp.restart(&path);
        self.after_lsp_restart(reopened);
    }

    /// After a restart (T576, or the LSP Status panel's per-server restart,
    /// T732): forget that the `reopened` files were synced so the next sync
    /// tick replays `didOpen` with their real content, and drop the old
    /// server's diagnostic marks.
    pub(super) fn after_lsp_restart(&mut self, reopened: Vec<std::path::PathBuf>) {
        for path in reopened {
            self.lsp_synced.remove(&path);
        }
        self.refresh_diagnostic_marks();
        self.status = t!("status.lsp_restarted").to_string();
    }

    /// Show the active file's language server stderr in a read-only tab
    /// (T576).
    pub(super) fn lsp_show_log(&mut self) {
        let Some(path) = self.active_path() else {
            return;
        };
        if !self.lsp.handles(&path) {
            self.status = t!("status.lsp_inactive").to_string();
            return;
        }
        let mut log = self.lsp.server_log(&path);
        if log.lines().all(|l| l.starts_with("== ")) {
            log.push_str(&t!("status.lsp_log_empty"));
            log.push('\n');
        }
        self.open_readonly_text_tab(&t!("ui.lsp_log_title"), &log);
    }

    /// Request hover info for the symbol under the cursor.
    pub(super) fn lsp_hover(&mut self) {
        let Some(path) = self.active_path() else {
            return;
        };
        if !self.lsp.handles(&path) {
            self.status = t!("status.lsp_inactive").to_string();
            return;
        }
        let (line, character) = self.cursor_lsp_position(&path);
        self.lsp.request_hover(&path, line, character);
    }

    /// Request completion candidates at the cursor.
    pub(super) fn lsp_complete(&mut self) {
        let Some(path) = self.active_path() else {
            return;
        };
        if !self.lsp.handles(&path) {
            self.status = t!("status.lsp_inactive").to_string();
            return;
        }
        let (line, character) = self.cursor_lsp_position(&path);
        self.lsp.request_completion(&path, line, character);
    }

    /// Populate the completion popup with project node titles (each inserting
    /// `Title]]`), filtered by any partial title already typed. No-op when there
    /// are no matching nodes.
    pub(super) fn open_node_link_completion(&mut self) {
        // Append the closing `]]` only when it isn't already there (auto-pair may
        // have inserted it when the user typed `[[`).
        let suffix = self.editor.active_tab().map_or("]]", |t| {
            let cur = t.editor.get_cursor();
            let code = t.editor.code_ref();
            if cur.plus(2) <= code.len_chars() && code.char_slice(cur, cur.plus(2)) == "]]" {
                ""
            } else {
                "]]"
            }
        });
        let prefix = self.word_prefix_before_cursor().to_lowercase();
        let mut titles: Vec<String> = self
            .roam_node_files()
            .iter()
            .filter_map(|(_, c)| crate::roam::node_title(c))
            .filter(|t| prefix.is_empty() || t.to_lowercase().starts_with(&prefix))
            .collect();
        titles.sort_unstable();
        titles.dedup();
        titles.truncate(200);
        if titles.is_empty() {
            self.status = t!("status.roam_no_nodes").to_string();
            return;
        }
        let items = titles
            .into_iter()
            .map(|t| crate::lsp_core::CompletionItem::new(t.clone(), format!("{t}{suffix}")))
            .collect();
        self.open_completion(items);
    }

    /// Populate the completion popup with contact emails (`mailto: true`) or
    /// contact names (`mailto: false`), compiled from every project `.org`
    /// file's Org-contacts entries. No-op (with a status message) when there
    /// are no matching contacts.
    pub(super) fn open_org_contact_link_completion(&mut self, mailto: bool) {
        // Append the closing `]]` only when it isn't already there (auto-pair may
        // have inserted it when the user typed `[[`).
        let suffix = self.editor.active_tab().map_or("]]", |t| {
            let cur = t.editor.get_cursor();
            let code = t.editor.code_ref();
            if cur.plus(2) <= code.len_chars() && code.char_slice(cur, cur.plus(2)) == "]]" {
                ""
            } else {
                "]]"
            }
        });
        let contacts = crate::org_contacts::all(&self.roam_node_files());
        let mut items: Vec<crate::lsp_core::CompletionItem> = if mailto {
            contacts
                .iter()
                .filter_map(|c| {
                    let email = c.field("EMAIL")?;
                    Some(crate::lsp_core::CompletionItem::new(
                        format!("{} <{email}>", c.name),
                        format!("{email}][{}{suffix}", c.name),
                    ))
                })
                .collect()
        } else {
            contacts
                .iter()
                .map(|c| {
                    crate::lsp_core::CompletionItem::new(
                        c.name.clone(),
                        format!("{}{suffix}", c.name),
                    )
                })
                .collect()
        };
        items.sort_by(|a, b| a.label.cmp(&b.label));
        items.dedup_by(|a, b| a.label == b.label);
        items.truncate(200);
        if items.is_empty() {
            self.status = t!("status.org_contacts_no_matches").to_string();
            return;
        }
        self.open_completion(items);
    }

    /// Go to the definition of the symbol under the cursor. When a language
    /// server handles the active file, ask it (`textDocument/definition`, the
    /// result arrives asynchronously and jumps via [`App::poll_lsp`]). Otherwise
    /// fall back to the heuristic, keyword-prefixed cross-workspace search below.
    /// Request LSP formatting of the active document (or selection, when one
    /// exists — via range formatting).
    pub(super) fn lsp_format(&mut self) {
        let Some(path) = self.active_path() else {
            return;
        };
        // T683: a language listed in `save.prettier_languages` formats with
        // the user's Prettier instead of the language server.
        if self.prettier_format_active() != super::prettier::PrettierOutcome::NotApplicable {
            return;
        }
        if !self.lsp.handles(&path) {
            self.status = t!("status.lsp_inactive").to_string();
            return;
        }
        // T652: sync first so the request (and the version its reply is
        // checked against) reflects the buffer as it is now.
        self.lsp_sync_active();
        let tab_size = u32::try_from(self.buffer_settings().tab_width).unwrap_or(4);
        let enc = self.lsp.encoding_for(&path);
        let sel = self.editor.active_tab_mut().and_then(|t| {
            let s = t.editor.get_selection()?;
            if s.is_empty() {
                return None;
            }
            let code = t.editor.code_ref();
            Some((
                char_to_lsp_pos(code, s.start.min(s.end), enc),
                char_to_lsp_pos(code, s.start.max(s.end), enc),
            ))
        });
        match sel {
            Some((start, end)) => self
                .lsp
                .request_range_formatting(&path, start, end, tab_size),
            None => self.lsp.request_formatting(&path, tab_size),
        }
    }

    /// After an editor key (T624): with `format_on_type` on, a typed
    /// character (or Enter, as `\n`) that the file's language server lists in
    /// `documentOnTypeFormattingProvider` sends `textDocument/onTypeFormatting`
    /// for the caret. The reply is applied like a formatting reply, and
    /// dropped as stale if the buffer changes first. Single-caret only.
    pub(super) fn on_type_formatting_after_key(&mut self, key: KeyEvent) {
        if !self.settings.completion.format_on_type || self.multi_caret_active() {
            return;
        }
        let ch = match key.code {
            KeyCode::Char(c) if !Self::ctrl(&key) && !Self::alt(&key) => c,
            KeyCode::Enter => '\n',
            _ => return,
        };
        let Some(path) = self.active_path() else {
            return;
        };
        if !self.lsp.handles(&path) || !self.lsp.is_on_type_trigger(&path, ch) {
            return;
        }
        // Sync first so the server formats the text with the typed character
        // in it, and the reply's version check passes.
        self.lsp_sync_active();
        let tab_size = u32::try_from(self.buffer_settings().tab_width).unwrap_or(4);
        let (line, character) = self.cursor_lsp_position(&path);
        self.lsp
            .request_on_type_formatting(&path, line, character, ch, tab_size);
    }

    /// Install a folding-range reply's `(start_line, end_line)` ranges on the
    /// active editor.
    pub(super) fn apply_folding_ranges(&mut self, ranges: &[(u32, u32)]) {
        let ranges: Vec<(usize, usize)> = ranges
            .iter()
            .map(|&(s, e)| (lsp_index(s), lsp_index(e)))
            .collect();
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_fold_ranges(ranges);
        }
    }

    /// Act on one edit-returning LSP reply (T652): formatting edits go to the
    /// tab of the file they were requested for, linked ranges start a linked
    /// edit, and a stale reply is dropped.
    pub(super) fn apply_edit_reply(&mut self, reply: crate::lsp::EditReply) {
        use crate::lsp::EditReplyKind;
        match reply.kind {
            EditReplyKind::Edits(edits) => self.apply_lsp_edits(&reply.path, &edits),
            EditReplyKind::LinkedRanges(ranges) => self.begin_linked_edit(&reply.path, &ranges),
            // `Stale`, and any kind a later vix-lsp adds that this host
            // doesn't know how to apply.
            _ => self.drop_stale_lsp_edits(&reply.path),
        }
    }

    /// Whether `path`'s open tab still holds exactly the text last synced to
    /// the language server -- the text an in-flight request's reply was
    /// computed against (T652). `None` when `path` has no open tab.
    fn lsp_tab_in_sync(&self, path: &Path) -> Option<(usize, bool)> {
        let idx = self
            .editor
            .tabs
            .iter()
            .position(|t| t.path.as_deref() == Some(path))?;
        let synced = self.lsp_synced.get(path).copied();
        let revision = self.editor.tabs.get(idx)?.editor.revision();
        Some((idx, synced == Some(revision)))
    }

    /// Apply a formatting reply's text edits to `path`'s tab -- the file the
    /// request was for, not whichever tab is active now (T652) -- highest
    /// position first so earlier offsets stay valid, then re-anchor the caret.
    /// Dropped (see [`App::drop_stale_lsp_edits`]) if that tab was closed or
    /// edited since the request was sent. A pending format-on-save of `path`
    /// writes the formatted tab back to disk.
    pub(super) fn apply_lsp_edits(
        &mut self,
        path: &Path,
        edits: &[(crate::lsp_core::Range, String)],
    ) {
        let Some((idx, true)) = self.lsp_tab_in_sync(path) else {
            self.drop_stale_lsp_edits(path);
            return;
        };
        let enc = self.lsp.encoding_for(path);
        let Some(t) = self.editor.tabs.get_mut(idx) else {
            return;
        };
        // Resolve every range to char offsets first, then apply tail-to-head.
        let mut resolved: Vec<(usize, usize, String)> = {
            let code = t.editor.code_ref();
            edits
                .iter()
                .map(|(r, text)| {
                    let start = lsp_pos_to_char(code, r.start.line, r.start.character, enc);
                    let end = lsp_pos_to_char(code, r.end.line, r.end.character, enc);
                    (start.min(end), start.max(end), text.clone())
                })
                .collect()
        };
        resolved.sort_by_key(|e| std::cmp::Reverse(e.0));
        let mut content: Vec<char> = t.editor.get_content().chars().collect();
        for (start, end, text) in resolved {
            let (a, b) = (start.min(content.len()), end.min(content.len()));
            if a <= b {
                content.splice(a..b, text.chars());
            }
        }
        let new_content: String = content.into_iter().collect();
        let caret = t.editor.get_cursor().min(new_content.chars().count());
        t.editor.set_content(&new_content);
        t.editor.set_cursor(caret);
        t.dirty = true;
        t.preview = false;
        self.status = t!("status.formatted").to_string();
        // If these edits were a format-on-save, write the formatted buffer back
        // -- to its own file, even if another tab is active by now.
        if self.format_save_pending.as_deref() == Some(path) {
            self.format_save_pending = None;
            let active = std::mem::replace(&mut self.editor.active, idx);
            self.write_active_to_disk();
            self.editor.active = active;
        }
    }

    /// An edit-returning LSP reply for `path` arrived after the document
    /// changed (or closed) and was dropped rather than applied to text it
    /// wasn't computed for (T652). Says so, and cancels a pending
    /// format-on-save of `path` -- the plain save already wrote the file.
    pub(super) fn drop_stale_lsp_edits(&mut self, path: &Path) {
        if self.format_save_pending.as_deref() == Some(path) {
            self.format_save_pending = None;
        }
        self.status = t!("status.lsp_edits_stale").to_string();
    }

    /// Jump to the next (`forward`) or previous diagnostic ("issue") in the active
    /// file, wrapping around. Reports when the file has no diagnostics.
    pub(super) fn goto_diagnostic(&mut self, forward: bool) {
        let Some(path) = self.active_path() else {
            return;
        };
        let mut lines: Vec<u32> = self
            .lsp
            .all_diagnostics()
            .filter(|(p, _)| **p == path)
            .flat_map(|(_, d)| d.into_iter().map(|x| x.range.start.line))
            .collect();
        if lines.is_empty() {
            self.status = t!("status.no_diagnostics").to_string();
            return;
        }
        lines.sort_unstable();
        lines.dedup();
        let cur = u32::try_from(self.editor.cursor_line()).unwrap_or(0);
        let target = if forward {
            lines
                .iter()
                .find(|&&l| l > cur)
                .copied()
                .unwrap_or_else(|| lines.first().copied().unwrap_or(cur))
        } else {
            lines
                .iter()
                .rev()
                .find(|&&l| l < cur)
                .copied()
                .unwrap_or_else(|| lines.last().copied().unwrap_or(cur))
        };
        let area = self.editor_view();
        // `target` is a 0-based LSP line; `goto` takes a 1-based one (T614:
        // passing it unconverted landed one line above the diagnostic).
        self.editor.goto(lsp_index(target).plus(1), None, area);
        self.show_cursor_diagnostic();
    }

    /// On a linked-editing response: store the ranges (as char offsets) and
    /// prompt for the shared replacement text, seeded with the current text.
    /// Only for `path`'s tab while it is still active and unchanged since the
    /// request (T652); otherwise the ranges are dropped as stale.
    pub(super) fn begin_linked_edit(&mut self, path: &Path, ranges: &[crate::lsp_core::Range]) {
        if self.active_path().as_deref() != Some(path)
            || !matches!(self.lsp_tab_in_sync(path), Some((_, true)))
        {
            self.drop_stale_lsp_edits(path);
            return;
        }
        let enc = self.lsp.encoding_for(path);
        let Some(t) = self.editor.active_tab() else {
            return;
        };
        let (offsets, seed) = {
            let code = t.editor.code_ref();
            let offsets: Vec<(usize, usize)> = ranges
                .iter()
                .map(|r| {
                    let s = lsp_pos_to_char(code, r.start.line, r.start.character, enc);
                    let e = lsp_pos_to_char(code, r.end.line, r.end.character, enc);
                    (s.min(e), s.max(e))
                })
                .collect();
            let seed = offsets.first().map_or_default(|&(s, e)| code.slice(s, e));
            (offsets, seed)
        };
        self.linked_ranges = Some(offsets);
        self.prompt = Some(
            Prompt::new(PromptKind::LinkedEdit, t!("prompt.linked_edit").to_string())
                .with_input(seed),
        );
    }

    /// Apply a rename `WorkspaceEdit`: edit open buffers in place (marking them
    /// dirty) and rewrite closed files on disk.
    pub(super) fn apply_workspace_edit(&mut self, edits: &[crate::lsp::FileEdits]) {
        let mut files = 0usize;
        let auth = crate::authority::current();
        for (path, file_edits) in edits {
            if file_edits.is_empty() {
                continue;
            }
            let enc = self.lsp.encoding_for(path);
            if let Some(tab) = self
                .editor
                .tabs
                .iter_mut()
                .find(|t| t.path.as_deref() == Some(path.as_path()))
            {
                let text = tab.editor.get_content();
                let new = apply_edits_to_text(&text, enc, file_edits);
                if new != text {
                    let caret = tab.editor.get_cursor().min(new.chars().count());
                    tab.editor.set_content(&new);
                    tab.editor.set_cursor(caret);
                    tab.dirty = true;
                    tab.preview = false;
                    files = files.plus(1);
                }
            } else if let Ok(text) = auth.read_to_string(path) {
                let new = apply_edits_to_text(&text, enc, file_edits);
                if new != text && auth.write(path, new.as_bytes()).is_ok() {
                    files = files.plus(1);
                }
            }
        }
        self.refresh_git();
        self.status = t!("status.renamed_in", n = files).to_string();
    }

    /// Tell language servers about workspace manifests and open files that
    /// changed on disk (T623, `workspace/didChangeWatchedFiles`). `changed`
    /// holds open files whose mtime moved; manifests are stat'ed here whether
    /// or not they are open, so a server notices an edit to `Cargo.toml` or
    /// `package.json` made outside Vix. Does nothing unless a server registered
    /// watchers.
    pub(super) fn forward_watched_files(&mut self, changed: Vec<PathBuf>) {
        // Project manifest/lock files stat'ed on every watched-files poll, whether or not they
        // are open.
        const MANIFESTS: [&str; 8] = [
            "Cargo.toml",
            "Cargo.lock",
            "package.json",
            "tsconfig.json",
            "pyproject.toml",
            "go.mod",
            "go.sum",
            "deno.json",
        ];
        if !self.lsp.watches_files() {
            self.reset_watch_scan();
            return;
        }
        let mut events: Vec<(PathBuf, crate::lsp_core::file_ops::FileChangeType)> = changed
            .into_iter()
            .map(|p| (p, crate::lsp_core::file_ops::FileChangeType::Changed))
            .collect();
        for name in MANIFESTS {
            let path = self.root.join(name);
            let now = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            // The first sighting only records the state.
            let Some(previous) = self.manifest_mtimes.insert(path.clone(), now) else {
                continue;
            };
            let kind = match (previous, now) {
                (None, Some(_)) => crate::lsp_core::file_ops::FileChangeType::Created,
                (Some(_), None) => crate::lsp_core::file_ops::FileChangeType::Deleted,
                (Some(a), Some(b)) if a != b => crate::lsp_core::file_ops::FileChangeType::Changed,
                _ => continue,
            };
            if !events.iter().any(|(p, _)| *p == path) {
                events.push((path, kind));
            }
        }
        // Registered glob watchers see files nobody has open, too (bounded scan).
        self.scan_watched_files(&mut events);
        if !events.is_empty() {
            self.lsp.did_change_watched_files(&events);
        }
    }

    /// Request signature help at the cursor (LSP).
    pub(super) fn lsp_signature_help(&mut self) {
        if let Some(path) = self.active_path()
            && self.lsp.handles(&path)
        {
            let (line, character) = self.cursor_lsp_position(&path);
            self.lsp.request_signature_help(&path, line, character);
        } else {
            self.status = t!("status.lsp_inactive").to_string();
        }
    }

    /// Open a panel listing every current LSP diagnostic across the workspace;
    /// Enter on a row jumps to it. Reuses the static-results search overlay.
    pub(super) fn open_diagnostics_panel(&mut self) {
        use crate::lsp_core::Severity;
        // T123d: pull a fresh workspace-wide report too, not just whatever
        // push (`publishDiagnostics`) has accumulated for files that
        // happen to have been opened/synced already -- a server that
        // supports pull can now surface its whole-project analysis here.
        // Best-effort and async: this panel still renders from whatever is
        // already in `self.lsp` below, and the pull's results (if any)
        // arrive on a later `poll_lsp` tick.
        self.lsp.request_workspace_diagnostics();
        // Each diagnostic's own `Hit` paired with its (possibly empty)
        // `relatedInformation` rows (T134 audit: parsed but never shown
        // before this) -- kept grouped so sorting reorders diagnostics
        // without scattering a diagnostic's notes away from it.
        let mut groups: Vec<(Hit, Vec<Hit>)> = Vec::new();
        for (path, diags) in self.lsp.all_diagnostics() {
            let rel = self.root_relative(path);
            for d in diags {
                let sev = match d.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                    Severity::Information => "info",
                    Severity::Hint => "hint",
                };
                let line = lsp_index(d.range.start.line).plus(1);
                let msg: String = d
                    .message
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(100)
                    .collect();
                let parent = Hit {
                    path: path.clone(),
                    rel: rel.clone(),
                    line,
                    col: lsp_index(d.range.start.character).plus(1),
                    display: format!("{rel}:{line}: [{sev}] {msg}"),
                    text: msg,
                };
                let related = d
                    .related
                    .iter()
                    .map(|(related_loc, related_msg)| {
                        let related_path = crate::lsp::uri_to_path(&related_loc.uri);
                        let related_rel = self.root_relative(&related_path);
                        let related_line = lsp_index(related_loc.range.start.line).plus(1);
                        let text: String = related_msg.chars().take(100).collect();
                        Hit {
                            path: related_path,
                            rel: related_rel.clone(),
                            line: related_line,
                            col: lsp_index(related_loc.range.start.character).plus(1),
                            display: format!("    ↳ {related_rel}:{related_line}: {text}"),
                            text,
                        }
                    })
                    .collect();
                groups.push((parent, related));
            }
        }
        // Problems matched in the last task's output, after the servers'
        // diagnostics, as their own source (T632).
        let task_hits = self.task_problem_hits();
        if groups.is_empty() && task_hits.is_empty() {
            self.status = t!("status.no_diagnostics").to_string();
            return;
        }
        groups.sort_by(|a, b| a.0.display.cmp(&b.0.display));
        let count = groups.len().plus(task_hits.len());
        let hits: Vec<Hit> = groups
            .into_iter()
            .flat_map(|(parent, related)| std::iter::once(parent).chain(related))
            .chain(task_hits)
            .collect();
        let mut ps = WorkspaceSearch::new(false);
        ps.flags.insert(WorkspaceFlags::STATIC_RESULTS);
        ps.title = Some(t!("ui.results_diagnostics").to_string());
        ps.status = t!("status.diagnostics_n", n = count).to_string();
        ps.hits = hits;
        self.workspace_search = Some(ps);
    }

    /// Handle a completed debugger prompt (REPL evaluate, add-watch, the
    /// Edit Breakpoint steps, set variable). Grouped out of
    /// [`App::accept_prompt`] to keep it within the line limit.
    pub(super) fn accept_debug_prompt(&mut self, kind: PromptKind, expr: &str) {
        match kind {
            PromptKind::DebugBreakpointCondition
            | PromptKind::DebugBreakpointHit
            | PromptKind::DebugBreakpointLog => self.accept_breakpoint_prompt(kind, expr),
            PromptKind::DebugSetVariable => self.accept_set_variable(expr),
            PromptKind::DebugFunctionBreakpoint => self.accept_function_breakpoint(expr),
            PromptKind::DebugInstructionBreakpoint => self.accept_instruction_breakpoint(expr),
            PromptKind::DebugRepl if !expr.is_empty() => self.dap.evaluate(expr),
            PromptKind::DebugWatch if !expr.is_empty() => {
                self.dap_view
                    .watches
                    .push((expr.to_string(), String::new()));
                self.dap.evaluate(expr);
                self.visible.set(crate::app::Visible::DEBUG_PANEL, true);
            }
            _ => {}
        }
    }
}

/// Map an LSP semantic-token type name onto the closest existing theme
/// syntax capture name (T123a), or `None` when there is no reasonable match
/// yet — every bundled theme (`themes/*.json`) currently defines exactly
/// four syntax colors (`comment`/`keyword`/`number`/`string`), a real
/// finding from implementing this task, not an assumption going in. Richer
/// LSP distinctions (`function`, `type`, `variable`, and the modifier-driven
/// ones like mutable/deprecated/unused this task's own description named as
/// the headline motivation) have nowhere to render yet without a theme
/// schema change — see `crates/vix-lsp/spec/index.md`'s T123a note for the
/// follow-up this was scoped down to avoid rushing.
fn semantic_token_capture(lsp_token_type: &str) -> Option<&'static str> {
    match lsp_token_type {
        "comment" => Some("comment"),
        "keyword" | "modifier" => Some("keyword"),
        "number" => Some("number"),
        "string" | "regexp" => Some("string"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp_core::{Position, Range};
    use crate::settings::Settings;
    use vix_input::KeyModifiers;

    fn range(l0: u32, c0: u32, l1: u32, c1: u32) -> Range {
        Range {
            start: Position {
                line: l0,
                character: c0,
            },
            end: Position {
                line: l1,
                character: c1,
            },
        }
    }

    /// A unique scratch directory holding `name` with `content`, opened in an app.
    fn app_with_file(tag: &str, name: &str, content: &str) -> (App, PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("vix-r2c-lspdap-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(name);
        std::fs::write(&file, content).unwrap();
        let mut app = App::new(dir.clone(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.open_initial(&file);
        // The tab's own spelling of the path (symlinked temp dirs resolve).
        let file = app.editor.active_tab().unwrap().path.clone().unwrap_or(file);
        (app, dir, file)
    }

    fn text(app: &App) -> String {
        app.editor.active_tab().unwrap().text()
    }

    fn adapter(ext: &str) -> crate::settings::DebugAdapter {
        toml::from_str(&format!(
            "adapter_id = \"fake\"\nextensions = [\"{ext}\"]\ncommand = [\"/nonexistent/vix-r2c-adapter\"]\n"
        ))
        .unwrap()
    }

    #[test]
    fn the_run_actions_open_prompts_toggle_the_panel_and_reject_unknown_ids() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        assert!(!app.run_dap_action("run.nonsense"));
        assert!(app.run_dap_action("run.repl"));
        assert!(matches!(app.prompt.as_ref().unwrap().kind, PromptKind::DebugRepl));
        assert!(app.run_dap_action("run.watch"));
        assert!(matches!(app.prompt.as_ref().unwrap().kind, PromptKind::DebugWatch));
        let shown = app.visible.contains(crate::app::Visible::DEBUG_PANEL);
        assert!(app.run_dap_action("run.panel"));
        assert_ne!(shown, app.visible.contains(crate::app::Visible::DEBUG_PANEL));
        assert!(app.run_dap_action("run.pause"));
    }

    #[test]
    fn starting_the_debugger_reports_what_is_missing() {
        let (mut app, dir, _file) = app_with_file("dbg-start", "main.zz", "x\n");
        app.run_dap_action("run.start");
        assert_eq!(app.status, t!("status.debug_no_adapter").to_string());
        app.settings.debug_adapters = vec![adapter("zz")];
        app.run_dap_action("run.attach");
        // The adapter binary does not exist, so the start fails cleanly.
        assert_eq!(app.status, t!("status.debug_failed").to_string());
        let mut bare = App::new(std::env::temp_dir(), Settings::default());
        bare.editor.active_tab_mut().unwrap().path = None;
        bare.run_dap_action("run.start");
        assert_eq!(bare.status, t!("status.debug_no_file").to_string());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn stopping_the_debugger_resets_state_and_focus() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.dap_stopped = Some((PathBuf::from("a.rs"), 3));
        app.focus = Focus::Debug;
        app.run_dap_action("run.stop");
        assert!(app.dap_stopped.is_none());
        assert_eq!(app.focus, Focus::Editor);
        assert_eq!(app.status, t!("status.debug_stopped").to_string());
    }

    #[test]
    fn breakpoints_toggle_on_the_cursor_line_and_need_a_file() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.editor.active_tab_mut().unwrap().path = None;
        app.run_dap_action("run.toggle_breakpoint");
        assert_eq!(app.status, t!("status.blame_no_file").to_string());

        let (mut app, dir, _f) = app_with_file("bp", "a.rs", "one\ntwo\nthree\n");
        app.editor.goto(2, None, ratatui::layout::Rect::new(0, 0, 80, 24));
        app.run_dap_action("run.toggle_breakpoint");
        assert_eq!(app.active_breakpoints(), vec![2]);
        assert_eq!(app.active_breakpoint(2).unwrap().line, 2);
        assert!(app.active_breakpoint(1).is_none());
        app.run_dap_action("run.toggle_breakpoint");
        assert!(app.active_breakpoints().is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_stopped_line_and_breakpoints_reach_the_active_tab() {
        let (mut app, dir, file) = app_with_file("stop", "a.rs", "one\ntwo\nthree\n");
        app.editor.goto(3, None, ratatui::layout::Rect::new(0, 0, 80, 24));
        app.toggle_breakpoint();
        app.dap_stopped = Some((file.clone(), 2));
        app.refresh_debug_markers();
        assert_eq!(app.active_breakpoints(), vec![3]);
        // A stop in another file leaves this buffer without a current line.
        app.dap_stopped = Some((dir.join("other.rs"), 1));
        app.refresh_debug_markers();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn debug_prompts_add_watches_and_ignore_empty_input() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.accept_debug_prompt(PromptKind::DebugWatch, "");
        assert!(app.dap_view.watches.is_empty());
        app.accept_debug_prompt(PromptKind::DebugWatch, "x + 1");
        assert_eq!(app.dap_view.watches.len(), 1);
        assert_eq!(app.dap_view.watches[0].0, "x + 1");
        assert!(app.visible.contains(crate::app::Visible::DEBUG_PANEL));
        app.accept_debug_prompt(PromptKind::DebugRepl, "");
        app.accept_debug_prompt(PromptKind::DebugRepl, "1 + 1");
        app.accept_debug_prompt(PromptKind::LinkedEdit, "ignored");
    }

    #[test]
    fn jumping_to_a_debug_location_opens_the_file_at_the_line() {
        let (mut app, dir, file) = app_with_file("jump", "a.txt", "one\ntwo\nthree\n");
        let other = dir.join("b.txt");
        std::fs::write(&other, "x\ny\nz\nw\n").unwrap();
        app.jump_to_debug_location(&other.to_string_lossy(), 3);
        assert_eq!(app.editor.cursor_1based().0, 3);
        assert_eq!(app.focus, Focus::Editor);
        // A missing file is ignored.
        app.jump_to_debug_location(&dir.join("gone.txt").to_string_lossy(), 1);
        assert_eq!(app.editor.cursor_1based().0, 3);
        let _ = file;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn inlay_hints_toggle_and_only_apply_while_shown() {
        let (mut app, dir, _f) = app_with_file("inlay", "a.rs", "let x = 1;\nlet y = 2;\n");
        let hints = vec![
            (0, 5, ": i32".to_string()),
            (1, 5, ": i32".to_string()),
            (99, 0, "beyond the end".to_string()),
        ];
        let on = app.visible.contains(crate::app::Visible::INLAY_HINTS);
        if on {
            app.toggle_inlay_hints();
        }
        app.apply_inlay_hints(&hints);
        assert!(!app.editor.active_tab().unwrap().editor.has_inlay_hints());
        app.toggle_inlay_hints();
        assert_eq!(app.status, t!("status.inlay_on").to_string());
        app.apply_inlay_hints(&hints);
        assert!(app.editor.active_tab().unwrap().editor.has_inlay_hints());
        app.toggle_inlay_hints();
        assert_eq!(app.status, t!("status.inlay_off").to_string());
        assert!(!app.editor.active_tab().unwrap().editor.has_inlay_hints());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn document_highlights_mark_every_occurrence() {
        let (mut app, dir, _f) = app_with_file("hl", "a.rs", "foo bar foo\n");
        app.apply_document_highlights(&[range(0, 0, 0, 3), range(0, 8, 0, 11)]);
        let marks = app.editor.active_tab().unwrap().editor.marks().unwrap().clone();
        assert_eq!(marks.len(), 2);
        assert_eq!((marks[0].0, marks[0].1), (0, 3));
        assert_eq!((marks[1].0, marks[1].1), (8, 11));
        assert_eq!(app.status, t!("status.highlights_n", n = 2).to_string());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn selection_ranges_expand_to_the_next_larger_and_shrink_back() {
        let (mut app, dir, _f) = app_with_file("sel", "a.rs", "fn main() { call(arg); }\n");
        let chain = [range(0, 17, 0, 20), range(0, 12, 0, 22), range(0, 0, 0, 24)];
        app.editor.active_tab_mut().unwrap().editor.set_cursor(18);
        app.expand_selection_dir = Some(true);
        app.apply_selection_range(&chain);
        let sel = |a: &mut App| {
            let s = a.editor.active_tab_mut().unwrap().editor.get_selection().unwrap();
            (s.start, s.end)
        };
        assert_eq!(sel(&mut app), (17, 20));
        app.expand_selection_dir = Some(true);
        app.apply_selection_range(&chain);
        assert_eq!(sel(&mut app), (12, 22));
        app.expand_selection_dir = Some(true);
        app.apply_selection_range(&chain);
        assert_eq!(sel(&mut app), (0, 24));
        // Shrinking picks the largest range strictly inside the selection.
        app.expand_selection_dir = Some(false);
        app.apply_selection_range(&chain);
        assert_eq!(sel(&mut app), (12, 22));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn semantic_token_types_map_onto_theme_captures() {
        assert_eq!(semantic_token_capture("comment"), Some("comment"));
        assert_eq!(semantic_token_capture("keyword"), Some("keyword"));
        assert_eq!(semantic_token_capture("modifier"), Some("keyword"));
        assert_eq!(semantic_token_capture("number"), Some("number"));
        assert_eq!(semantic_token_capture("string"), Some("string"));
        assert_eq!(semantic_token_capture("regexp"), Some("string"));
        assert_eq!(semantic_token_capture("variable"), None);
        // Tokens convert without disturbing the buffer text.
        let (mut app, dir, _f) = app_with_file("sem", "a.rs", "let s = \"hi\";\n");
        app.apply_semantic_tokens(&[
            crate::lsp_core::SemanticToken {
                line: 0,
                character: 8,
                length: 4,
                token_type: "string".to_string(),
            },
            crate::lsp_core::SemanticToken {
                line: 9,
                character: 0,
                length: 1,
                token_type: "string".to_string(),
            },
            crate::lsp_core::SemanticToken {
                line: 0,
                character: 0,
                length: 3,
                token_type: "function".to_string(),
            },
        ]);
        assert_eq!(text(&app), "let s = \"hi\";\n");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn folding_ranges_become_fold_markers() {
        let (mut app, dir, _f) = app_with_file("fold", "a.rs", "a {\n b\n}\nc\n");
        app.apply_folding_ranges(&[(0, 2)]);
        let ed = &app.editor.active_tab().unwrap().editor;
        assert_eq!(ed.fold_marker(0), Some(false));
        assert_eq!(ed.fold_marker(3), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_in_sync_edit_reply_rewrites_the_buffer_highest_first() {
        let (mut app, dir, file) = app_with_file("edits", "a.rs", "let a = 1;\nlet b = 2;\n");
        let rev = app.editor.active_tab().unwrap().editor.revision();
        app.lsp_synced.insert(file.clone(), rev);
        app.apply_lsp_edits(
            &file,
            &[
                (range(0, 4, 0, 5), "alpha".to_string()),
                (range(1, 4, 1, 5), "beta".to_string()),
            ],
        );
        assert_eq!(text(&app), "let alpha = 1;\nlet beta = 2;\n");
        assert!(app.editor.active_tab().unwrap().dirty);
        assert_eq!(app.status, t!("status.formatted").to_string());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_stale_edit_reply_is_dropped_and_cancels_a_pending_format_save() {
        let (mut app, dir, file) = app_with_file("stale", "a.rs", "x\n");
        // Never synced: the buffer is not the text the reply was computed for.
        app.format_save_pending = Some(file.clone());
        app.apply_lsp_edits(&file, &[(range(0, 0, 0, 1), "y".to_string())]);
        assert_eq!(text(&app), "x\n");
        assert!(app.format_save_pending.is_none());
        assert_eq!(app.status, t!("status.lsp_edits_stale").to_string());
        app.status.clear();
        app.drop_stale_lsp_edits(&file);
        assert_eq!(app.status, t!("status.lsp_edits_stale").to_string());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_format_on_save_reply_writes_the_formatted_buffer_to_disk() {
        let (mut app, dir, file) = app_with_file("fmtsave", "a.rs", "let   a=1;\n");
        let rev = app.editor.active_tab().unwrap().editor.revision();
        app.lsp_synced.insert(file.clone(), rev);
        app.format_save_pending = Some(file.clone());
        app.apply_lsp_edits(&file, &[(range(0, 0, 0, 10), "let a = 1;".to_string())]);
        assert!(app.format_save_pending.is_none());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "let a = 1;\n");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn linked_ranges_prompt_for_the_shared_text() {
        let (mut app, dir, file) = app_with_file("linked", "a.html", "<div></div>\n");
        // Out of sync: dropped.
        app.begin_linked_edit(&file, &[range(0, 1, 0, 4), range(0, 7, 0, 10)]);
        assert!(app.prompt.is_none());
        assert_eq!(app.status, t!("status.lsp_edits_stale").to_string());
        let rev = app.editor.active_tab().unwrap().editor.revision();
        app.lsp_synced.insert(file.clone(), rev);
        app.begin_linked_edit(&file, &[range(0, 1, 0, 4), range(0, 7, 0, 10)]);
        let p = app.prompt.as_ref().unwrap();
        assert!(matches!(p.kind, PromptKind::LinkedEdit));
        assert_eq!(p.input, "div");
        assert_eq!(app.linked_ranges, Some(vec![(1, 4), (7, 10)]));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_workspace_edit_changes_open_buffers_and_closed_files() {
        let (mut app, dir, file) = app_with_file("wsedit", "a.rs", "let old = 1;\n");
        let closed = dir.join("b.rs");
        std::fs::write(&closed, "use old;\n").unwrap();
        app.apply_workspace_edit(&[
            (file.clone(), vec![(range(0, 4, 0, 7), "new".to_string())]),
            (closed.clone(), vec![(range(0, 4, 0, 7), "new".to_string())]),
            (dir.join("unchanged.rs"), Vec::new()),
        ]);
        assert_eq!(text(&app), "let new = 1;\n");
        assert!(app.editor.active_tab().unwrap().dirty);
        assert_eq!(std::fs::read_to_string(&closed).unwrap(), "use new;\n");
        assert_eq!(app.status, t!("status.renamed_in", n = 2).to_string());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lsp_jump_opens_the_file_at_the_utf16_column() {
        let (mut app, dir, _f) = app_with_file("lspjump", "a.txt", "one\n");
        let target = dir.join("t.txt");
        std::fs::write(&target, "zero\nabc def\nlast\n").unwrap();
        app.lsp_jump(&target, 1, 4);
        assert_eq!(app.editor.cursor_1based(), (2, 5));
        // A line past the end clamps to the last line.
        app.lsp_jump(&target, 99, 0);
        assert_eq!(app.editor.cursor_1based().0, 4);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn language_server_requests_say_so_when_no_server_handles_the_file() {
        let (mut app, dir, _f) = app_with_file("inactive", "a.txt", "x\n");
        let inactive = t!("status.lsp_inactive").to_string();
        for call in [
            App::lsp_restart as fn(&mut App),
            App::lsp_show_log,
            App::lsp_hover,
            App::lsp_complete,
            App::lsp_format,
            App::lsp_signature_help,
        ] {
            app.status.clear();
            call(&mut app);
            assert_eq!(app.status, inactive);
        }
        // Syncing and diagnostics without a server are quiet no-ops.
        app.lsp_sync_active();
        assert!(app.lsp_synced.is_empty());
        app.refresh_diagnostic_marks();
        app.goto_diagnostic(true);
        assert_eq!(app.status, t!("status.no_diagnostics").to_string());
        app.status.clear();
        app.open_diagnostics_panel();
        assert_eq!(app.status, t!("status.no_diagnostics").to_string());
        assert!(app.workspace_search.is_none());
        app.after_lsp_restart(vec![PathBuf::from("x")]);
        assert_eq!(app.status, t!("status.lsp_restarted").to_string());
        assert!(!app.lsp_busy());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn format_on_type_ignores_keys_without_a_trigger_server() {
        let (mut app, dir, _f) = app_with_file("ontype", "a.rs", "x\n");
        let key = |c| KeyEvent::new(c, KeyModifiers::NONE);
        app.settings.completion.format_on_type = false;
        app.on_type_formatting_after_key(key(KeyCode::Char(';')));
        app.settings.completion.format_on_type = true;
        app.on_type_formatting_after_key(key(KeyCode::Char(';')));
        app.on_type_formatting_after_key(key(KeyCode::Enter));
        app.on_type_formatting_after_key(key(KeyCode::Left));
        app.on_type_formatting_after_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
        assert_eq!(text(&app), "x\n");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn wiki_link_completion_offers_org_node_titles() {
        let (mut app, dir, _f) = app_with_file("roam", "main.org", "see [[");
        std::fs::write(dir.join("alpha.org"), "#+title: Alpha Note\n").unwrap();
        std::fs::write(dir.join("beta.org"), "#+title: Beta Note\n").unwrap();
        app.file_index = vec![dir.join("alpha.org"), dir.join("beta.org")];
        app.open_node_link_completion();
        let popup = app.completion.as_ref().expect("titles offered");
        let labels: Vec<&str> = popup.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["Alpha Note", "Beta Note"]);
        assert!(popup.items[0].insert_text.ends_with("]]"));
        app.completion = None;
        app.file_index.clear();
        app.open_node_link_completion();
        assert!(app.completion.is_none());
        assert_eq!(app.status, t!("status.roam_no_nodes").to_string());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn org_contact_completion_offers_names_and_mailto_links() {
        let (mut app, dir, _f) = app_with_file("contacts", "main.org", "mail [[");
        std::fs::write(
            dir.join("people.org"),
            "* Ada Lovelace\n:PROPERTIES:\n:EMAIL: ada@example.org\n:END:\n* Bob\n:PROPERTIES:\n:PHONE: 1\n:END:\n",
        )
        .unwrap();
        app.file_index = vec![dir.join("people.org")];
        app.open_org_contact_link_completion(true);
        let popup = app.completion.as_ref().unwrap();
        assert_eq!(popup.items.len(), 1);
        assert!(popup.items[0].label.contains("ada@example.org"));
        app.completion = None;
        app.open_org_contact_link_completion(false);
        let labels: Vec<&str> = app
            .completion
            .as_ref()
            .unwrap()
            .items
            .iter()
            .map(|i| i.label.as_str())
            .collect();
        assert_eq!(labels, vec!["Ada Lovelace", "Bob"]);
        app.completion = None;
        app.file_index.clear();
        app.open_org_contact_link_completion(false);
        assert_eq!(app.status, t!("status.org_contacts_no_matches").to_string());
        std::fs::remove_dir_all(&dir).ok();
    }
}
