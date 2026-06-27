#![warn(clippy::pedantic)]
use crate::editor_core::actions::{ToggleComment, Redo, Undo, Copy, Paste, Cut, DeleteLine, Duplicate, SelectAll, MoveLeft, MoveRight, MoveUp, MoveDown, Delete, InsertNewline, InsertText, Indent, UnIndent};
use crate::editor_core::editor::Editor;
use crate::editor_core::multicursor::CaretMove;
use crate::editor_core::selection::SelectionSnap;
use anyhow::Result;
use crossterm::event::{
    KeyEvent, KeyModifiers,
    MouseButton, MouseEvent, MouseEventKind,
};
use ratatui_core::layout::Rect;

impl Editor {
    /// Handle a crossterm key event, then scroll to keep the cursor visible.
    ///
    /// # Errors
    /// Returns an error when an applied action fails.
    pub fn input(
        &mut self, key: KeyEvent, area: &Rect,
    ) -> Result<()> {
        use crossterm::event::KeyCode;

        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let _alt = key.modifiers.contains(KeyModifiers::ALT);

        match key.code {
            KeyCode::Char('÷') => self.apply(ToggleComment { }),
            KeyCode::Char('z' | 'Z') if ctrl && shift => self.apply(Redo { }),
            KeyCode::Char('z') if ctrl => self.apply(Undo { }),
            KeyCode::Char('c') if ctrl => self.apply(Copy { }),
            KeyCode::Char('v') if ctrl => self.apply(Paste { }),
            KeyCode::Char('x') if ctrl => self.apply(Cut { }),
            KeyCode::Char('k') if ctrl => self.apply(DeleteLine { }),
            // Ctrl+Shift+D duplicates the line/selection; Ctrl+D adds the next
            // occurrence of the selection/word as a caret.
            KeyCode::Char('d' | 'D') if ctrl && shift => self.apply(Duplicate { }),
            KeyCode::Char('d') if ctrl => self.add_next_occurrence(),
            KeyCode::Char('a') if ctrl => self.apply(SelectAll { }),
            // Multiple-caret routing: while extra carets exist, edit/move them all.
            KeyCode::Esc       if self.has_multi_carets() => self.clear_carets(),
            KeyCode::Left      if self.has_multi_carets() => self.multi_move(CaretMove::Left, shift),
            KeyCode::Right     if self.has_multi_carets() => self.multi_move(CaretMove::Right, shift),
            KeyCode::Up        if self.has_multi_carets() => self.multi_move(CaretMove::Up, shift),
            KeyCode::Down      if self.has_multi_carets() => self.multi_move(CaretMove::Down, shift),
            KeyCode::Backspace if self.has_multi_carets() => self.multi_delete(false),
            KeyCode::Enter     if self.has_multi_carets() => self.multi_insert("\n"),
            KeyCode::Char(c)   if self.has_multi_carets() && !ctrl => self.multi_insert(&c.to_string()),
            KeyCode::Left      => self.apply(MoveLeft { shift }),
            KeyCode::Right     => self.apply(MoveRight { shift }),
            KeyCode::Up        => self.apply(MoveUp { shift }),
            KeyCode::Down      => self.apply(MoveDown { shift }),
            KeyCode::Backspace if self.auto_pair_backspace() => {}
            KeyCode::Backspace => self.apply(Delete { }),
            KeyCode::Enter     => self.apply(InsertNewline { }),
            KeyCode::Char(c) if self.auto_pair(c) => {}
            KeyCode::Char(c)   => self.apply(InsertText { text: c.to_string() }),
            KeyCode::Tab       => self.apply(Indent { }),
            KeyCode::BackTab   => self.apply(UnIndent { }),
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
        const PAIRS: &[(char, char)] =
            &[('(', ')'), ('[', ']'), ('{', '}'), ('"', '"'), ('\'', '\''), ('`', '`')];
        if !self.auto_pair {
            return false;
        }

        let cursor = self.get_cursor();
        let next = self.char_at(cursor);

        // Step over an existing closer.
        if PAIRS.iter().any(|&(_, cl)| cl == c) && next == Some(c) {
            self.apply(MoveRight { shift: false });
            return true;
        }

        let Some(&(_, closer)) = PAIRS.iter().find(|&&(op, _)| op == c) else {
            return false;
        };

        // Wrap a non-empty selection.
        if let Some(sel) = self.get_selection()
            && !sel.is_empty() {
                let text = self.get_content_slice(sel.start, sel.end);
                self.apply(InsertText { text: format!("{c}{text}{closer}") });
                return true;
            }

        // Don't auto-pair a quote adjacent to a word character.
        if matches!(c, '"' | '\'' | '`') {
            let prev = cursor.checked_sub(1).and_then(|p| self.char_at(p));
            if prev.is_some_and(char::is_alphanumeric) || next.is_some_and(char::is_alphanumeric) {
                return false;
            }
        }

        self.apply(InsertText { text: format!("{c}{closer}") });
        self.apply(MoveLeft { shift: false });
        true
    }

    /// Backspace inside an empty auto-pair (`()`, `[]`, `{}`, `""`, …) deletes
    /// both characters as one edit. Returns `true` when it consumed the key.
    /// Only fires with a single caret and no selection.
    fn auto_pair_backspace(&mut self) -> bool {
        const PAIRS: &[(char, char)] =
            &[('(', ')'), ('[', ']'), ('{', '}'), ('"', '"'), ('\'', '\''), ('`', '`')];
        if !self.auto_pair {
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
        let code = self.code_mut();
        code.tx();
        code.set_state_before(cursor, None);
        code.remove(cursor - 1, cursor + 1);
        let newc = cursor - 1;
        code.set_state_after(newc, None);
        code.commit();
        self.set_cursor(newc);
        self.set_selection(None);
        self.reset_highlight_cache();
        true
    }

    /// Handle a crossterm mouse event (scroll, click, drag, selection).
    ///
    /// # Errors
    /// Returns an error when an applied action fails.
    pub fn mouse(
        &mut self, mouse: MouseEvent, area: &Rect,
    ) -> Result<()> {

        match mouse.kind {
            MouseEventKind::ScrollUp => self.scroll_up(),
            MouseEventKind::ScrollDown => self.scroll_down(area.height as usize),
            MouseEventKind::Down(MouseButton::Left) => {
                let pos = self.cursor_from_mouse(mouse.column, mouse.row, area);
                if let Some(cursor) = pos {
                    self.handle_mouse_down(cursor);
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // Auto-scroll when dragging on the last or first visible row
                if mouse.row == area.top() {
                    self.scroll_up();
                }
                if mouse.row == area.bottom().saturating_sub(1) {
                    self.scroll_down(area.height as usize);
                }
                let pos = self.cursor_from_mouse(mouse.column, mouse.row, area);
                if let Some(cursor) = pos {
                    self.handle_mouse_drag(cursor);
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.selection_snap = SelectionSnap::None;
            }
            _ => {}
        }
        Ok(())
    }
}