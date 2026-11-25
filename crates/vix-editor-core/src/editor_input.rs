//! The widget's built-in keyboard and mouse handling: the default key map an
//! [`Editor`] uses when it is driven directly (without the host app's
//! keybinding layer), plus bracket auto-pairing and drag auto-scroll.
//!
//! [`Editor::input`] translates a [`KeyEvent`] into an
//! [`actions`](crate::actions) value and [`Editor::apply`]s it; while extra
//! carets exist ([`crate::multicursor`]) the motion/edit keys are routed to the
//! multi-caret operations instead. The match arm order is significant: the
//! first matching arm wins, so guarded arms (`if self.has_multi_carets()`,
//! `if ctrl`) sit before the unguarded fallbacks they shadow. The Vix app
//! normally resolves keys itself and calls the actions directly; this is the
//! standalone/default path. See `crates/vix-editor-core/spec/index.md`.

use crate::actions::{
    Copy, Cut, Delete, DeleteLine, Duplicate, Indent, InsertNewline, InsertText, MoveDown,
    MoveLeft, MoveRight, MoveUp, Paste, Redo, SelectAll, ToggleComment, UnIndent, Undo,
};
use crate::editor::{Editor, Flags};
use crate::multicursor::CaretMove;
use crate::selection::SelectionSnap;
use anyhow::Result;
use ratatui_core::layout::Rect;
use vix_input::{KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

/// Lines one mouse-wheel notch scrolls the editor.
pub const WHEEL_LINES: usize = 3;

/// `a + b` for screen columns; a column past the top of `u16` is past the right edge.
fn add_u16(a: u16, b: u16) -> u16 {
    add_u16_or(a, b, u16::MAX)
}

/// `a + b`, or the declared `fallback` when the sum does not fit.
fn add_u16_or(a: u16, b: u16, fallback: u16) -> u16 {
    a.checked_add(b).unwrap_or(fallback)
}

/// `a - b` for screen rows and columns; below zero is zero.
fn sub_u16(a: u16, b: u16) -> u16 {
    sub_u16_or(a, b, 0)
}

/// `a - b`, or the declared `fallback` when it would go below zero.
fn sub_u16_or(a: u16, b: u16, fallback: u16) -> u16 {
    a.checked_sub(b).unwrap_or(fallback)
}

/// `a - b`, or the declared `fallback` when it would go below zero. The
/// fallback is a parameter so each call names the declaration it comes from;
/// written inline, `checked_sub(..).unwrap_or(0)` is rewritten by Clippy to
/// `saturating_sub`, which the checked-arithmetic spec forbids.
fn sub_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_sub(b).unwrap_or(fallback)
}

/// `a + b`, or the declared `fallback` when the sum does not fit (see
/// `sub_or` for why the fallback is a parameter).
fn add_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_add(b).unwrap_or(fallback)
}

impl Editor {
    /// Handle a key event, then scroll to keep the cursor visible.
    ///
    /// # Errors
    /// Returns an error when an applied action fails.
    pub fn input(&mut self, key: KeyEvent, area: &Rect) -> Result<()> {
        use vix_input::KeyCode;

        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        // Alt is read but unused: reserved so adding an Alt chord doesn't
        // change this function's shape.
        let _alt = key.modifiers.contains(KeyModifiers::ALT);

        match key.code {
            // `÷` is what Option+/ types on a macOS keyboard; treat it as the
            // comment-toggle chord there.
            KeyCode::Char('÷') => self.apply(ToggleComment {}),
            // Shift makes terminals report an uppercase `Z`; accept both.
            KeyCode::Char('z' | 'Z') if ctrl && shift => self.apply(Redo {}),
            KeyCode::Char('z') if ctrl => self.apply(Undo {}),
            KeyCode::Char('c') if ctrl => self.apply(Copy {}),
            KeyCode::Char('v') if ctrl => self.apply(Paste {}),
            KeyCode::Char('x') if ctrl => self.apply(Cut {}),
            KeyCode::Char('k') if ctrl => self.apply(DeleteLine {}),
            // Ctrl+Shift+D duplicates the line/selection; Ctrl+D adds the next
            // occurrence of the selection/word as a caret.
            KeyCode::Char('d' | 'D') if ctrl && shift => self.apply(Duplicate {}),
            KeyCode::Char('d') if ctrl => self.add_next_occurrence(),
            KeyCode::Char('a') if ctrl => self.apply(SelectAll {}),
            // Multiple-caret routing: while extra carets exist, edit/move them all.
            KeyCode::Esc if self.has_multi_carets() => self.clear_carets(),
            KeyCode::Left if ctrl && self.has_multi_carets() => {
                self.multi_move(CaretMove::WordLeft, shift);
            }
            KeyCode::Right if ctrl && self.has_multi_carets() => {
                self.multi_move(CaretMove::WordRight, shift);
            }
            KeyCode::Left if self.has_multi_carets() => self.multi_move(CaretMove::Left, shift),
            KeyCode::Right if self.has_multi_carets() => self.multi_move(CaretMove::Right, shift),
            KeyCode::Up if self.has_multi_carets() => self.multi_move(CaretMove::Up, shift),
            KeyCode::Down if self.has_multi_carets() => self.multi_move(CaretMove::Down, shift),
            KeyCode::Home if self.has_multi_carets() => self.multi_move(CaretMove::Home, shift),
            KeyCode::End if self.has_multi_carets() => self.multi_move(CaretMove::LineEnd, shift),
            KeyCode::Backspace if self.has_multi_carets() => self.multi_delete(false),
            KeyCode::Enter if self.has_multi_carets() => self.multi_insert("\n"),
            KeyCode::Char(c) if self.has_multi_carets() && !ctrl => {
                self.multi_insert(&c.to_string());
            }
            KeyCode::Left => self.apply(MoveLeft { shift }),
            KeyCode::Right => self.apply(MoveRight { shift }),
            KeyCode::Up => self.apply(MoveUp { shift }),
            KeyCode::Down => self.apply(MoveDown { shift }),
            // Guard-with-side-effect: `auto_pair_backspace` performs the
            // paired delete itself and returns `true` to swallow the key.
            KeyCode::Backspace if self.auto_pair_backspace() => {}
            KeyCode::Backspace => self.apply(Delete {}),
            KeyCode::Enter => self.apply(InsertNewline {}),
            // Same trick: `auto_pair` inserts/steps itself when it fires.
            KeyCode::Char(c) if self.auto_pair(c) => {}
            KeyCode::Char(c) => self.apply(InsertText {
                text: c.to_string(),
            }),
            KeyCode::Tab => self.apply(Indent {}),
            KeyCode::BackTab => self.apply(UnIndent {}),
            _ => {}
        }
        self.focus(area);
        Ok(())
    }

    /// Bracket/quote auto-pairing for a typed character `c`. Returns `true` when
    /// it consumed the key (so the caller skips the plain insert):
    ///
    /// - Typing an opening paren, bracket, brace, or quote inserts the matching
    ///   closer and leaves the cursor between them; with a non-empty selection it
    ///   wraps the selection instead.
    /// - Typing a closer when the next character is that same closer just steps
    ///   over it (so you can type through the auto-inserted closer).
    /// - Quotes are not paired right next to a word character (so apostrophes in
    ///   prose/identifiers are left alone).
    fn auto_pair(&mut self, c: char) -> bool {
        // Duplicated in `auto_pair_backspace`; keep the two tables in sync.
        const PAIRS: &[(char, char)] = &[
            ('(', ')'),
            ('[', ']'),
            ('{', '}'),
            ('"', '"'),
            ('\'', '\''),
            ('`', '`'),
        ];
        if !self.flags.contains(Flags::AUTO_PAIR) {
            return false;
        }

        let cursor = self.get_cursor();
        let next = self.char_at(cursor);

        // Step over an existing closer. Checked before the opener lookup so
        // that for symmetric pairs (`"`) typing the second quote steps over
        // instead of opening a new pair.
        if PAIRS.iter().any(|&(_, cl)| cl == c) && next == Some(c) {
            self.apply(MoveRight { shift: false });
            return true;
        }

        let Some(&(_, closer)) = PAIRS.iter().find(|&&(op, _)| op == c) else {
            return false;
        };

        // Wrap a non-empty selection.
        if let Some(sel) = self.get_selection()
            && !sel.is_empty()
        {
            let text = self.get_content_slice(sel.start, sel.end);
            self.apply(InsertText {
                text: format!("{c}{text}{closer}"),
            });
            return true;
        }

        // Don't auto-pair a quote adjacent to a word character.
        if matches!(c, '"' | '\'' | '`') {
            let prev = cursor.checked_sub(1).and_then(|p| self.char_at(p));
            if prev.is_some_and(char::is_alphanumeric) || next.is_some_and(char::is_alphanumeric) {
                return false;
            }
        }

        // Two actions, but the cursor ends between the pair for the user.
        self.apply(InsertText {
            text: format!("{c}{closer}"),
        });
        self.apply(MoveLeft { shift: false });
        true
    }

    /// Backspace inside an empty auto-pair (`()`, `[]`, `{}`, `""`, …) deletes
    /// both characters as one edit. Returns `true` when it consumed the key.
    /// Only fires with a single caret and no selection.
    fn auto_pair_backspace(&mut self) -> bool {
        const PAIRS: &[(char, char)] = &[
            ('(', ')'),
            ('[', ']'),
            ('{', '}'),
            ('"', '"'),
            ('\'', '\''),
            ('`', '`'),
        ];
        if !self.flags.contains(Flags::AUTO_PAIR) {
            return false;
        }
        if self.get_selection().is_some_and(|s| !s.is_empty()) {
            return false;
        }
        let cursor = self.get_cursor();
        let Some(prev) = cursor.checked_sub(1).and_then(|p| self.char_at(p)) else {
            return false;
        };
        let next = self.char_at(cursor);
        if !PAIRS.iter().any(|&(op, cl)| op == prev && next == Some(cl)) {
            return false;
        }
        // Hand-rolled transaction (not `apply`) so both characters go in one
        // undo step and the cursor is restored precisely on undo.
        let code = self.code_mut();
        code.tx();
        code.set_state_before(cursor, None);
        // `prev` was read from the char before the cursor, so the cursor is at least 1.
        let before = sub_or(cursor, 1, 0);
        code.remove(before, add_or(cursor, 1, usize::MAX));
        let newc = before;
        code.set_state_after(newc, None);
        code.commit();
        self.set_cursor(newc);
        self.set_selection(None);
        self.reset_highlight_cache();
        true
    }

    /// Handle a mouse event (scroll, click, drag, selection).
    ///
    /// `area` is the rectangle the editor was last rendered into, used to map
    /// terminal cells to buffer offsets. A drag also records the pointer so
    /// [`Editor::autoscroll_tick`] can keep scrolling while it is held still.
    ///
    /// # Errors
    /// Returns an error when an applied action fails.
    pub fn mouse(&mut self, mouse: MouseEvent, area: &Rect) -> Result<()> {
        match mouse.kind {
            // A wheel notch moves several lines, as in other editors: one line
            // a notch made the wheel feel slow on a long file.
            MouseEventKind::ScrollUp => {
                for _ in 0..WHEEL_LINES {
                    self.scroll_up();
                }
            }
            MouseEventKind::ScrollDown => {
                for _ in 0..WHEEL_LINES {
                    self.scroll_down(usize::from(area.height));
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                self.drag_pointer = None;
                let pos = self.cursor_from_mouse(mouse.column, mouse.row, area);
                if let Some(cursor) = pos {
                    self.handle_mouse_down(cursor);
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                self.drag_pointer = Some((mouse.column, mouse.row));
                self.drag_to(mouse.column, mouse.row, *area);
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.selection_snap = SelectionSnap::None;
                self.drag_pointer = None;
            }
            _ => {}
        }
        Ok(())
    }

    /// Extend the selection to the pointer at `(column, row)` during a drag
    /// (T581). A pointer past the top or bottom edge scrolls one line and
    /// extends to the edge row, and one left of the text clamps to the first
    /// column, so the selection keeps growing instead of stalling.
    fn drag_to(&mut self, column: u16, row: u16, area: Rect) {
        // Scroll first, then extend to the (clamped) edge row, so the
        // selection tracks the freshly scrolled-in line.
        let top = area.top();
        let last = sub_u16(area.bottom(), 1);
        if row <= top {
            self.scroll_up();
        }
        if row >= last {
            self.scroll_down(usize::from(area.height));
        }
        let gutter = u16::try_from(self.get_line_number_width()).unwrap_or(u16::MAX);
        // `max` then `min`, not `clamp`: a view narrower than its gutter (or no
        // rows at all) has `min > max`, which `clamp` asserts against.
        let col = column
            .max(add_u16(area.left(), gutter))
            .min(sub_u16(area.right(), 1));
        if let Some(cursor) = self.cursor_from_mouse(col, row.max(top).min(last), &area) {
            self.handle_mouse_drag(cursor);
        }
    }

    /// Repeat the last drag while the pointer is held still outside the top
    /// or bottom edge (T581). Call periodically; a no-op when no drag is in
    /// progress or the pointer is inside the view. Returns whether it acted.
    pub fn autoscroll_tick(&mut self, area: &Rect) -> bool {
        let Some((column, row)) = self.drag_pointer else {
            return false;
        };
        // Strictly inside the view: nothing to do. The edge rows themselves
        // count as "outside" to match `drag_to`'s `<= top` / `>= last`.
        if row > area.top() && row < sub_u16(area.bottom(), 1) {
            return false;
        }
        self.drag_to(column, row, *area);
        true
    }
}

#[cfg(test)]
pub(crate) mod tests_support {
    use std::sync::Mutex;
    /// Serializes tests that use the process-global clipboard.
    pub static CLIP: Mutex<()> = Mutex::new(());
}

#[cfg(test)]
mod tests {
    use super::*;
    use vix_input::KeyModifiers;

    fn ev(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn doc() -> Editor {
        let text = (0..200).fold(String::new(), |a, i| a + &format!("line {i}\n"));
        Editor::new("text", &text, Vec::new()).unwrap()
    }

    /// T581: dragging past the bottom/top edge keeps scrolling and keeps
    /// extending the selection, and a held pointer keeps going on ticks.
    #[test]
    fn drag_past_the_edges_autoscrolls_and_extends_the_selection() {
        let area = Rect::new(0, 5, 40, 10);
        let mut e = doc();
        e.set_offset_y(50);
        e.mouse(ev(MouseEventKind::Down(MouseButton::Left), 8, 8), &area)
            .unwrap();
        let anchor_line = e.code_ref().char_to_line(e.get_cursor());

        // Pointer far below the view (e.g. over the status bar).
        e.mouse(ev(MouseEventKind::Drag(MouseButton::Left), 8, 30), &area)
            .unwrap();
        assert_eq!(e.offset_y, 51, "scrolled down");
        let sel = e.get_selection().expect("selection extended");
        let end_line = e.code_ref().char_to_line(sel.end);
        assert!(
            end_line > anchor_line + 5,
            "extended to the last visible row"
        );

        // Held still: ticks keep scrolling and extending.
        assert!(e.autoscroll_tick(&area));
        assert!(e.autoscroll_tick(&area));
        assert_eq!(e.offset_y, 53);
        let sel2 = e.get_selection().unwrap();
        let end2 = e.code_ref().char_to_line(sel2.end);
        assert!(end2 > end_line);

        // Pointer above the view scrolls back up.
        e.mouse(ev(MouseEventKind::Drag(MouseButton::Left), 8, 0), &area)
            .unwrap();
        assert_eq!(e.offset_y, 52);

        // Inside the view: no tick action. After release: none either.
        e.mouse(ev(MouseEventKind::Drag(MouseButton::Left), 8, 8), &area)
            .unwrap();
        assert!(!e.autoscroll_tick(&area));
        e.mouse(ev(MouseEventKind::Drag(MouseButton::Left), 8, 30), &area)
            .unwrap();
        e.mouse(ev(MouseEventKind::Up(MouseButton::Left), 8, 30), &area)
            .unwrap();
        assert!(!e.autoscroll_tick(&area));
    }

    // ----- default key map ------------------------------------------------

    use vix_input::KeyCode;

    fn key(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    fn area() -> Rect {
        Rect::new(0, 0, 40, 10)
    }

    fn ed(text: &str, cursor: usize) -> Editor {
        let mut e = Editor::new("rust", text, Vec::new()).unwrap();
        e.set_cursor(cursor);
        e
    }

    fn press(e: &mut Editor, code: KeyCode, mods: KeyModifiers) {
        e.input(key(code, mods), &area()).unwrap();
    }

    fn typed(e: &mut Editor, s: &str) {
        for c in s.chars() {
            press(e, KeyCode::Char(c), KeyModifiers::NONE);
        }
    }

    fn content(e: &Editor) -> String {
        e.code_ref().get_content()
    }

    #[test]
    fn plain_keys_move_and_edit() {
        let mut e = ed("ab\ncd", 0);
        press(&mut e, KeyCode::Right, KeyModifiers::NONE);
        assert_eq!(e.get_cursor(), 1);
        press(&mut e, KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(e.get_cursor(), 4);
        press(&mut e, KeyCode::Left, KeyModifiers::NONE);
        assert_eq!(e.get_cursor(), 3);
        press(&mut e, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(e.get_cursor(), 0);
        press(&mut e, KeyCode::Right, KeyModifiers::SHIFT);
        assert_eq!(e.get_selection().unwrap().sorted(), (0, 1));
        press(&mut e, KeyCode::Char('X'), KeyModifiers::SHIFT);
        assert_eq!(content(&e), "Xb\ncd");
        press(&mut e, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(content(&e), "X\nb\ncd");
        press(&mut e, KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(content(&e), "Xb\ncd");
        press(&mut e, KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(content(&e), "    Xb\ncd");
        press(&mut e, KeyCode::BackTab, KeyModifiers::NONE);
        assert_eq!(content(&e), "Xb\ncd");
        // Unbound keys are ignored.
        press(&mut e, KeyCode::F(5), KeyModifiers::NONE);
        assert_eq!(content(&e), "Xb\ncd");
    }

    #[test]
    fn ctrl_chords_undo_redo_select_delete_duplicate() {
        let mut e = ed("ab\ncd", 1);
        typed(&mut e, "x");
        assert_eq!(content(&e), "axb\ncd");
        press(&mut e, KeyCode::Char('z'), KeyModifiers::CONTROL);
        assert_eq!(content(&e), "ab\ncd");
        press(
            &mut e,
            KeyCode::Char('Z'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        );
        assert_eq!(content(&e), "axb\ncd");
        press(
            &mut e,
            KeyCode::Char('d'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        );
        assert_eq!(content(&e), "axb\naxb\ncd");
        press(&mut e, KeyCode::Char('k'), KeyModifiers::CONTROL);
        assert_eq!(content(&e), "axb\ncd");
        press(&mut e, KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert_eq!(e.get_selection().unwrap().sorted(), (0, 6));
        press(&mut e, KeyCode::Char('÷'), KeyModifiers::NONE);
        assert!(content(&e).starts_with("//axb"));
    }

    #[test]
    fn ctrl_clipboard_chords() {
        let _g = crate::editor_input::tests_support::CLIP
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut e = ed("hello", 0);
        e.set_selection_range(0, 5);
        press(&mut e, KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(content(&e), "hello");
        press(&mut e, KeyCode::Char('x'), KeyModifiers::CONTROL);
        assert_eq!(content(&e), "");
        press(&mut e, KeyCode::Char('v'), KeyModifiers::CONTROL);
        assert_eq!(content(&e), "hello");
    }

    #[test]
    fn ctrl_d_adds_the_next_occurrence_and_multi_caret_keys_edit_all() {
        let mut e = ed("foo foo foo", 0);
        press(&mut e, KeyCode::Char('d'), KeyModifiers::CONTROL);
        press(&mut e, KeyCode::Char('d'), KeyModifiers::CONTROL);
        assert!(e.has_multi_carets());
        // Typing replaces every selected occurrence.
        typed(&mut e, "b");
        assert!(content(&e).starts_with("b b"));
        press(&mut e, KeyCode::Enter, KeyModifiers::NONE);
        assert!(content(&e).contains('\n'));
        press(&mut e, KeyCode::Backspace, KeyModifiers::NONE);
        // Esc collapses back to one caret.
        press(&mut e, KeyCode::Esc, KeyModifiers::NONE);
        assert!(!e.has_multi_carets());
    }

    #[test]
    fn multi_caret_motion_keys_move_every_caret() {
        for (code, mods) in [
            (KeyCode::Left, KeyModifiers::NONE),
            (KeyCode::Right, KeyModifiers::NONE),
            (KeyCode::Up, KeyModifiers::NONE),
            (KeyCode::Down, KeyModifiers::NONE),
            (KeyCode::Home, KeyModifiers::NONE),
            (KeyCode::End, KeyModifiers::NONE),
            (KeyCode::Left, KeyModifiers::CONTROL),
            (KeyCode::Right, KeyModifiers::CONTROL),
            (KeyCode::Right, KeyModifiers::SHIFT),
        ] {
            let mut e = ed("ab cd\nef gh\nij kl\nmn op", 3);
            e.add_caret_at(15);
            assert!(e.has_multi_carets());
            let before = e.caret_positions();
            press(&mut e, code, mods);
            assert!(e.has_multi_carets(), "{code:?} kept both carets");
            let after = e.caret_positions();
            assert_ne!(before, after, "{code:?}/{mods:?} moved the carets");
        }
    }

    #[test]
    fn auto_pairing_inserts_steps_over_wraps_and_deletes_pairs() {
        let mut e = ed("", 0);
        e.set_auto_pair(true);
        typed(&mut e, "(");
        assert_eq!((content(&e).as_str(), e.get_cursor()), ("()", 1));
        typed(&mut e, ")");
        assert_eq!((content(&e).as_str(), e.get_cursor()), ("()", 2));
        // Backspace inside an empty pair removes both, as one undo step.
        e.set_cursor(1);
        press(&mut e, KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!((content(&e).as_str(), e.get_cursor()), ("", 0));
        press(&mut e, KeyCode::Char('z'), KeyModifiers::CONTROL);
        assert_eq!(content(&e), "()");
        // Quotes pair at a boundary but not beside a word character.
        let mut e = ed("", 0);
        e.set_auto_pair(true);
        typed(&mut e, "\"");
        assert_eq!(content(&e), "\"\"");
        let mut e = ed("a", 1);
        e.set_auto_pair(true);
        typed(&mut e, "'");
        assert_eq!(content(&e), "a'");
        let mut e = ed("a", 0);
        e.set_auto_pair(true);
        typed(&mut e, "'");
        assert_eq!(content(&e), "'a");
        // A selection is wrapped.
        let mut e = ed("word", 0);
        e.set_auto_pair(true);
        e.set_selection_range(0, 4);
        typed(&mut e, "[");
        assert_eq!(content(&e), "[word]");
        // A non-pair character is not consumed; Backspace beside text is plain.
        let mut e = ed("ab", 1);
        e.set_auto_pair(true);
        press(&mut e, KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(content(&e), "b");
        // A selection makes Backspace delete the selection, not a pair.
        let mut e = ed("()", 1);
        e.set_auto_pair(true);
        e.set_selection_range(0, 1);
        press(&mut e, KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(content(&e), ")");
        // With auto-pair off an opener is inserted alone.
        let mut e = ed("", 0);
        e.set_auto_pair(false);
        typed(&mut e, "(");
        assert_eq!(content(&e), "(");
    }

    #[test]
    fn wheel_scrolls_three_lines_per_notch() {
        let a = Rect::new(0, 0, 40, 10);
        let mut e = doc();
        e.mouse(ev(MouseEventKind::ScrollDown, 1, 1), &a).unwrap();
        assert_eq!(e.offset_y, WHEEL_LINES);
        e.mouse(ev(MouseEventKind::ScrollUp, 1, 1), &a).unwrap();
        assert_eq!(e.offset_y, 0);
        // Other mouse events are ignored.
        e.mouse(ev(MouseEventKind::Moved, 1, 1), &a).unwrap();
        assert_eq!(e.offset_y, 0);
    }

    #[test]
    fn click_places_the_cursor() {
        let a = Rect::new(0, 0, 40, 10);
        let mut e = doc();
        let gutter = u16::try_from(e.get_line_number_width()).unwrap();
        e.mouse(
            ev(MouseEventKind::Down(MouseButton::Left), gutter + 2, 3),
            &a,
        )
        .unwrap();
        assert_eq!(e.get_cursor(), e.code_ref().line_to_char(3) + 2);
    }
}
