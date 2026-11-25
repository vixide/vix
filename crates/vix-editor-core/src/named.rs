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
        add_or(code.line_to_char(row), code.line_len(row), usize::MAX)
    }
    /// Char offset of the first non-blank (not space/tab) character of the line
    /// containing `pos`; the line end when the line is all blanks.
    pub(crate) fn line_first_text(&self, pos: usize) -> usize {
        let code = self.code_ref();
        let row = code.char_to_line(pos);
        let start = code.line_to_char(row);
        let len = code.line_len(row);
        let line = code.slice(start, add_or(start, len, usize::MAX));
        let lead = line.chars().take_while(|c| *c == ' ' || *c == '\t').count();
        add_or(start, lead, usize::MAX)
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
        let next_row = add_or(row, 1, usize::MAX);
        let end = if next_row < code.len_lines() {
            code.line_to_char(next_row)
        } else {
            add_or(code.line_to_char(row), code.line_len(row), usize::MAX)
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
            let after = add_or(pos, 1, usize::MAX);
            let (_, e2) = code.word_boundaries(after);
            e2.max(after)
        }
    }
    /// Char offset of the start of the word at or before the character left of
    /// `pos`; `0` at the buffer start.
    pub(crate) fn prev_word(&self, pos: usize) -> usize {
        let code = self.code_ref();
        if pos == 0 {
            return 0;
        }
        let left = sub_or(pos, 1, 0);
        let (start, _) = code.word_boundaries(left);
        start.min(left)
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
            if add_or(row, 1, usize::MAX) >= total {
                return code.len_chars();
            }
            row = add_or(row, 1, usize::MAX);
            while row < total && !blank(row) {
                row = add_or(row, 1, usize::MAX);
            }
            if row >= total {
                return code.len_chars();
            }
        } else {
            if row == 0 {
                return 0;
            }
            row = sub_or(row, 1, 0);
            while row > 0 && !blank(row) {
                row = sub_or(row, 1, 0);
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
        self.move_by_rows(view_h.div_euclid(2).max(1), false, false);
    }
    /// Move the cursor down half a page.
    pub fn half_page_down(&mut self, view_h: usize) {
        self.move_by_rows(view_h.div_euclid(2).max(1), true, false);
    }

    /// Center the viewport on the cursor line.
    pub fn center(&mut self, view_h: usize) {
        let row = self.code_ref().char_to_line(self.get_cursor());
        self.set_offset_y(sub_or(row, view_h.div_euclid(2), 0));
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
        let total = sub_or(self.code_ref().len_lines(), 1, 0);
        let target = add_or(self.get_offset_y(), view_h.div_euclid(2), usize::MAX).min(total);
        self.set_cursor(self.code_ref().line_to_char(target));
    }
    /// Move the cursor to the bottom visible line (at its start; the old column
    /// is not kept).
    pub fn cursor_to_view_bottom(&mut self, view_h: usize) {
        self.clear_selection();
        let total = sub_or(self.code_ref().len_lines(), 1, 0);
        let target = add_or(self.get_offset_y(), sub_or(view_h, 1, 0), usize::MAX).min(total);
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
    /// Cut from the cursor to the end of its line to the clipboard (Emacs's
    /// `kill-line`). At the end of a line, cut the line break instead, which
    /// joins the next line; at the end of the buffer, do nothing.
    pub fn cut_to_line_end(&mut self) {
        let cur = self.get_cursor();
        let end = self.line_end(cur);
        let to = if end > cur {
            end
        } else if end < self.code_ref().len_chars() {
            add_or(end, 1, usize::MAX)
        } else {
            return;
        };
        self.set_selection(Some(Selection::new(cur, to)));
        self.set_cursor(to);
        self.apply(Cut {});
    }
    /// Select the word under the cursor, or the word that ends just before
    /// it. Does nothing when the cursor touches no word.
    pub fn select_word(&mut self) {
        let cur = self.get_cursor();
        let code = self.code_ref();
        let (mut start, mut end) = code.word_boundaries(cur);
        if start >= end
            && let Some(before) = cur.checked_sub(1)
        {
            (start, end) = code.word_boundaries(before);
        }
        if start >= end {
            return;
        }
        self.set_selection(Some(Selection::new(start, end)));
        self.set_cursor(end);
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
mod cut_to_line_end_and_select_word_tests {
    use crate::editor::Editor;

    fn ed(text: &str, cursor: usize) -> Editor {
        let mut e = Editor::new("rust", text, Vec::new()).unwrap();
        e.set_cursor(cursor);
        e
    }

    fn text(e: &Editor) -> String {
        e.code_ref().get_content()
    }

    #[test]
    fn cut_to_line_end_cuts_the_rest_of_the_line() {
        let mut e = ed("hello world\nnext", 5);
        e.cut_to_line_end();
        assert_eq!(text(&e), "hello\nnext");
        assert_eq!(e.get_cursor(), 5);
    }

    #[test]
    fn cut_to_line_end_at_line_end_joins_the_next_line() {
        let mut e = ed("ab\ncd", 2);
        e.cut_to_line_end();
        assert_eq!(text(&e), "abcd");
    }

    #[test]
    fn cut_to_line_end_at_buffer_end_does_nothing() {
        let mut e = ed("ab", 2);
        e.cut_to_line_end();
        assert_eq!(text(&e), "ab");
    }

    #[test]
    fn select_word_selects_the_word_under_the_cursor() {
        let mut e = ed("one two three", 5);
        e.select_word();
        let s = e.get_selection().unwrap();
        assert_eq!((s.start.min(s.end), s.start.max(s.end)), (4, 7));
    }

    #[test]
    fn select_word_just_after_a_word_selects_it() {
        let mut e = ed("one two", 3);
        e.select_word();
        let s = e.get_selection().unwrap();
        assert_eq!((s.start.min(s.end), s.start.max(s.end)), (0, 3));
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

#[cfg(test)]
mod motion_and_edit_tests {
    //! The named motion / selection / edit wrappers: exact cursor, selection
    //! and text results for each catalog action.

    use crate::editor::Editor;
    use crate::editor_input::tests_support::CLIP;

    fn ed(text: &str, cursor: usize) -> Editor {
        let mut e = Editor::new("rust", text, Vec::new()).unwrap();
        e.set_cursor(cursor);
        e
    }

    fn text(e: &Editor) -> String {
        e.code_ref().get_content()
    }

    fn sel(e: &mut Editor) -> Option<(usize, usize)> {
        e.get_selection().map(|s| s.sorted())
    }

    #[test]
    fn arrow_motions_move_one_step_and_clear_selection() {
        let mut e = ed("abc\ndef", 1);
        e.cursor_right();
        assert_eq!(e.get_cursor(), 2);
        e.cursor_left();
        assert_eq!(e.get_cursor(), 1);
        e.cursor_down();
        assert_eq!(e.get_cursor(), 5);
        e.cursor_up();
        assert_eq!(e.get_cursor(), 1);
        assert_eq!(sel(&mut e), None);
    }

    #[test]
    fn select_arrows_extend_the_selection() {
        let mut e = ed("abc\ndef", 1);
        e.select_right();
        assert_eq!(sel(&mut e), Some((1, 2)));
        e.select_left();
        e.select_left();
        assert_eq!(sel(&mut e), Some((0, 1)));
        let mut e = ed("abc\ndef", 1);
        e.select_down();
        assert_eq!(sel(&mut e), Some((1, 5)));
        let mut e = ed("abc\ndef", 5);
        e.select_up();
        assert_eq!(sel(&mut e), Some((1, 5)));
    }

    #[test]
    fn buffer_start_and_end_motions() {
        let mut e = ed("abc\ndef", 5);
        e.cursor_start();
        assert_eq!(e.get_cursor(), 0);
        e.cursor_end();
        assert_eq!(e.get_cursor(), 7);
        let mut e = ed("abc\ndef", 5);
        e.select_to_start();
        assert_eq!((e.get_cursor(), sel(&mut e)), (0, Some((0, 5))));
        let mut e = ed("abc\ndef", 2);
        e.select_to_end();
        assert_eq!((e.get_cursor(), sel(&mut e)), (7, Some((2, 7))));
    }

    #[test]
    fn line_motions() {
        let mut e = ed("x\n  abc def\ny", 6);
        e.start_of_line();
        assert_eq!(e.get_cursor(), 2);
        e.end_of_line();
        assert_eq!(e.get_cursor(), 11);
        e.start_of_text();
        assert_eq!(e.get_cursor(), 4);
        // Smart Home toggles first-text <-> column 0 <-> first-text.
        e.set_cursor(8);
        e.start_of_text_toggle();
        assert_eq!(e.get_cursor(), 4);
        e.start_of_text_toggle();
        assert_eq!(e.get_cursor(), 2);
        e.start_of_text_toggle();
        assert_eq!(e.get_cursor(), 4);
    }

    #[test]
    fn selecting_line_motions() {
        let mut e = ed("x\n  abc def\ny", 8);
        e.select_to_start_of_line();
        assert_eq!((e.get_cursor(), sel(&mut e)), (2, Some((2, 8))));
        let mut e = ed("x\n  abc def\ny", 8);
        e.select_to_end_of_line();
        assert_eq!((e.get_cursor(), sel(&mut e)), (11, Some((8, 11))));
        let mut e = ed("x\n  abc def\ny", 8);
        e.select_to_start_of_text();
        assert_eq!((e.get_cursor(), sel(&mut e)), (4, Some((4, 8))));
        let mut e = ed("x\n  abc def\ny", 8);
        e.select_to_start_of_text_toggle();
        assert_eq!((e.get_cursor(), sel(&mut e)), (4, Some((4, 8))));
        e.select_to_start_of_text_toggle();
        assert_eq!(e.get_cursor(), 2);
    }

    #[test]
    fn select_line_takes_the_newline_and_handles_last_line() {
        let mut e = ed("ab\ncd", 1);
        e.select_line();
        assert_eq!((e.get_cursor(), sel(&mut e)), (3, Some((0, 3))));
        let mut e = ed("ab\ncd", 4);
        e.select_line();
        assert_eq!((e.get_cursor(), sel(&mut e)), (5, Some((3, 5))));
    }

    #[test]
    fn word_motions() {
        let mut e = ed("foo bar baz", 0);
        e.word_right();
        assert_eq!(e.get_cursor(), 3);
        e.word_right();
        assert_eq!(e.get_cursor(), 7);
        e.sub_word_right();
        assert_eq!(e.get_cursor(), 11);
        e.word_right(); // at the end: stays
        assert_eq!(e.get_cursor(), 11);
        e.word_left();
        assert_eq!(e.get_cursor(), 8);
        e.sub_word_left();
        assert_eq!(e.get_cursor(), 4);
        e.set_cursor(0);
        e.word_left();
        assert_eq!(e.get_cursor(), 0);
    }

    #[test]
    fn selecting_word_motions() {
        let mut e = ed("foo bar baz", 4);
        e.select_word_right();
        assert_eq!(sel(&mut e), Some((4, 7)));
        e.select_sub_word_right();
        assert_eq!(sel(&mut e), Some((4, 11)));
        let mut e = ed("foo bar baz", 11);
        e.select_word_left();
        assert_eq!(sel(&mut e), Some((8, 11)));
        e.select_sub_word_left();
        assert_eq!(sel(&mut e), Some((4, 11)));
    }

    #[test]
    fn word_deletion() {
        let mut e = ed("foo bar baz", 3);
        e.delete_word_right();
        assert_eq!(text(&e), "foo baz");
        let mut e = ed("foo bar baz", 7);
        e.delete_word_left();
        assert_eq!(text(&e), "foo  baz");
        assert_eq!(e.get_cursor(), 4);
        let mut e = ed("foo bar", 0);
        e.delete_sub_word_right();
        assert_eq!(text(&e), " bar");
        let mut e = ed("foo bar", 7);
        e.delete_sub_word_left();
        assert_eq!(text(&e), "foo ");
        // Nothing to delete at the buffer edges.
        let mut e = ed("foo", 3);
        e.delete_word_right();
        let mut e2 = ed("foo", 0);
        e2.delete_word_left();
        assert_eq!((text(&e), text(&e2)), ("foo".into(), "foo".into()));
    }

    #[test]
    fn paragraph_motions() {
        let src = "a\nb\n\nc\nd\n\ne";
        let mut e = ed(src, 0);
        e.paragraph_next();
        assert_eq!(e.get_cursor(), 4); // the first blank line
        e.paragraph_next();
        assert_eq!(e.get_cursor(), 9); // the second blank line
        e.paragraph_next(); // last paragraph: clamps to the end
        assert_eq!(e.get_cursor(), 11);
        e.paragraph_next(); // already on the last line
        assert_eq!(e.get_cursor(), 11);
        e.paragraph_previous();
        assert_eq!(e.get_cursor(), 9);
        e.paragraph_previous();
        assert_eq!(e.get_cursor(), 4);
        e.paragraph_previous();
        assert_eq!(e.get_cursor(), 0);
        e.paragraph_previous();
        assert_eq!(e.get_cursor(), 0);
        let mut e = ed(src, 0);
        e.select_to_paragraph_next();
        assert_eq!(sel(&mut e), Some((0, 4)));
        let mut e = ed(src, 10);
        e.select_to_paragraph_previous();
        assert_eq!(sel(&mut e), Some((9, 10)));
    }

    #[test]
    fn paging_moves_by_the_viewport_height() {
        let body = (0..20)
            .map(|i| format!("l{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let row = |e: &Editor| e.code_ref().char_to_line(e.get_cursor());
        let mut e = ed(&body, 0);
        e.page_down(5);
        assert_eq!(row(&e), 5);
        e.half_page_down(6);
        assert_eq!(row(&e), 8);
        e.half_page_up(6);
        assert_eq!(row(&e), 5);
        e.page_up(5);
        assert_eq!(row(&e), 0);
        // A zero-height viewport still moves one row.
        e.page_down(0);
        assert_eq!(row(&e), 1);
        e.select_page_down(3);
        assert_eq!(row(&e), 4);
        assert!(sel(&mut e).is_some());
        e.select_page_up(2);
        assert_eq!(row(&e), 2);
    }

    #[test]
    fn viewport_relative_cursor_placement() {
        let body = (0..30)
            .map(|i| format!("l{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let row = |e: &Editor| e.code_ref().char_to_line(e.get_cursor());
        let mut e = ed(&body, 0);
        e.set_cursor(e.code_ref().line_to_char(12));
        e.center(10);
        assert_eq!(e.get_offset_y(), 7);
        e.cursor_to_view_top();
        assert_eq!(row(&e), 7);
        e.cursor_to_view_center(10);
        assert_eq!(row(&e), 12);
        e.cursor_to_view_bottom(10);
        assert_eq!(row(&e), 16);
        // Clamped to the last line when the viewport overhangs the buffer.
        e.set_offset_y(28);
        e.cursor_to_view_bottom(10);
        assert_eq!(row(&e), 29);
        e.cursor_to_view_center(10);
        assert_eq!(row(&e), 29);
    }

    #[test]
    fn editing_wrappers() {
        let mut e = ed("ab", 2);
        e.insert_newline();
        assert_eq!(text(&e), "ab\n");
        e.insert_str_action("cd");
        e.backspace();
        assert_eq!(text(&e), "ab\nc");
        e.undo();
        assert_eq!(text(&e), "ab\ncd");
        e.undo();
        assert_eq!(text(&e), "ab\n");
        e.redo();
        assert_eq!(text(&e), "ab\ncd");
        let mut e = ed("ab", 1);
        e.insert_tab();
        assert_eq!(text(&e), "    ab");
        e.indent_line();
        assert_eq!(text(&e), "        ab");
        e.outdent_line();
        assert_eq!(text(&e), "    ab");
        e.select_all();
        e.indent_selection();
        assert_eq!(text(&e), "        ab");
        e.outdent_selection();
        assert_eq!(text(&e), "    ab");
        e.deselect();
        assert_eq!(sel(&mut e), None);
    }

    #[test]
    fn duplicate_and_delete_line() {
        let mut e = ed("ab\ncd", 1);
        e.duplicate_line();
        assert_eq!(text(&e), "ab\nab\ncd");
        e.delete_line();
        assert_eq!(text(&e), "ab\ncd");
        let mut e = ed("ab", 1);
        e.set_selection_range(0, 1);
        e.duplicate();
        assert_eq!(text(&e), "aab");
    }

    #[test]
    fn clipboard_wrappers_round_trip() {
        let _g = CLIP
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut e = ed("one two", 0);
        e.set_selection_range(0, 3);
        e.copy();
        assert_eq!(text(&e), "one two");
        assert_eq!(sel(&mut e), Some((0, 3)));
        e.cut();
        assert_eq!(text(&e), " two");
        e.set_cursor(4);
        e.paste();
        assert_eq!(text(&e), " twoone");
        // copy_line leaves the selection and cursor exactly as they were.
        let mut e = ed("ab\ncd", 4);
        e.set_selection_range(3, 4);
        e.copy_line();
        assert_eq!((e.get_cursor(), sel(&mut e)), (4, Some((3, 4))));
        assert_eq!(e.get_clipboard().unwrap(), "cd");
        e.cut_line();
        assert_eq!(text(&e), "ab\n");
        assert_eq!(e.get_clipboard().unwrap(), "cd");
    }

    #[test]
    fn every_motion_moves_all_carets_when_there_are_several() {
        type Motion = fn(&mut Editor);
        let motions: &[(&str, Motion, bool)] = &[
            ("cursor_up", Editor::cursor_up, false),
            ("cursor_down", Editor::cursor_down, false),
            ("cursor_left", Editor::cursor_left, false),
            ("cursor_right", Editor::cursor_right, false),
            ("select_up", Editor::select_up, true),
            ("select_down", Editor::select_down, true),
            ("select_left", Editor::select_left, true),
            ("select_right", Editor::select_right, true),
            ("cursor_start", Editor::cursor_start, false),
            ("cursor_end", Editor::cursor_end, false),
            ("select_to_start", Editor::select_to_start, true),
            ("select_to_end", Editor::select_to_end, true),
            ("start_of_line", Editor::start_of_line, false),
            ("end_of_line", Editor::end_of_line, false),
            ("start_of_text", Editor::start_of_text, false),
            ("start_of_text_toggle", Editor::start_of_text_toggle, false),
            (
                "select_to_start_of_line",
                Editor::select_to_start_of_line,
                true,
            ),
            ("select_to_end_of_line", Editor::select_to_end_of_line, true),
            (
                "select_to_start_of_text",
                Editor::select_to_start_of_text,
                true,
            ),
            (
                "select_to_start_of_text_toggle",
                Editor::select_to_start_of_text_toggle,
                true,
            ),
            ("word_right", Editor::word_right, false),
            ("word_left", Editor::word_left, false),
            ("select_word_right", Editor::select_word_right, true),
            ("select_word_left", Editor::select_word_left, true),
        ];
        for (name, motion, selects) in motions {
            let mut e = ed("  one two\n  three four\n  five six\nseven", 0);
            // Two carets mid-line on lines 1 and 2, so every motion has room
            // to move both in some direction.
            e.set_cursor(e.code_ref().line_to_char(1) + 6);
            e.add_caret_at(e.code_ref().line_to_char(2) + 6);
            let before = e.caret_positions();
            motion(&mut e);
            let after = e.caret_positions();
            assert_ne!(before, after, "{name} moved the carets");
            if *selects {
                assert!(!e.caret_selections().is_empty(), "{name} selects");
            } else {
                assert!(e.caret_selections().is_empty(), "{name} clears");
            }
        }
    }

    #[test]
    fn select_word_does_nothing_away_from_words() {
        let mut e = ed("a   b", 3);
        e.select_word();
        assert_eq!(sel(&mut e), None);
        let mut e = ed("  x", 0);
        e.select_word();
        assert_eq!(sel(&mut e), None);
    }
}
