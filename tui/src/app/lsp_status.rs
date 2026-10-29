//! View → LSP Status… (T732): a live overlay listing every configured
//! language server — name, command, root, state, open files, in-flight
//! `$/progress` tasks, advertised capabilities and last error — with a
//! per-server restart, plus the status-bar progress segment.
//!
//! The rows are not snapshotted: the renderer reads
//! [`crate::lsp::Lsp::statuses`] every frame, so the panel follows a server
//! from Starting to Running (or Failed) while it is open. The panel itself
//! only remembers which server is highlighted.

use vix_input::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};

use super::{App, rect_contains};

/// The LSP Status overlay's state: the highlighted server (an index into
/// [`crate::lsp::Lsp::statuses`]) and the first visible line.
#[derive(Clone, Debug, Default)]
pub(crate) struct LspStatusPanel {
    /// Index of the highlighted server.
    pub(crate) selected: usize,
    /// First visible line of the rendered list, kept by the renderer.
    pub(crate) scroll: usize,
    /// Each server's first line in the rendered list, as last drawn, so a
    /// click can be mapped back to a server.
    pub(crate) starts: Vec<usize>,
}

/// Longest progress text the status bar shows before eliding.
const PROGRESS_SEGMENT_CHARS: usize = 48;

impl App {
    /// Open View → LSP Status… (`lsp.status`).
    pub(super) fn open_lsp_status(&mut self) {
        self.lsp_status = Some(LspStatusPanel::default());
    }

    /// Keys while the LSP Status panel is open: Up/Down pick a server, `r`
    /// or Enter restarts it, Esc or `q` closes.
    pub(super) fn lsp_status_key(&mut self, key: KeyEvent) {
        let count = self.lsp.statuses().len();
        let Some(panel) = self.lsp_status.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Up => panel.selected = panel.selected.saturating_sub(1),
            KeyCode::Down => {
                if panel.selected + 1 < count {
                    panel.selected += 1;
                }
            }
            KeyCode::Home => panel.selected = 0,
            KeyCode::End => panel.selected = count.saturating_sub(1),
            KeyCode::Enter | KeyCode::Char('r') => self.lsp_status_restart_selected(),
            KeyCode::Esc | KeyCode::Char('q') => self.lsp_status = None,
            _ => {}
        }
    }

    /// A left click on a server's block in the LSP Status panel highlights
    /// it; any other mouse event is swallowed while the panel is open.
    pub(super) fn lsp_status_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.lsp_status;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row = usize::from(mouse.row - r.y);
        let Some(panel) = self.lsp_status.as_mut() else {
            return;
        };
        let line = panel.scroll + row;
        if let Some(idx) = panel.starts.iter().rposition(|&s| s <= line) {
            panel.selected = idx;
        }
    }

    /// Restart the highlighted server in the LSP Status panel.
    fn lsp_status_restart_selected(&mut self) {
        let Some(selected) = self.lsp_status.as_ref().map(|p| p.selected) else {
            return;
        };
        let Some(status) = self.lsp.statuses().into_iter().nth(selected) else {
            return;
        };
        let reopened = self.lsp.restart_language(&status.language_id);
        self.after_lsp_restart(reopened);
    }

    /// The status bar's language-server progress segment (T732): the
    /// newest in-flight `$/progress` task, elided to a fixed width and
    /// followed by a gap, or empty when nothing is in progress.
    #[must_use]
    pub(crate) fn lsp_progress_segment(&self) -> String {
        let Some(line) = self.lsp.progress_line() else {
            return String::new();
        };
        let mut text: String = line.chars().take(PROGRESS_SEGMENT_CHARS).collect();
        if line.chars().count() > PROGRESS_SEGMENT_CHARS {
            text.push('…');
        }
        format!("{text}   ")
    }
}
