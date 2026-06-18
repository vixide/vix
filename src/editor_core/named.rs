//! Named editor actions (the `spec/actions/actions.tsv` catalog), as `snake_case`
//! methods on [`Editor`]. These wrap the lower-level `Action` structs and rope
//! helpers so the host can dispatch them by name (`run_action`) and bind them to
//! keys. Methods that need the viewport height take it as `view_h`.
//!
//! A handful of catalog actions are genuinely host- or mode-level (tabs, splits,
//! files, macros, shell/command/overwrite modes, suspend); those are dispatched
//! by the host, not here.

#![warn(clippy::pedantic)]

#![allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]

use crate::editor_core::actions::{
    Copy, Cut, Delete, DeleteLine, Duplicate, Indent, InsertNewline, InsertText, MoveDown,
    MoveLeft, MoveRight, MoveUp, Paste, Redo, SelectAll, UnIndent, Undo,
};
use crate::editor_core::editor::Editor;
use crate::editor_core::selection::Selection;

impl Editor {
    // ----- cursor movement ------------------------------------------------
    /// Move the cursor up one line.
    pub fn cursor_up(&mut self) { self.apply(MoveUp { shift: false }); }
    /// Move the cursor down one line.
    pub fn cursor_down(&mut self) { self.apply(MoveDown { shift: false }); }
    /// Move the cursor one character to the left.
    pub fn cursor_left(&mut self) { self.apply(MoveLeft { shift: false }); }
    /// Move the cursor one character to the right.
    pub fn cursor_right(&mut self) { self.apply(MoveRight { shift: false }); }
    /// Extend the selection one line up.
    pub fn select_up(&mut self) { self.apply(MoveUp { shift: true }); }
    /// Extend the selection one line down.
    pub fn select_down(&mut self) { self.apply(MoveDown { shift: true }); }
    /// Extend the selection one character to the left.
    pub fn select_left(&mut self) { self.apply(MoveLeft { shift: true }); }
    /// Extend the selection one character to the right.
    pub fn select_right(&mut self) { self.apply(MoveRight { shift: true }); }

    /// Move the cursor to the very start of the buffer.
    pub fn cursor_start(&mut self) {
        self.clear_selection();
        self.set_cursor(0);
    }
    /// Move the cursor to the very end of the buffer.
    pub fn cursor_end(&mut self) {
        self.clear_selection();
        self.set_cursor(self.code_ref().len_chars());
    }
    /// Extend the selection to the start of the buffer.
    pub fn select_to_start(&mut self) {
        self.extend_selection(0);
        self.set_cursor(0);
    }
    /// Extend the selection to the end of the buffer.
    pub fn select_to_end(&mut self) {
        let end = self.code_ref().len_chars();
        self.extend_selection(end);
        self.set_cursor(end);
    }

    // ----- line motions ---------------------------------------------------
    fn line_start(&self, pos: usize) -> usize {
        self.code_ref().line_to_char(self.code_ref().char_to_line(pos))
    }
    fn line_end(&self, pos: usize) -> usize {
        let code = self.code_ref();
        let row = code.char_to_line(pos);
        code.line_to_char(row) + code.line_len(row)
    }
    /// First non-whitespace column of the cursor's line.
    fn line_first_text(&self, pos: usize) -> usize {
        let code = self.code_ref();
        let row = code.char_to_line(pos);
        let start = code.line_to_char(row);
        let len = code.line_len(row);
        let line = code.slice(start, start + len);
        let lead = line.chars().take_while(|c| *c == ' ' || *c == '\t').count();
        start + lead
    }

    /// Move the cursor to the start of the current line.
    pub fn start_of_line(&mut self) {
        self.clear_selection();
        let p = self.line_start(self.get_cursor());
        self.set_cursor(p);
    }
    /// Move the cursor to the end of the current line.
    pub fn end_of_line(&mut self) {
        self.clear_selection();
        let p = self.line_end(self.get_cursor());
        self.set_cursor(p);
    }
    /// Move to the first non-whitespace character of the line.
    pub fn start_of_text(&mut self) {
        self.clear_selection();
        let p = self.line_first_text(self.get_cursor());
        self.set_cursor(p);
    }
    /// Toggle between the first non-whitespace character and column 0.
    pub fn start_of_text_toggle(&mut self) {
        self.clear_selection();
        let cur = self.get_cursor();
        let text = self.line_first_text(cur);
        let p = if cur == text { self.line_start(cur) } else { text };
        self.set_cursor(p);
    }
    /// Extend the selection to the start of the current line.
    pub fn select_to_start_of_line(&mut self) {
        let p = self.line_start(self.get_cursor());
        self.extend_selection(p);
        self.set_cursor(p);
    }
    /// Extend the selection to the end of the current line.
    pub fn select_to_end_of_line(&mut self) {
        let p = self.line_end(self.get_cursor());
        self.extend_selection(p);
        self.set_cursor(p);
    }
    /// Extend the selection to the first non-whitespace character of the line.
    pub fn select_to_start_of_text(&mut self) {
        let p = self.line_first_text(self.get_cursor());
        self.extend_selection(p);
        self.set_cursor(p);
    }
    /// Extend the selection, toggling between the first non-whitespace character and column 0.
    pub fn select_to_start_of_text_toggle(&mut self) {
        let cur = self.get_cursor();
        let text = self.line_first_text(cur);
        let p = if cur == text { self.line_start(cur) } else { text };
        self.extend_selection(p);
        self.set_cursor(p);
    }
    /// Select the whole current line (including its trailing newline if any).
    pub fn select_line(&mut self) {
        let code = self.code_ref();
        let cur = self.get_cursor();
        let row = code.char_to_line(cur);
        let start = code.line_to_char(row);
        let end = if row + 1 < code.len_lines() {
            code.line_to_char(row + 1)
        } else {
            code.line_to_char(row) + code.line_len(row)
        };
        self.set_selection(Some(Selection::new(start, end)));
        self.set_cursor(end);
    }

    // ----- word motions (sub-word approximates whole-word) ----------------
    fn next_word(&self, pos: usize) -> usize {
        let code = self.code_ref();
        let len = code.len_chars();
        if pos >= len {
            return len;
        }
        let (_, end) = code.word_boundaries(pos);
        if end > pos {
            end
        } else {
            // On a boundary/whitespace: skip one then to the next word end.
            let (_, e2) = code.word_boundaries(pos + 1);
            e2.max(pos + 1)
        }
    }
    fn prev_word(&self, pos: usize) -> usize {
        let code = self.code_ref();
        if pos == 0 {
            return 0;
        }
        let (start, _) = code.word_boundaries(pos.saturating_sub(1));
        start.min(pos.saturating_sub(1))
    }

    /// Move the cursor to the start of the next word.
    pub fn word_right(&mut self) {
        self.clear_selection();
        let p = self.next_word(self.get_cursor());
        self.set_cursor(p);
    }
    /// Move the cursor to the start of the previous word.
    pub fn word_left(&mut self) {
        self.clear_selection();
        let p = self.prev_word(self.get_cursor());
        self.set_cursor(p);
    }
    /// Move the cursor right by one sub-word (approximated as a whole word).
    pub fn sub_word_right(&mut self) { self.word_right(); }
    /// Move the cursor left by one sub-word (approximated as a whole word).
    pub fn sub_word_left(&mut self) { self.word_left(); }
    /// Extend the selection to the start of the next word.
    pub fn select_word_right(&mut self) {
        let p = self.next_word(self.get_cursor());
        self.extend_selection(p);
        self.set_cursor(p);
    }
    /// Extend the selection to the start of the previous word.
    pub fn select_word_left(&mut self) {
        let p = self.prev_word(self.get_cursor());
        self.extend_selection(p);
        self.set_cursor(p);
    }
    /// Extend the selection right by one sub-word (approximated as a whole word).
    pub fn select_sub_word_right(&mut self) { self.select_word_right(); }
    /// Extend the selection left by one sub-word (approximated as a whole word).
    pub fn select_sub_word_left(&mut self) { self.select_word_left(); }

    /// Delete from the cursor to the start of the next word.
    pub fn delete_word_right(&mut self) {
        let cur = self.get_cursor();
        let to = self.next_word(cur);
        if to > cur {
            self.set_selection(Some(Selection::new(cur, to)));
            self.set_cursor(to);
            self.apply(Delete {});
        }
    }
    /// Delete from the cursor back to the start of the previous word.
    pub fn delete_word_left(&mut self) {
        let cur = self.get_cursor();
        let to = self.prev_word(cur);
        if to < cur {
            self.set_selection(Some(Selection::new(to, cur)));
            self.set_cursor(cur);
            self.apply(Delete {});
        }
    }
    /// Delete the next sub-word (approximated as a whole word).
    pub fn delete_sub_word_right(&mut self) { self.delete_word_right(); }
    /// Delete the previous sub-word (approximated as a whole word).
    pub fn delete_sub_word_left(&mut self) { self.delete_word_left(); }

    // ----- paragraph motions ---------------------------------------------
    fn paragraph(&self, pos: usize, forward: bool) -> usize {
        let code = self.code_ref();
        let total = code.len_lines();
        let mut row = code.char_to_line(pos);
        let blank = |r: usize| code.line_len(r) == 0;
        if forward {
            if row + 1 >= total {
                return code.len_chars();
            }
            row += 1;
            while row < total && !blank(row) {
                row += 1;
            }
            if row >= total {
                return code.len_chars();
            }
        } else {
            if row == 0 {
                return 0;
            }
            row -= 1;
            while row > 0 && !blank(row) {
                row -= 1;
            }
        }
        code.line_to_char(row)
    }
    /// Move the cursor to the start of the next paragraph.
    pub fn paragraph_next(&mut self) {
        self.clear_selection();
        let p = self.paragraph(self.get_cursor(), true);
        self.set_cursor(p);
    }
    /// Move the cursor to the start of the previous paragraph.
    pub fn paragraph_previous(&mut self) {
        self.clear_selection();
        let p = self.paragraph(self.get_cursor(), false);
        self.set_cursor(p);
    }
    /// Extend the selection to the start of the next paragraph.
    pub fn select_to_paragraph_next(&mut self) {
        let p = self.paragraph(self.get_cursor(), true);
        self.extend_selection(p);
        self.set_cursor(p);
    }
    /// Extend the selection to the start of the previous paragraph.
    pub fn select_to_paragraph_previous(&mut self) {
        let p = self.paragraph(self.get_cursor(), false);
        self.extend_selection(p);
        self.set_cursor(p);
    }

    // ----- vertical paging / scrolling -----------------------------------
    fn move_by_rows(&mut self, rows: usize, down: bool, shift: bool) {
        for _ in 0..rows {
            if down {
                self.apply(MoveDown { shift });
            } else {
                self.apply(MoveUp { shift });
            }
        }
    }
    /// Move the cursor up one page (the viewport height).
    pub fn page_up(&mut self, view_h: usize) { self.move_by_rows(view_h.max(1), false, false); }
    /// Move the cursor down one page (the viewport height).
    pub fn page_down(&mut self, view_h: usize) { self.move_by_rows(view_h.max(1), true, false); }
    /// Extend the selection up one page (the viewport height).
    pub fn select_page_up(&mut self, view_h: usize) { self.move_by_rows(view_h.max(1), false, true); }
    /// Extend the selection down one page (the viewport height).
    pub fn select_page_down(&mut self, view_h: usize) { self.move_by_rows(view_h.max(1), true, true); }
    /// Move the cursor up half a page.
    pub fn half_page_up(&mut self, view_h: usize) { self.move_by_rows((view_h / 2).max(1), false, false); }
    /// Move the cursor down half a page.
    pub fn half_page_down(&mut self, view_h: usize) { self.move_by_rows((view_h / 2).max(1), true, false); }

    /// Center the viewport on the cursor line.
    pub fn center(&mut self, view_h: usize) {
        let row = self.code_ref().char_to_line(self.get_cursor());
        self.set_offset_y(row.saturating_sub(view_h / 2));
    }
    /// Move the cursor to the top/center/bottom visible line, keeping its column.
    pub fn cursor_to_view_top(&mut self) {
        self.clear_selection();
        let target = self.get_offset_y();
        self.set_cursor(self.code_ref().line_to_char(target));
    }
    /// Move the cursor to the center visible line, keeping its column.
    pub fn cursor_to_view_center(&mut self, view_h: usize) {
        self.clear_selection();
        let total = self.code_ref().len_lines().saturating_sub(1);
        let target = (self.get_offset_y() + view_h / 2).min(total);
        self.set_cursor(self.code_ref().line_to_char(target));
    }
    /// Move the cursor to the bottom visible line, keeping its column.
    pub fn cursor_to_view_bottom(&mut self, view_h: usize) {
        self.clear_selection();
        let total = self.code_ref().len_lines().saturating_sub(1);
        let target = (self.get_offset_y() + view_h.saturating_sub(1)).min(total);
        self.set_cursor(self.code_ref().line_to_char(target));
    }

    // ----- editing --------------------------------------------------------
    /// Insert a newline at the cursor.
    pub fn insert_newline(&mut self) { self.apply(InsertNewline {}); }
    /// Insert a tab (indent) at the cursor.
    pub fn insert_tab(&mut self) { self.apply(Indent {}); }
    /// Delete the character before the cursor.
    pub fn backspace(&mut self) { self.apply(Delete {}); }
    /// Forward delete (the character at the cursor).
    pub fn delete(&mut self) {
        self.apply(MoveRight { shift: false });
        self.apply(Delete {});
    }
    /// Undo the last edit.
    pub fn undo(&mut self) { self.apply(Undo {}); }
    /// Redo the last undone edit.
    pub fn redo(&mut self) { self.apply(Redo {}); }
    /// Copy the selection to the clipboard.
    pub fn copy(&mut self) { self.apply(Copy {}); }
    /// Cut the selection to the clipboard.
    pub fn cut(&mut self) { self.apply(Cut {}); }
    /// Paste the clipboard at the cursor.
    pub fn paste(&mut self) { self.apply(Paste {}); }
    /// Select the entire buffer.
    pub fn select_all(&mut self) { self.apply(SelectAll {}); }
    /// Duplicate the selection (or current line).
    pub fn duplicate(&mut self) { self.apply(Duplicate {}); }
    /// Duplicate the current line.
    pub fn duplicate_line(&mut self) { self.apply(Duplicate {}); }
    /// Delete the current line.
    pub fn delete_line(&mut self) { self.apply(DeleteLine {}); }
    /// Indent the current line.
    pub fn indent_line(&mut self) { self.apply(Indent {}); }
    /// Outdent the current line.
    pub fn outdent_line(&mut self) { self.apply(UnIndent {}); }
    /// Indent the selection.
    pub fn indent_selection(&mut self) { self.apply(Indent {}); }
    /// Outdent the selection.
    pub fn outdent_selection(&mut self) { self.apply(UnIndent {}); }
    /// Copy the current line to the clipboard (without changing the selection).
    pub fn copy_line(&mut self) {
        let saved = self.get_selection();
        let saved_cur = self.get_cursor();
        self.select_line();
        self.apply(Copy {});
        self.set_selection(saved);
        self.set_cursor(saved_cur);
    }
    /// Cut the current line to the clipboard.
    pub fn cut_line(&mut self) {
        self.select_line();
        self.apply(Cut {});
    }
    /// Drop the selection, keeping the cursor.
    pub fn deselect(&mut self) { self.clear_selection(); }
    /// Insert literal text (used by paste-primary fallbacks/tests).
    pub fn insert_str_action(&mut self, text: &str) {
        self.apply(InsertText { text: text.to_string() });
    }
}
