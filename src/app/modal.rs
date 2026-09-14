//! `Settings::modal_engine`'s host wiring (`crates/vix-modal/spec/index.md`).
//! T112: Visual / Visual Line entry, exit, and cursor-extending movement.
//! `vim_key`/`spacemacs_key` call [`App::modal_key`] before falling through to
//! `vim_normal_key`'s existing table — everything this doesn't recognize
//! keeps working exactly as it did before the engine existed (T113+
//! progressively narrow that fallback as real motions/operators land).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::App;

impl App {
    /// The modal engine's own key handling, tried before `vim_normal_key`'s
    /// table. Returns `true` if `key` was consumed. A cheap no-op whenever
    /// `Settings::modal_engine` is off (the default) or the mode is
    /// [`vix_modal::Mode::Insert`] (that path is still `modal_insert`,
    /// unchanged, checked by the caller before this ever runs).
    pub(super) fn modal_key(&mut self, key: KeyEvent) -> bool {
        if !self.settings.modal_engine {
            return false;
        }
        match self.modal_mode {
            vix_modal::Mode::Normal => self.modal_normal_key(key),
            vix_modal::Mode::Visual | vix_modal::Mode::VisualLine => self.modal_visual_key(key),
            vix_modal::Mode::Insert => false,
        }
    }

    /// Normal-mode keys the engine itself owns: entering Visual / Visual
    /// Line. Everything else is `vim_normal_key`'s (T113+ moves more here).
    fn modal_normal_key(&mut self, key: KeyEvent) -> bool {
        if Self::ctrl(&key) || Self::alt(&key) {
            return false;
        }
        match key.code {
            KeyCode::Char('v') => {
                self.modal_mode = vix_modal::Mode::Visual;
                true
            }
            KeyCode::Char('V') => {
                self.modal_mode = vix_modal::Mode::VisualLine;
                let area = self.editor_view();
                self.editor.select_line(area);
                true
            }
            _ => false,
        }
    }

    /// Visual / Visual Line mode keys: `Esc` returns to Normal (collapsing
    /// the selection to the cursor); `h j k l` and the arrow keys extend it.
    /// Anything else falls through unconsumed — T114 is where operators
    /// learn to act on a Visual selection instead.
    fn modal_visual_key(&mut self, key: KeyEvent) -> bool {
        if key.code == KeyCode::Esc {
            self.modal_mode = vix_modal::Mode::Normal;
            if let Some(tab) = self.editor.active_tab_mut() {
                tab.editor.clear_selection();
            }
            return true;
        }
        if Self::ctrl(&key) || Self::alt(&key) {
            return false;
        }
        let step = match key.code {
            KeyCode::Char('h') | KeyCode::Left => Some(KeyCode::Left),
            KeyCode::Char('j') | KeyCode::Down => Some(KeyCode::Down),
            KeyCode::Char('k') | KeyCode::Up => Some(KeyCode::Up),
            KeyCode::Char('l') | KeyCode::Right => Some(KeyCode::Right),
            _ => None,
        };
        let Some(step) = step else {
            return false;
        };
        // The real shift-extend mechanism: the editor's own selection model
        // already tracks a stable anchor across repeated extends/shrinks
        // (`Ctrl+Shift+Right`/`Left` already relies on the same plumbing —
        // `spec/index.md`'s audit confirms it's end-to-end already), so
        // there's no need for the modal engine to track its own anchor.
        self.editor_key(KeyEvent::new(step, KeyModifiers::SHIFT));
        if self.modal_mode == vix_modal::Mode::VisualLine {
            self.snap_visual_selection_to_whole_lines();
        }
        true
    }

    /// Re-snap the active tab's selection to whole lines, keeping the
    /// cursor on whichever end the selection is currently growing toward.
    /// `Editor::select_line` can't do this itself — it always selects just
    /// the cursor's current line fresh, with no memory of how far a
    /// multi-line Visual Line selection has already grown.
    fn snap_visual_selection_to_whole_lines(&mut self) {
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let cursor = tab.editor.get_cursor();
        let Some(sel) = tab.editor.get_selection() else {
            return;
        };
        let code = tab.editor.code_ref();
        let start_row = code.char_to_line(sel.start);
        // `sel.end` is exclusive, so back it up one character before asking
        // which line it's on -- otherwise a selection ending right at a
        // line break would misreport itself as reaching into the next line.
        let end_row = code.char_to_line(sel.end.saturating_sub(1).max(sel.start));
        let line_start = code.line_to_char(start_row);
        let line_end = if end_row + 1 < code.len_lines() {
            code.line_to_char(end_row + 1)
        } else {
            line_start
                + (start_row..=end_row)
                    .map(|r| code.line_len(r))
                    .sum::<usize>()
        };
        // Keep the cursor on the same end the native shift-motion just left
        // it on, so growing the selection downward keeps the caret visually
        // at the bottom (and upward, at the top) instead of snapping it to
        // a fixed end regardless of direction.
        if code.char_to_line(cursor) <= start_row {
            tab.editor.set_selection_range(line_end, line_start);
        } else {
            tab.editor.set_selection_range(line_start, line_end);
        }
    }
}
