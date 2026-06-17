//! Line operations: move the current line up or down. Vix-owned code, held to
//! the crate's `clippy::pedantic`.

use crate::editor::Editor;

impl Editor {
    /// Move the cursor's line up one row, swapping it with the line above. No-op
    /// on the first line. Records one undoable edit and keeps the cursor on the
    /// moved line.
    pub fn move_line_up(&mut self) {
        self.move_line(false);
    }

    /// Move the cursor's line down one row, swapping it with the line below.
    /// No-op on the last line. Records one undoable edit and keeps the cursor on
    /// the moved line.
    pub fn move_line_down(&mut self) {
        self.move_line(true);
    }

    /// Join the current line with the next, or — when the selection spans
    /// several lines — join all of them into one. Adjacent lines are merged with
    /// a single space, trimming the trailing space of each line and the leading
    /// space of the next. Records one undoable edit. No-op when there is nothing
    /// to join (a single line with no line below it).
    pub fn join_lines(&mut self) {
        let text = {
            let code = self.code_ref();
            code.slice(0, code.len_chars())
        };
        let had_final_newline = text.ends_with('\n');
        let mut lines: Vec<&str> = text.split('\n').collect();
        if had_final_newline {
            lines.pop();
        }
        let n = lines.len();
        if n == 0 {
            return;
        }

        // The line range to join: the selection's line span, or the cursor line
        // plus the one below it.
        let (mut a, mut b) = self.line_span();
        if a == b {
            if a + 1 >= n {
                return; // last line, nothing below to join
            }
            b = a + 1;
        }
        a = a.min(n - 1);
        b = b.min(n - 1);

        let mut merged = String::from(lines[a].trim_end());
        for line in &lines[a + 1..=b] {
            let t = line.trim_start();
            if !merged.is_empty() && !t.is_empty() {
                merged.push(' ');
            }
            merged.push_str(t);
        }
        // Cursor lands at the seam after the first line's content.
        let cursor_col = lines[a].trim_end().chars().count();

        let mut rebuilt: Vec<&str> = lines[..a].to_vec();
        rebuilt.push(&merged);
        rebuilt.extend_from_slice(&lines[b + 1..]);
        let mut new_text = rebuilt.join("\n");
        if had_final_newline {
            new_text.push('\n');
        }
        let offset: usize = lines[..a].iter().map(|l| l.chars().count() + 1).sum();
        self.replace_all(&new_text, offset + cursor_col);
    }

    /// Sort the selected lines ascending (byte order), or the whole buffer when
    /// nothing is selected. Records one undoable edit. No-op on fewer than two
    /// lines in range.
    pub fn sort_lines(&mut self) {
        let text = {
            let code = self.code_ref();
            code.slice(0, code.len_chars())
        };
        let had_final_newline = text.ends_with('\n');
        let mut lines: Vec<&str> = text.split('\n').collect();
        if had_final_newline {
            lines.pop();
        }
        let n = lines.len();
        if n < 2 {
            return;
        }
        let (a, b) = match self.get_selection() {
            Some(_) => self.line_span(),
            None => (0, n - 1),
        };
        let (a, b) = (a.min(n - 1), b.min(n - 1));
        if a >= b {
            return;
        }
        lines[a..=b].sort_unstable();
        let mut new_text = lines.join("\n");
        if had_final_newline {
            new_text.push('\n');
        }
        let offset: usize = lines[..a].iter().map(|l| l.chars().count() + 1).sum();
        self.replace_all(&new_text, offset);
    }

    /// The inclusive `(first, last)` line range covered by the selection, or the
    /// cursor line twice when there is no selection.
    fn line_span(&mut self) -> (usize, usize) {
        match self.get_selection() {
            Some(sel) if !sel.is_empty() => {
                let a = self.code_ref().char_to_line(sel.start);
                // A selection ending exactly at a line start does not include
                // that trailing (empty) line.
                let end = if sel.end > sel.start { sel.end - 1 } else { sel.end };
                let b = self.code_ref().char_to_line(end);
                (a.min(b), a.max(b))
            }
            _ => {
                let l = self.cursor_line();
                (l, l)
            }
        }
    }

    /// Replace the whole buffer with `new_text` and put the caret at `cursor`,
    /// as one undoable edit that clears the selection.
    fn replace_all(&mut self, new_text: &str, cursor: usize) {
        let old_cursor = self.cursor;
        let len = self.code_ref().len_chars();
        let selection = self.get_selection();
        let code = self.code_mut();
        code.tx();
        code.set_state_before(old_cursor, selection);
        code.remove(0, len);
        code.insert(0, new_text);
        code.set_state_after(cursor, None);
        code.commit();
        self.set_cursor(cursor);
        self.set_selection(None);
        self.reset_highlight_cache();
    }

    fn move_line(&mut self, down: bool) {
        let cursor = self.cursor;
        let text = {
            let code = self.code_ref();
            code.slice(0, code.len_chars())
        };
        // Split into real lines, setting aside any final newline so it stays at
        // the end after the swap (it is not itself a movable line).
        let had_final_newline = text.ends_with('\n');
        let mut lines: Vec<&str> = text.split('\n').collect();
        if had_final_newline {
            lines.pop();
        }
        let n = lines.len();
        if n < 2 {
            return;
        }

        let cur_line = self.code_ref().char_to_line(cursor).min(n - 1);
        if down && cur_line + 1 >= n {
            return;
        }
        if !down && cur_line == 0 {
            return;
        }
        let target = if down { cur_line + 1 } else { cur_line - 1 };

        // Column within the current line, preserved after the move.
        let line_start = self.code_ref().line_to_char(cur_line);
        let col = cursor - line_start;

        lines.swap(cur_line, target);
        let mut new_text = lines.join("\n");
        if had_final_newline {
            new_text.push('\n');
        }

        // Offset of the moved line in the rebuilt text, plus the preserved column
        // (clamped to the moved line's new length).
        let offset: usize = lines[..target].iter().map(|l| l.chars().count() + 1).sum();
        let moved_len = lines[target].chars().count();
        let new_cursor = offset + col.min(moved_len);

        let len = self.code_ref().len_chars();
        let selection = self.get_selection();
        let code = self.code_mut();
        code.tx();
        code.set_state_before(cursor, selection);
        code.remove(0, len);
        code.insert(0, &new_text);
        code.set_state_after(new_cursor, None);
        code.commit();

        self.set_cursor(new_cursor);
        self.set_selection(None);
        self.reset_highlight_cache();
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;

    fn ed(text: &str, cursor: usize) -> Editor {
        let mut e = Editor::new("text", text, Vec::new()).unwrap();
        e.set_cursor(cursor);
        e
    }

    #[test]
    fn move_down_swaps_with_next_line() {
        let mut e = ed("aaa\nbbb\nccc", 1); // cursor on line 0
        e.move_line_down();
        assert_eq!(e.code_ref().slice(0, e.code_ref().len_chars()), "bbb\naaa\nccc");
    }

    #[test]
    fn move_up_swaps_with_previous_line() {
        let mut e = ed("aaa\nbbb\nccc", 5); // cursor on line 1 ("bbb")
        e.move_line_up();
        assert_eq!(e.code_ref().slice(0, e.code_ref().len_chars()), "bbb\naaa\nccc");
    }

    #[test]
    fn move_up_on_first_line_is_noop() {
        let mut e = ed("aaa\nbbb", 1);
        e.move_line_up();
        assert_eq!(e.code_ref().slice(0, e.code_ref().len_chars()), "aaa\nbbb");
    }

    #[test]
    fn move_down_on_last_line_is_noop() {
        let mut e = ed("aaa\nbbb", 5); // last line, no trailing newline
        e.move_line_down();
        assert_eq!(e.code_ref().slice(0, e.code_ref().len_chars()), "aaa\nbbb");
    }

    #[test]
    fn final_newline_is_preserved() {
        let mut e = ed("aaa\nbbb\n", 1);
        e.move_line_down();
        assert_eq!(e.code_ref().slice(0, e.code_ref().len_chars()), "bbb\naaa\n");
    }

    fn content(e: &Editor) -> String {
        e.code_ref().slice(0, e.code_ref().len_chars())
    }

    #[test]
    fn join_merges_current_line_with_next() {
        let mut e = ed("foo\nbar\nbaz", 0);
        e.join_lines();
        assert_eq!(content(&e), "foo bar\nbaz");
    }

    #[test]
    fn join_trims_boundary_whitespace_to_one_space() {
        let mut e = ed("foo   \n   bar", 0);
        e.join_lines();
        assert_eq!(content(&e), "foo bar");
    }

    #[test]
    fn join_on_last_line_is_noop() {
        let mut e = ed("foo\nbar", 5); // cursor on "bar" (last line)
        e.join_lines();
        assert_eq!(content(&e), "foo\nbar");
    }

    #[test]
    fn join_collapses_selected_lines() {
        let mut e = ed("a\nb\nc\nd", 0);
        e.set_selection_range(0, 5); // covers lines a, b, c
        e.join_lines();
        assert_eq!(content(&e), "a b c\nd");
    }

    #[test]
    fn join_preserves_final_newline() {
        let mut e = ed("foo\nbar\n", 0);
        e.join_lines();
        assert_eq!(content(&e), "foo bar\n");
    }

    #[test]
    fn sort_orders_whole_buffer_when_unselected() {
        let mut e = ed("banana\napple\ncherry", 0);
        e.sort_lines();
        assert_eq!(content(&e), "apple\nbanana\ncherry");
    }

    #[test]
    fn sort_only_selected_lines() {
        let mut e = ed("3\n2\n1\nkeep", 0);
        e.set_selection_range(0, 6); // lines "3","2","1"
        e.sort_lines();
        assert_eq!(content(&e), "1\n2\n3\nkeep");
    }

    #[test]
    fn sort_preserves_final_newline() {
        let mut e = ed("b\na\n", 0);
        e.sort_lines();
        assert_eq!(content(&e), "a\nb\n");
    }

    #[test]
    fn sort_single_line_is_noop() {
        let mut e = ed("only", 0);
        e.sort_lines();
        assert_eq!(content(&e), "only");
    }
}
