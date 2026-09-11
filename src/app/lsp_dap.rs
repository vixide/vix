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

#![warn(clippy::pedantic)]

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent};

use super::{
    App, CompletionPopup, Focus, Prompt, PromptKind, apply_edits_to_text, char_to_lsp_pos,
    lsp_pos_to_char, severity_color,
};
use crate::editor::SEARCH_MARK;
use crate::workspace_search::{Flags as WorkspaceFlags, Hit, WorkspaceSearch};

impl App {
    /// Request inlay hints for the whole document `path` (when display is on).
    fn request_inlay_hints(&mut self, path: &Path) {
        if !self.show_inlay_hints || !self.lsp.handles(path) {
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
        if !self.show_inlay_hints {
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
                    let line_idx = line as usize;
                    if line_idx >= code.len_lines() {
                        return None;
                    }
                    let abs = lsp_pos_to_char(code, line, character, enc);
                    let col = abs.saturating_sub(code.line_to_char(line_idx));
                    Some((line_idx, col, label.clone()))
                })
                .collect()
        };
        t.editor.set_inlay_hints(converted);
    }

    /// Toggle inlay-hint display: clear them when turning off, refetch when on.
    pub(super) fn toggle_inlay_hints(&mut self) {
        self.show_inlay_hints = !self.show_inlay_hints;
        if self.show_inlay_hints {
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
                .min_by_key(|(s, e)| e - s)
        } else {
            // Largest range strictly inside the current selection.
            resolved
                .iter()
                .filter(|(s, e)| *s >= cur_s && *e <= cur_e && (*s > cur_s || *e < cur_e))
                .max_by_key(|(s, e)| e - s)
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
            .and_then(|t| t.path.clone().map(|p| (p, t.editor.cursor_line() + 1)))
        else {
            self.status = t!("status.blame_no_file").to_string();
            return;
        };
        let set = self.breakpoints.entry(path.clone()).or_default();
        if !set.remove(&line) {
            set.insert(line);
        }
        let lines: Vec<usize> = set.iter().copied().collect();
        if self.dap.is_active() {
            self.dap.set_breakpoints(&path.to_string_lossy(), &lines);
        }
        self.refresh_debug_markers();
    }

    /// Start debugging the active file with its configured adapter.
    pub(super) fn start_debugger(&mut self) {
        let Some(path) = self.editor.active_tab().and_then(|t| t.path.clone()) else {
            self.status = t!("status.debug_no_file").to_string();
            return;
        };
        let Some(adapter) = self.adapter_for(&path) else {
            self.status = t!("status.debug_no_adapter").to_string();
            return;
        };
        let bps: std::collections::HashMap<String, Vec<usize>> = self
            .breakpoints
            .iter()
            .map(|(p, l)| {
                (
                    p.to_string_lossy().into_owned(),
                    l.iter().copied().collect(),
                )
            })
            .collect();
        if self.dap.start(&adapter, &path.to_string_lossy(), bps) {
            self.show_debug_panel = true;
            self.status = t!("status.debug_started").to_string();
        } else {
            self.status = t!("status.debug_failed").to_string();
        }
    }

    /// Stop the active debug session and clear its state.
    pub(super) fn stop_debugger(&mut self) {
        self.dap.stop();
        self.dap_stopped = None;
        self.dap_stack.clear();
        self.dap_variables.clear();
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
            .map(|s| s.iter().map(|l| l.saturating_sub(1)).collect())
            .unwrap_or_default();
        let debug_line = stopped
            .filter(|(p, _)| *p == path)
            .map(|(_, l)| l.saturating_sub(1));
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_breakpoints(lines);
            t.editor.set_debug_line(debug_line);
        }
    }

    /// The breakpoint lines (1-based) set on the active file. For tests/tools.
    #[must_use]
    pub fn active_breakpoints(&self) -> Vec<usize> {
        self.editor
            .active_tab()
            .and_then(|t| t.path.as_ref())
            .and_then(|p| self.breakpoints.get(p))
            .map(|s| s.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Open `path` and move the cursor to `line` (1-based) for a debug stop.
    pub(super) fn jump_to_debug_location(&mut self, path: &str, line: usize) {
        let p = PathBuf::from(path);
        if p.is_file() {
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
        } else {
            self.lsp.did_change(&path, &text);
        }
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
        self.lsp.busy()
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
                        end = start + 1; // make a zero-width diagnostic visible
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
                let ln = (line as usize).min(code.len_lines().saturating_sub(1));
                let line_start = code.line_to_char(ln);
                let line_text = code.slice(line_start, line_start + code.line_len(ln));
                let col = crate::lsp_core::position::col_to_char(&line_text, character, enc);
                (ln + 1, col + 1)
            };
            let area = s.editor_view();
            s.editor.goto(target_line, Some(target_col), area);
        });
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
            if cur + 2 <= code.len_chars() && code.char_slice(cur, cur + 2) == "]]" {
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
            .map(|t| crate::lsp_core::CompletionItem {
                insert_text: format!("{t}{suffix}"),
                label: t,
                detail: None,
                data: None,
            })
            .collect();
        self.completion = Some(CompletionPopup { items, selected: 0 });
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
            if cur + 2 <= code.len_chars() && code.char_slice(cur, cur + 2) == "]]" {
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
                    Some(crate::lsp_core::CompletionItem {
                        label: format!("{} <{email}>", c.name),
                        insert_text: format!("{email}][{}{suffix}", c.name),
                        detail: None,
                        data: None,
                    })
                })
                .collect()
        } else {
            contacts
                .iter()
                .map(|c| crate::lsp_core::CompletionItem {
                    label: c.name.clone(),
                    insert_text: format!("{}{suffix}", c.name),
                    detail: None,
                    data: None,
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
        self.completion = Some(CompletionPopup { items, selected: 0 });
    }

    fn accept_completion(&mut self) {
        let Some(popup) = self.completion.take() else {
            return;
        };
        let Some(item) = popup.items.get(popup.selected) else {
            return;
        };
        let prefix = self.word_prefix_before_cursor();
        // If the candidate begins with what's typed, insert only the remainder so
        // the prefix is not duplicated; otherwise insert it whole.
        let insert = if !prefix.is_empty() && item.insert_text.starts_with(&prefix) {
            item.insert_text[prefix.len()..].to_string()
        } else {
            item.insert_text.clone()
        };
        let area = self.layout.editor;
        if self.editor.insert_str(&insert, area) {
            self.mark_active_dirty();
        }
    }

    /// Handle a key while the completion popup is open. Returns true if consumed.
    pub(super) fn completion_key(&mut self, key: KeyEvent) -> bool {
        if self.completion.is_none() {
            return false;
        }
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.completion.as_mut() {
                    p.selected = p.selected.saturating_sub(1);
                }
                self.resolve_selected_completion();
                true
            }
            KeyCode::Down => {
                if let Some(p) = self.completion.as_mut()
                    && p.selected + 1 < p.items.len()
                {
                    p.selected += 1;
                }
                self.resolve_selected_completion();
                true
            }
            KeyCode::Enter | KeyCode::Tab => {
                self.accept_completion();
                true
            }
            KeyCode::Esc => {
                self.completion = None;
                true
            }
            _ => {
                // Any other key dismisses the popup and is handled normally.
                self.completion = None;
                false
            }
        }
    }

    /// Lazily fetch fuller detail for the highlighted completion item via
    /// `completionItem/resolve`, when it has no detail yet but carries resolve
    /// data.
    pub(super) fn resolve_selected_completion(&mut self) {
        let Some(path) = self.active_path() else {
            return;
        };
        if !self.lsp.handles(&path) {
            return;
        }
        let Some(popup) = self.completion.as_ref() else {
            return;
        };
        let Some(item) = popup.items.get(popup.selected) else {
            return;
        };
        if item.detail.is_some() {
            return; // already has detail to show
        }
        let Some(data) = item.data.clone() else {
            return;
        };
        let label = item.label.clone();
        self.lsp
            .request_completion_resolve(&path, &label, Some(&data));
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
        if !self.lsp.handles(&path) {
            self.status = t!("status.lsp_inactive").to_string();
            return;
        }
        let tab_size = u32::try_from(self.settings.tab_width).unwrap_or(4);
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

    /// Apply LSP text edits to the active buffer (highest position first, so
    /// earlier offsets stay valid), then re-anchor the caret.
    pub(super) fn apply_lsp_edits(&mut self, edits: &[(crate::lsp_core::Range, String)]) {
        let Some(path) = self.active_path() else {
            return;
        };
        let enc = self.lsp.encoding_for(&path);
        let Some(t) = self.editor.active_tab_mut() else {
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
        // If these edits were a format-on-save, write the formatted buffer back.
        if let Some(pending) = self.format_save_pending.take()
            && self.active_path().as_deref() == Some(pending.as_path())
        {
            self.write_active_to_disk();
        }
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
            .flat_map(|(_, d)| d.iter().map(|x| x.range.start.line))
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
                .unwrap_or(lines[0])
        } else {
            lines
                .iter()
                .rev()
                .find(|&&l| l < cur)
                .copied()
                .unwrap_or(*lines.last().unwrap())
        };
        let area = self.editor_view();
        self.editor.goto(target as usize, None, area);
    }

    /// On a linked-editing response: store the ranges (as char offsets) and
    /// prompt for the shared replacement text, seeded with the current text.
    pub(super) fn begin_linked_edit(&mut self, ranges: &[crate::lsp_core::Range]) {
        let Some(path) = self.active_path() else {
            return;
        };
        let enc = self.lsp.encoding_for(&path);
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
            let seed = offsets
                .first()
                .map(|&(s, e)| code.slice(s, e))
                .unwrap_or_default();
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
                    files += 1;
                }
            } else if let Ok(text) = std::fs::read_to_string(path) {
                let new = apply_edits_to_text(&text, enc, file_edits);
                if new != text && std::fs::write(path, new).is_ok() {
                    files += 1;
                }
            }
        }
        self.refresh_git();
        self.status = t!("status.renamed_in", n = files).to_string();
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
        let mut hits: Vec<Hit> = Vec::new();
        for (path, diags) in self.lsp.all_diagnostics() {
            let rel = path
                .strip_prefix(&self.root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned();
            for d in diags {
                let sev = match d.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                    Severity::Information => "info",
                    Severity::Hint => "hint",
                };
                let line = d.range.start.line as usize + 1;
                let msg: String = d
                    .message
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(100)
                    .collect();
                hits.push(Hit {
                    path: path.clone(),
                    rel: rel.clone(),
                    line,
                    col: d.range.start.character as usize + 1,
                    display: format!("{rel}:{line}: [{sev}] {msg}"),
                    text: msg,
                });
            }
        }
        if hits.is_empty() {
            self.status = t!("status.no_diagnostics").to_string();
            return;
        }
        hits.sort_by(|a, b| a.display.cmp(&b.display));
        let mut ps = WorkspaceSearch::new(false);
        ps.flags.insert(WorkspaceFlags::STATIC_RESULTS);
        ps.status = t!("status.diagnostics_n", n = hits.len()).to_string();
        ps.hits = hits;
        self.workspace_search = Some(ps);
    }

    /// Handle a completed debugger prompt (REPL evaluate or add-watch). Grouped
    /// out of [`App::accept_prompt`] to keep it within the line limit.
    pub(super) fn accept_debug_prompt(&mut self, kind: PromptKind, expr: &str) {
        if expr.is_empty() {
            return;
        }
        match kind {
            PromptKind::DebugRepl => self.dap.evaluate(expr),
            PromptKind::DebugWatch => {
                self.dap_watches.push((expr.to_string(), String::new()));
                self.dap.evaluate(expr);
                self.show_debug_panel = true;
            }
            _ => {}
        }
    }
}
