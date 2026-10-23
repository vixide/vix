//! Tools → Compare with File: the read-only side-by-side diff overlay between the
//! active buffer and another file. Plain file comparison, independent of the
//! `git` capability (moved out of `git.rs` by T593 for that reason).

use crossterm::event::{KeyCode, KeyEvent};

use super::{App, DiffViewState};

impl App {
    /// Open a read-only unified-diff overlay comparing the active buffer with the
    /// file at `input` (resolved relative to the workspace root).
    pub(super) fn open_diff_with(&mut self, input: &str) {
        if input.is_empty() {
            return;
        }
        let other = self.resolve(input);
        let Ok(other_text) = crate::fileops::read_to_string_guarded(&other) else {
            self.messages
                .error(t!("msg.open_failed", error = other.display()).to_string());
            return;
        };
        let Some(tab) = self.editor.active_tab() else {
            return;
        };
        let current = tab.editor.get_content();
        let here = tab.path.as_ref().and_then(|p| p.file_name()).map_or_else(
            || t!("ui.untitled").to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let there = other
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let lines = crate::diff_view::build(&other_text, &current);
        if lines.is_empty() {
            self.status = t!("status.diff_identical").to_string();
            return;
        }
        self.diff_view = Some(DiffViewState {
            title: format!("{there} ↔ {here}"),
            lines,
            scroll: 0,
        });
    }

    pub(super) fn diff_view_key(&mut self, key: KeyEvent) {
        let page = self.layout.editor.height.max(1) as usize;
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.diff_view = None,
            KeyCode::Up => {
                if let Some(d) = self.diff_view.as_mut() {
                    d.scroll = d.scroll.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if let Some(d) = self.diff_view.as_mut() {
                    d.scroll = (d.scroll + 1).min(d.lines.len().saturating_sub(1));
                }
            }
            KeyCode::PageUp => {
                if let Some(d) = self.diff_view.as_mut() {
                    d.scroll = d.scroll.saturating_sub(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(d) = self.diff_view.as_mut() {
                    d.scroll = (d.scroll + page).min(d.lines.len().saturating_sub(1));
                }
            }
            _ => {}
        }
    }
}
