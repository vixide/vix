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
}
