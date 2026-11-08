//! Named editor actions (the `spec/actions.tsv` catalog), as `snake_case`
//! methods on [`Editor`]. These wrap the lower-level `Action` structs and rope
//! helpers so the host can dispatch them by name (`run_action`) and bind them to
//! keys. Methods that need the viewport height take it as `view_h`.
//!
//! A handful of catalog actions are genuinely host- or mode-level (tabs, splits,
//! files, macros, shell/command/overwrite modes, suspend); those are dispatched
//! by the host, not here.
//!
//! Conventions shared by the methods below:
//!
//! - Motion methods first offer the move to `multi_motion`; when extra
//!   carets exist (see [`crate::multicursor`]) it moves them all and the
//!   single-caret code is skipped. Plain motions clear the selection; the
//!   `select_*` forms extend it ([`Editor::extend_selection`] keeps the anchor).
//! - Edits go through [`Editor::apply`] so they are undoable and respect
//!   read-only.
//! - "Sub-word" motions are currently aliases of whole-word motions.
//!
//! The authoritative list of names is `crates/vix-editor-core/spec/actions.tsv`;
//! see also `crates/vix-editor-core/spec/index.md`.

use crate::actions::{
    Copy, Cut, Delete, DeleteLine, Duplicate, Indent, InsertNewline, InsertText, MoveDown,
    MoveLeft, MoveRight, MoveUp, Paste, Redo, SelectAll, UnIndent, Undo,
};
use crate::editor::Editor;
use crate::multicursor::CaretMove;
use crate::selection::Selection;

impl Editor {
    // ----- cursor movement ------------------------------------------------
    /// Move the cursor up one line.
    pub fn cursor_up(&mut self) {
        if self.multi_motion(CaretMove::Up, false) {
            return;
        }
        self.apply(MoveUp { shift: false });
    }
    /// Move the cursor down one line.
    pub fn cursor_down(&mut self) {
        if self.multi_motion(CaretMove::Down, false) {
            return;
        }
        self.apply(MoveDown { shift: false });
    }
    /// Move the cursor one character to the left.
    pub fn cursor_left(&mut self) {
        if self.multi_motion(CaretMove::Left, false) {
            return;
        }
        self.apply(MoveLeft { shift: false });
    }
    /// Move the cursor one character to the right.
    pub fn cursor_right(&mut self) {
        if self.multi_motion(CaretMove::Right, false) {
            return;
        }
        self.apply(MoveRight { shift: false });
    }
    /// Extend the selection one line up.
    pub fn select_up(&mut self) {
        if self.multi_motion(CaretMove::Up, true) {
            return;
        }
        self.apply(MoveUp { shift: true });
    }
    /// Extend the selection one line down.
    pub fn select_down(&mut self) {
        if self.multi_motion(CaretMove::Down, true) {
            return;
        }
        self.apply(MoveDown { shift: true });
    }
    /// Extend the selection one character to the left.
    pub fn select_left(&mut self) {
        if self.multi_motion(CaretMove::Left, true) {
            return;
        }
        self.apply(MoveLeft { shift: true });
    }
    /// Extend the selection one character to the right.
    pub fn select_right(&mut self) {
        if self.multi_motion(CaretMove::Right, true) {
            return;
        }
        self.apply(MoveRight { shift: true });
    }

    /// Move the cursor to the very start of the buffer.
    pub fn cursor_start(&mut self) {
        if self.multi_motion(CaretMove::BufferStart, false) {
            return;
        }
        self.clear_selection();
        self.set_cursor(0);
    }
    /// Move the cursor to the very end of the buffer.
    pub fn cursor_end(&mut self) {
        if self.multi_motion(CaretMove::BufferEnd, false) {
            return;
        }
        self.clear_selection();
        self.set_cursor(self.code_ref().len_chars());
    }
    /// Extend the selection to the start of the buffer.
    pub fn select_to_start(&mut self) {
        if self.multi_motion(CaretMove::BufferStart, true) {
            return;
        }
        self.extend_selection(0);
        self.set_cursor(0);
    }
    /// Extend the selection to the end of the buffer.
    pub fn select_to_end(&mut self) {
        if self.multi_motion(CaretMove::BufferEnd, true) {
            return;
        }
        let end = self.code_ref().len_chars();
        self.extend_selection(end);
        self.set_cursor(end);
    }

    // ----- line motions ---------------------------------------------------
    /// Char offset of the start of the line containing `pos`.
    pub(crate) fn line_start(&self, pos: usize) -> usize {
        self.code_ref()
            .line_to_char(self.code_ref().char_to_line(pos))
    }
    /// Char offset just past the last character of `pos`'s line (before its
    /// newline), i.e. the end-of-line cursor position.
    pub(crate) fn line_end(&self, pos: usize) -> usize {
        let code = self.code_ref();
        let row = code.char_to_line(pos);
        code.line_to_char(row) + code.line_len(row)
    }
    /// Char offset of the first non-blank (not space/tab) character of the line
    /// containing `pos`; the line end when the line is all blanks.
    pub(crate) fn line_first_text(&self, pos: usize) -> usize {
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
        if self.multi_motion(CaretMove::LineStart, false) {
            return;
        }
        self.clear_selection();
        let p = self.line_start(self.get_cursor());
        self.set_cursor(p);
    }
    /// Move the cursor to the end of the current line.
    pub fn end_of_line(&mut self) {
        if self.multi_motion(CaretMove::LineEnd, false) {
            return;
        }
        self.clear_selection();
        let p = self.line_end(self.get_cursor());
        self.set_cursor(p);
    }
    /// Move to the first non-blank character of the line.
    pub fn start_of_text(&mut self) {
        if self.multi_motion(CaretMove::TextStart, false) {
            return;
        }
        self.clear_selection();
        let p = self.line_first_text(self.get_cursor());
        self.set_cursor(p);
    }
    /// Smart Home: toggle between the first non-blank character and column 0
    /// (from the first non-blank it goes to column 0, from anywhere else to the
    /// first non-blank).
    pub fn start_of_text_toggle(&mut self) {
        if self.multi_motion(CaretMove::Home, false) {
            return;
        }
        self.clear_selection();
        let cur = self.get_cursor();
        let text = self.line_first_text(cur);
        let p = if cur == text {
            self.line_start(cur)
        } else {
            text
        };
        self.set_cursor(p);
    }
    /// Extend the selection to the start of the current line.
    pub fn select_to_start_of_line(&mut self) {
        if self.multi_motion(CaretMove::LineStart, true) {
            return;
        }
        let p = self.line_start(self.get_cursor());
        self.extend_selection(p);
        self.set_cursor(p);
    }
    /// Extend the selection to the end of the current line.
    pub fn select_to_end_of_line(&mut self) {
        if self.multi_motion(CaretMove::LineEnd, true) {
            return;
        }
        let p = self.line_end(self.get_cursor());
        self.extend_selection(p);
        self.set_cursor(p);
    }
    /// Extend the selection to the first non-whitespace character of the line.
    pub fn select_to_start_of_text(&mut self) {
        if self.multi_motion(CaretMove::TextStart, true) {
            return;
        }
        let p = self.line_first_text(self.get_cursor());
        self.extend_selection(p);
        self.set_cursor(p);
    }
    /// Extend the selection, toggling between the first non-whitespace character and column 0.
    pub fn select_to_start_of_text_toggle(&mut self) {
        if self.multi_motion(CaretMove::Home, true) {
            return;
        }
        let cur = self.get_cursor();
        let text = self.line_first_text(cur);
        let p = if cur == text {
            self.line_start(cur)
        } else {
            text
        };
        self.extend_selection(p);
        self.set_cursor(p);
    }
    /// Select the whole current line (including its trailing newline if any).
    /// The cursor ends at the selection end, so repeating is not cumulative.
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
    /// Char offset of the *end* of the word at or after `pos` (the VS Code
    /// style "word right" stop); `pos` itself when already at the buffer end.
    pub(crate) fn next_word(&self, pos: usize) -> usize {
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
    /// Char offset of the start of the word at or before the character left of
    /// `pos`; `0` at the buffer start.
    pub(crate) fn prev_word(&self, pos: usize) -> usize {
        let code = self.code_ref();
        if pos == 0 {
            return 0;
        }
        let (start, _) = code.word_boundaries(pos.saturating_sub(1));
        start.min(pos.saturating_sub(1))
    }

    /// Move the cursor to the end of the current/next word (the stop
    /// `next_word` computes).
    pub fn word_right(&mut self) {
        if self.multi_motion(CaretMove::WordRight, false) {
            return;
        }
        self.clear_selection();
        let p = self.next_word(self.get_cursor());
        self.set_cursor(p);
    }
    /// Move the cursor to the start of the previous word.
    pub fn word_left(&mut self) {
        if self.multi_motion(CaretMove::WordLeft, false) {
            return;
        }
        self.clear_selection();
        let p = self.prev_word(self.get_cursor());
        self.set_cursor(p);
    }
    /// Move the cursor right by one sub-word (approximated as a whole word).
    pub fn sub_word_right(&mut self) {
        self.word_right();
    }
    /// Move the cursor left by one sub-word (approximated as a whole word).
    pub fn sub_word_left(&mut self) {
        self.word_left();
    }
    /// Extend the selection to the end of the current/next word.
    pub fn select_word_right(&mut self) {
        if self.multi_motion(CaretMove::WordRight, true) {
            return;
        }
        let p = self.next_word(self.get_cursor());
        self.extend_selection(p);
        self.set_cursor(p);
    }
    /// Extend the selection to the start of the previous word.
    pub fn select_word_left(&mut self) {
        if self.multi_motion(CaretMove::WordLeft, true) {
            return;
        }
        let p = self.prev_word(self.get_cursor());
        self.extend_selection(p);
        self.set_cursor(p);
    }
    /// Extend the selection right by one sub-word (approximated as a whole word).
    pub fn select_sub_word_right(&mut self) {
        self.select_word_right();
    }
    /// Extend the selection left by one sub-word (approximated as a whole word).
    pub fn select_sub_word_left(&mut self) {
        self.select_word_left();
    }

    /// Delete from the cursor to the end of the current/next word.
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
    pub fn delete_sub_word_right(&mut self) {
        self.delete_word_right();
    }
    /// Delete the previous sub-word (approximated as a whole word).
    pub fn delete_sub_word_left(&mut self) {
        self.delete_word_left();
    }

    // ----- paragraph motions ---------------------------------------------
    /// Char offset of the next (`forward`) or previous blank line from `pos`,
    /// clamping to the buffer end or start when there is none. A paragraph
    /// break is a line with zero characters (whitespace-only lines are not
    /// blank).
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
    /// Apply `rows` single-line moves. Going through `MoveUp`/`MoveDown`
    /// (rather than jumping) preserves the goal column across short lines.
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
    pub fn page_up(&mut self, view_h: usize) {
        self.move_by_rows(view_h.max(1), false, false);
    }
    /// Move the cursor down one page (the viewport height).
    pub fn page_down(&mut self, view_h: usize) {
        self.move_by_rows(view_h.max(1), true, false);
    }
    /// Extend the selection up one page (the viewport height).
    pub fn select_page_up(&mut self, view_h: usize) {
        self.move_by_rows(view_h.max(1), false, true);
    }
    /// Extend the selection down one page (the viewport height).
    pub fn select_page_down(&mut self, view_h: usize) {
        self.move_by_rows(view_h.max(1), true, true);
    }
    /// Move the cursor up half a page.
    pub fn half_page_up(&mut self, view_h: usize) {
        self.move_by_rows((view_h / 2).max(1), false, false);
    }
    /// Move the cursor down half a page.
    pub fn half_page_down(&mut self, view_h: usize) {
        self.move_by_rows((view_h / 2).max(1), true, false);
    }

    /// Center the viewport on the cursor line.
    pub fn center(&mut self, view_h: usize) {
        let row = self.code_ref().char_to_line(self.get_cursor());
        self.set_offset_y(row.saturating_sub(view_h / 2));
    }
    /// Move the cursor to the top visible line. (The cursor lands at the start of
    /// that line; the old column is not kept.)
    pub fn cursor_to_view_top(&mut self) {
        self.clear_selection();
        let target = self.get_offset_y();
        self.set_cursor(self.code_ref().line_to_char(target));
    }
    /// Move the cursor to the center visible line (at its start; the old column
    /// is not kept).
    pub fn cursor_to_view_center(&mut self, view_h: usize) {
        self.clear_selection();
        let total = self.code_ref().len_lines().saturating_sub(1);
        let target = (self.get_offset_y() + view_h / 2).min(total);
        self.set_cursor(self.code_ref().line_to_char(target));
    }
    /// Move the cursor to the bottom visible line (at its start; the old column
    /// is not kept).
    pub fn cursor_to_view_bottom(&mut self, view_h: usize) {
        self.clear_selection();
        let total = self.code_ref().len_lines().saturating_sub(1);
        let target = (self.get_offset_y() + view_h.saturating_sub(1)).min(total);
        self.set_cursor(self.code_ref().line_to_char(target));
    }

    // ----- editing --------------------------------------------------------
    /// Insert a newline at the cursor.
    pub fn insert_newline(&mut self) {
        self.apply(InsertNewline {});
    }
    /// Insert a tab (indent) at the cursor.
    pub fn insert_tab(&mut self) {
        self.apply(Indent {});
    }
    /// Delete the character before the cursor.
    pub fn backspace(&mut self) {
        self.apply(Delete {});
    }
    /// Forward delete: at every caret, delete its pending selection, else
    /// exactly the one character at the caret (never an indent unit). At the
    /// end of the buffer it is a no-op, not a backspace. One undo step.
    pub fn delete(&mut self) {
        // Not "move right, then backspace": stepping right collapses a
        // selection, and backspace in indentation removes a whole indent unit.
        self.multi_delete(true);
    }
    /// Undo the last edit.
    pub fn undo(&mut self) {
        self.apply(Undo {});
    }
    /// Redo the last undone edit.
    pub fn redo(&mut self) {
        self.apply(Redo {});
    }
    /// Copy the selection to the clipboard.
    pub fn copy(&mut self) {
        self.apply(Copy {});
    }
    /// Cut the selection to the clipboard.
    pub fn cut(&mut self) {
        self.apply(Cut {});
    }
    /// Paste the clipboard at the cursor.
    pub fn paste(&mut self) {
        self.apply(Paste {});
    }
    /// Select the entire buffer.
    pub fn select_all(&mut self) {
        self.apply(SelectAll {});
    }
    /// Duplicate the selection (or current line).
    pub fn duplicate(&mut self) {
        self.apply(Duplicate {});
    }
    /// Duplicate the current line (same action as [`Editor::duplicate`], which
    /// already falls back to the line when nothing is selected).
    pub fn duplicate_line(&mut self) {
        self.apply(Duplicate {});
    }
    /// Delete the current line.
    pub fn delete_line(&mut self) {
        self.apply(DeleteLine {});
    }
    /// Indent the current line.
    pub fn indent_line(&mut self) {
        self.apply(Indent {});
    }
    /// Outdent the current line.
    pub fn outdent_line(&mut self) {
        self.apply(UnIndent {});
    }
    /// Indent the selection.
    pub fn indent_selection(&mut self) {
        self.apply(Indent {});
    }
    /// Outdent the selection.
    pub fn outdent_selection(&mut self) {
        self.apply(UnIndent {});
    }
    /// Copy the current line to the clipboard (without changing the selection).
    pub fn copy_line(&mut self) {
        // Select, copy, then restore: the user's selection is observably
        // unchanged.
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
    pub fn deselect(&mut self) {
        self.clear_selection();
    }
    /// Insert literal text (used by paste-primary fallbacks/tests).
    pub fn insert_str_action(&mut self, text: &str) {
        self.apply(InsertText {
            text: text.to_string(),
        });
    }
}

#[cfg(test)]
mod forward_delete_tests {
    //! Forward delete (`delete`, the catalog's `delete` action): it removes a
    //! pending selection, else exactly the one character at the cursor —
    //! never an indent unit, and never the character behind the cursor.

    use crate::editor::Editor;

    /// A Rust buffer (a known language, so backspace in indentation removes
    /// the whole indent — the behaviour forward delete must not inherit).
    fn ed(text: &str, cursor: usize) -> Editor {
        let mut e = Editor::new("rust", text, Vec::new()).unwrap();
        e.set_cursor(cursor);
        e
    }

    fn text(e: &Editor) -> String {
        e.code_ref().get_content()
    }

    #[test]
    fn deletes_the_char_at_the_cursor() {
        let mut e = ed("abcd", 1);
        e.delete();
        assert_eq!(text(&e), "acd");
        assert_eq!(e.get_cursor(), 1);
    }

    #[test]
    fn deletes_a_pending_selection_whole() {
        let mut e = ed("abcd", 0);
        e.set_selection_range(0, 2);
        e.delete();
        assert_eq!(text(&e), "cd");
        assert_eq!(e.get_cursor(), 0);
        assert!(e.get_selection().is_none_or(|s| s.is_empty()));
    }

    #[test]
    fn deletes_a_backward_selection_whole() {
        let mut e = ed("abcd", 3);
        e.set_selection_range(3, 1); // anchor 3, cursor 1
        e.delete();
        assert_eq!(text(&e), "ad");
        assert_eq!(e.get_cursor(), 1);
    }

    #[test]
    fn deletes_a_multi_line_selection_whole() {
        let mut e = ed("ab\ncd\nef", 0);
        e.set_selection_range(1, 7); // "b\ncd\ne"
        e.delete();
        assert_eq!(text(&e), "af");
        assert_eq!(e.get_cursor(), 1);
    }

    #[test]
    fn inside_space_indentation_deletes_one_space() {
        let mut e = ed("    x", 2);
        e.delete();
        assert_eq!(text(&e), "   x");
        assert_eq!(e.get_cursor(), 2);
    }

    #[test]
    fn at_the_end_of_space_indentation_deletes_the_text_char() {
        let mut e = ed("    xy", 4);
        e.delete();
        assert_eq!(text(&e), "    y");
    }

    #[test]
    fn inside_tab_indentation_deletes_one_tab() {
        let mut e = ed("\t\t\tx", 1);
        e.set_indent(Some("\t".to_string()));
        e.delete();
        assert_eq!(text(&e), "\t\tx");
        assert_eq!(e.get_cursor(), 1);
    }

    #[test]
    fn backspace_in_indentation_still_removes_the_whole_indent() {
        // The contrast case: backward delete keeps its indent-unit behaviour.
        let mut e = ed("    x", 2);
        e.backspace();
        assert_eq!(text(&e), "  x");
        assert_eq!(e.get_cursor(), 0);
    }

    #[test]
    fn at_the_end_of_a_line_joins_the_next_line() {
        let mut e = ed("ab\ncd", 2);
        e.delete();
        assert_eq!(text(&e), "abcd");
        assert_eq!(e.get_cursor(), 2);
    }

    #[test]
    fn at_the_end_of_a_crlf_line_removes_only_the_cr() {
        // One character at a time, like the cursor moves.
        let mut e = ed("ab\r\ncd", 2);
        e.delete();
        assert_eq!(text(&e), "ab\ncd");
    }

    #[test]
    fn at_the_end_of_the_buffer_is_a_no_op() {
        let mut e = ed("abc", 3);
        e.delete();
        assert_eq!(text(&e), "abc");
        assert_eq!(e.get_cursor(), 3);
        // Nothing happened, so there is nothing to undo.
        e.undo();
        assert_eq!(text(&e), "abc");
    }

    #[test]
    fn in_an_empty_buffer_is_a_no_op() {
        let mut e = ed("", 0);
        e.delete();
        assert_eq!(text(&e), "");
        assert_eq!(e.get_cursor(), 0);
    }

    #[test]
    fn deletes_one_multi_byte_char() {
        let mut e = ed("aéb", 1);
        e.delete();
        assert_eq!(text(&e), "ab");
        let mut e = ed("a😀b", 1);
        e.delete();
        assert_eq!(text(&e), "ab");
    }

    #[test]
    fn deletes_one_wide_cjk_char() {
        let mut e = ed("中文字", 1);
        e.delete();
        assert_eq!(text(&e), "中字");
        assert_eq!(e.get_cursor(), 1);
    }

    #[test]
    fn deletes_a_base_char_and_leaves_its_combining_mark() {
        // The editor's unit is the `char` (cursor motion steps one `char`),
        // so a combining mark is a character of its own.
        let mut e = ed("e\u{301}x", 0);
        e.delete();
        assert_eq!(text(&e), "\u{301}x");
        let mut e = ed("e\u{301}x", 1);
        e.delete();
        assert_eq!(text(&e), "ex");
    }

    #[test]
    fn at_every_caret_deletes_one_char_or_its_selection() {
        let mut e = ed("abcd\nefgh\nij", 1);
        e.add_caret_at(6); // 'f'
        e.add_caret_at(10); // 'i'
        e.delete();
        assert_eq!(text(&e), "acd\negh\nj");
    }

    #[test]
    fn at_every_caret_never_deletes_an_indent_unit() {
        let mut e = ed("    a\n    b", 1);
        e.add_caret_at(7);
        e.delete();
        assert_eq!(text(&e), "   a\n   b");
    }

    #[test]
    fn undo_restores_the_text_and_cursor() {
        for (src, at, sel) in [
            ("abcd", 1, None),
            ("abcd", 0, Some((0, 2))),
            ("ab\ncd\nef", 0, Some((1, 7))),
            ("    x", 2, None),
            ("ab\ncd", 2, None),
            ("中文字", 1, None),
        ] {
            let mut e = ed(src, at);
            if let Some((a, c)) = sel {
                e.set_selection_range(a, c);
            }
            let before = e.get_cursor();
            e.delete();
            assert_ne!(text(&e), src, "{src:?} should change");
            e.undo();
            assert_eq!(text(&e), src, "undo of delete in {src:?}");
            assert_eq!(e.get_cursor(), before, "undo cursor in {src:?}");
            e.redo();
            assert_ne!(text(&e), src, "redo of delete in {src:?}");
        }
    }

    #[test]
    fn multi_delete_forward_matches_the_single_caret_rules() {
        // The Delete key with several carets calls `multi_delete(true)`.
        let mut e = ed("    a\n    b", 2);
        e.add_caret_at(8);
        e.multi_delete(true);
        assert_eq!(text(&e), "   a\n   b");

        let mut e = ed("中文\ne\u{301}", 0);
        e.add_caret_at(3);
        e.multi_delete(true);
        assert_eq!(text(&e), "文\n\u{301}");

        // At the very end a caret is a no-op, not a backspace.
        let mut e = ed("ab\ncd", 2);
        e.add_caret_at(5);
        e.multi_delete(true);
        assert_eq!(text(&e), "abcd");
    }
}
