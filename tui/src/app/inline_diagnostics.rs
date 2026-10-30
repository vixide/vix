//! Inline diagnostics and the diagnostic at the cursor (T614).
//!
//! Each tick maps the active file's diagnostics onto the editor's
//! end-of-line virtual-text layer (per the `inline_diagnostics` setting) and,
//! when the cursor reaches a line with a diagnostic, shows its message in the
//! status line. The selection rules are pure, in [`crate::lsp::inline`].

use std::path::PathBuf;

use crate::editor_core::virtual_text::{EolText, VirtualTextLayer};
use crate::lsp::inline::{self, InlineMode};
use crate::lsp_core::{Diagnostic, Severity};

use super::App;

/// What the status line last showed for the cursor's diagnostic: the file, the
/// zero-based cursor line, and the shown text (empty when that line had none).
pub(super) type DiagStatus = Option<(PathBuf, usize, String)>;

/// The status-line text for a diagnostic: its translated severity label and the
/// first line of its message.
fn status_text(d: &Diagnostic) -> String {
    let message = inline::first_line(&d.message);
    match d.severity {
        Severity::Error => t!("status.diagnostic_error", message = message),
        Severity::Warning => t!("status.diagnostic_warning", message = message),
        Severity::Information => t!("status.diagnostic_info", message = message),
        Severity::Hint => t!("status.diagnostic_hint", message = message),
    }
    .to_string()
}

impl App {
    /// The active file's path and diagnostics, when a language server handles
    /// it, plus the zero-based cursor line.
    fn active_diagnostics(&self) -> Option<(PathBuf, Vec<Diagnostic>, usize)> {
        let path = self.active_path()?;
        if !self.lsp.handles(&path) {
            return None;
        }
        let diags = self.lsp.diagnostics_for(&path);
        Some((path, diags, self.editor.cursor_line()))
    }

    /// Redraw the active editor's inline diagnostics for the cursor's current
    /// line and the `inline_diagnostics` setting, and follow the cursor with the
    /// status-line message. Cheap; called once per tick from `poll_lsp`.
    pub(super) fn refresh_inline_diagnostics(&mut self) {
        let Some((path, diags, cursor)) = self.active_diagnostics() else {
            if let Some(t) = self.editor.active_tab_mut() {
                t.editor.clear_eol_texts(VirtualTextLayer::Diagnostics);
            }
            return;
        };
        let mode = InlineMode::parse(&self.settings.inline_diagnostics);
        let texts: Vec<EolText> =
            inline::inline_entries(&diags, mode, u32::try_from(cursor).unwrap_or(u32::MAX))
                .into_iter()
                .map(|e| {
                    EolText::new(e.line as usize, e.message)
                        .with_color(super::severity_color(e.severity))
                })
                .collect();
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_eol_texts(VirtualTextLayer::Diagnostics, texts);
        }
        self.follow_cursor_diagnostic(path, &diags, cursor, false);
    }

    /// Show the diagnostic on the cursor's line in the status line, even when
    /// the cursor was already there (Go → Next/Previous Issue calls this after
    /// moving).
    pub(super) fn show_cursor_diagnostic(&mut self) {
        if let Some((path, diags, cursor)) = self.active_diagnostics() {
            self.follow_cursor_diagnostic(path, &diags, cursor, true);
        }
    }

    /// Update the status line when the cursor's line (or its diagnostic)
    /// changed since last time, or always when `force`: show the line's
    /// diagnostic, or clear the previous one if the status still shows it.
    fn follow_cursor_diagnostic(
        &mut self,
        path: PathBuf,
        diags: &[Diagnostic],
        cursor: usize,
        force: bool,
    ) {
        let line = u32::try_from(cursor).unwrap_or(u32::MAX);
        let text = inline::diagnostic_on_line(diags, line)
            .map(status_text)
            .unwrap_or_default();
        let unchanged = self
            .diag_status
            .as_ref()
            .is_some_and(|(p, l, s)| *p == path && *l == cursor && *s == text);
        if unchanged && !force {
            return;
        }
        let previous = self.diag_status.take().map(|(_, _, s)| s);
        if !text.is_empty() {
            self.status.clone_from(&text);
        } else if previous.is_some_and(|s| !s.is_empty() && s == self.status) {
            self.status.clear();
        }
        self.diag_status = Some((path, cursor, text));
    }
}
