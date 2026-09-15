//! `Settings::modal_engine`'s host wiring (`crates/vix-modal/spec/index.md`).
//! T112: Visual / Visual Line entry, exit, and cursor-extending movement.
//! T113 (this slice): the numeric-count prefix and every pure motion in
//! `vix_modal::motion`, driving Normal-mode movement. `vim_key`/
//! `spacemacs_key` call [`App::modal_key`] before falling through to
//! `vim_normal_key`'s existing table — everything this doesn't recognize
//! (`d`/`c`/`y`/`x`/`p`/…, and Visual mode's own motion vocabulary, still
//! just `h j k l` per T112) keeps working exactly as it did before the
//! engine existed. T114/T115 progressively narrow that fallback further.

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

    /// Normal-mode keys the engine itself owns: the T113 motions (with their
    /// numeric-count prefix), plus `v`/`V` entering Visual / Visual Line
    /// (T112). Everything else is `vim_normal_key`'s (T114+ moves more here).
    fn modal_normal_key(&mut self, key: KeyEvent) -> bool {
        // A pending `gg` or `f`/`t`/`F`/`T` always resolves (or silently
        // cancels, matching the old table's own `gg`/`dd`/`yy` convention)
        // on the very next key, modified or not — so this runs before the
        // ctrl/alt guard below, which only gates *fresh* dispatch.
        if self.modal_pending_g || self.modal_pending_find.is_some() {
            self.modal_resolve_pending(key);
            return true;
        }
        if Self::ctrl(&key) || Self::alt(&key) {
            return false;
        }
        // A digit keeps accumulating the count prefix -- unless it's a
        // leading '0', which `Count::push_digit` itself refuses (that's the
        // `0` motion, not the start of a count).
        if let KeyCode::Char(c) = key.code
            && self.modal_count.push_digit(c)
        {
            return true;
        }
        self.modal_normal_motion_key(key)
    }

    /// Resolve a pending `gg` (`modal_pending_g`) or `f`/`t`/`F`/`T`
    /// (`modal_pending_find`) now that its second key has arrived. Always
    /// consumes the key, matching the old table's own "a miss silently
    /// cancels" convention for `gg`/`dd`/`yy`.
    fn modal_resolve_pending(&mut self, key: KeyEvent) {
        let count = self.modal_count.value();
        self.modal_count.reset();
        let unmodified = !Self::ctrl(&key) && !Self::alt(&key);
        if self.modal_pending_g {
            self.modal_pending_g = false;
            if unmodified && key.code == KeyCode::Char('g') {
                self.modal_goto_line(Some(count));
            }
        } else if let Some(find_key) = self.modal_pending_find.take()
            && unmodified
            && let KeyCode::Char(target) = key.code
        {
            self.modal_find_char(find_key, target, count);
        }
    }

    /// The T113 motions themselves, and `v`/`V` (T112) — everything left
    /// once [`Self::modal_normal_key`] has handled pending keys, ctrl/alt,
    /// and digit accumulation. `count` is the resolved numeric prefix
    /// (`1` when none was typed).
    fn modal_normal_motion_key(&mut self, key: KeyEvent) -> bool {
        let count = self.modal_count.value();
        match key.code {
            KeyCode::Char('h') | KeyCode::Left => {
                self.apply_modal_motion(|t, p| vix_modal::motion::char_left(t, p, count));
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.apply_modal_motion(|t, p| vix_modal::motion::char_right(t, p, count));
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.apply_modal_motion(|t, p| vix_modal::motion::line_down(t, p, count));
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.apply_modal_motion(|t, p| vix_modal::motion::line_up(t, p, count));
            }
            KeyCode::Char('0') => {
                self.apply_modal_motion(vix_modal::motion::line_start);
            }
            KeyCode::Char('^') => {
                self.apply_modal_motion(vix_modal::motion::line_first_non_blank);
            }
            KeyCode::Char('$') => {
                self.apply_modal_motion(|t, p| vix_modal::motion::line_end(t, p, count));
            }
            KeyCode::Char('w') => {
                self.apply_modal_motion(|t, p| vix_modal::motion::word_forward(t, p, count));
            }
            KeyCode::Char('b') => {
                self.apply_modal_motion(|t, p| vix_modal::motion::word_backward(t, p, count));
            }
            KeyCode::Char('e') => {
                self.apply_modal_motion(|t, p| vix_modal::motion::word_end(t, p, count));
            }
            KeyCode::Char('{') => {
                self.apply_modal_motion(|t, p| vix_modal::motion::paragraph_backward(t, p, count));
            }
            KeyCode::Char('}') => {
                self.apply_modal_motion(|t, p| vix_modal::motion::paragraph_forward(t, p, count));
            }
            KeyCode::Char('(') => {
                self.apply_modal_motion(|t, p| vix_modal::motion::sentence_backward(t, p, count));
            }
            KeyCode::Char(')') => {
                self.apply_modal_motion(|t, p| vix_modal::motion::sentence_forward(t, p, count));
            }
            // `gg`: a count typed before either `g` applies to the pair as a
            // whole (`5gg` = `gg` with count 5), so it must survive past
            // this first `g` -- only the pending-resolution arm above (or an
            // unrelated key cancelling it) resets `modal_count`.
            KeyCode::Char('g') => {
                self.modal_pending_g = true;
                return true;
            }
            KeyCode::Char('G') => {
                let line = if self.modal_count.is_empty() {
                    None
                } else {
                    Some(count)
                };
                self.modal_count.reset();
                self.modal_goto_line(line);
                return true;
            }
            // `f`/`t`/`F`/`T`: same "count survives to the second key" rule
            // as `gg`.
            KeyCode::Char(c @ ('f' | 't' | 'F' | 'T')) => {
                self.modal_pending_find = Some(c);
                return true;
            }
            KeyCode::Char('v') => {
                self.modal_mode = vix_modal::Mode::Visual;
            }
            KeyCode::Char('V') => {
                self.modal_mode = vix_modal::Mode::VisualLine;
                let area = self.editor_view();
                self.editor.select_line(area);
            }
            _ => {
                self.modal_count.reset();
                return false;
            }
        }
        self.modal_count.reset();
        true
    }

    /// Run a pure motion (`fn(text, pos) -> usize`, `count` already applied
    /// by the caller) against the active tab's buffer, moving its cursor.
    /// A no-op with no active tab.
    fn apply_modal_motion(&mut self, f: impl FnOnce(&str, usize) -> usize) {
        self.apply_modal_motion_fallible(|text, pos| Some(f(text, pos)));
    }

    /// Like [`Self::apply_modal_motion`], for a motion that can fail to find
    /// a target (`f`/`t`/`F`/`T`) — the cursor is left untouched on `None`.
    fn apply_modal_motion_fallible(&mut self, f: impl FnOnce(&str, usize) -> Option<usize>) {
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let pos = tab.editor.get_cursor();
        let text = tab.editor.get_content();
        if let Some(new_pos) = f(&text, pos) {
            tab.editor.set_cursor(new_pos);
        }
    }

    /// `gg`/`G`: see [`vix_modal::motion::goto_line`] for what `line` means.
    fn modal_goto_line(&mut self, line: Option<usize>) {
        self.apply_modal_motion(|t, _pos| vix_modal::motion::goto_line(t, line));
    }

    /// Resolve a pending `f`/`t`/`F`/`T` (`find_key`) now that its target
    /// character has arrived.
    fn modal_find_char(&mut self, find_key: char, target: char, count: usize) {
        self.apply_modal_motion_fallible(|t, p| match find_key {
            'f' => vix_modal::motion::find_char_forward(t, p, target, count),
            't' => vix_modal::motion::till_char_forward(t, p, target, count),
            'F' => vix_modal::motion::find_char_backward(t, p, target, count),
            'T' => vix_modal::motion::till_char_backward(t, p, target, count),
            _ => None,
        });
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
