//! Line operations on [`Editor`]: move the current line up or down (including
//! collapsed folds as one unit), join lines, and the "lines in range" family
//! (sort, reverse, shuffle, de-duplicate, trim trailing whitespace). Vix-owned
//! code, held to the crate's `clippy::pedantic`.
//!
//! All operations share one strategy: read the whole buffer, split it into
//! lines (setting aside a final newline so it stays last), transform the line
//! vector, and write the whole text back as **one** undoable edit via
//! `replace_all` / `move_line`. That is simple and keeps undo atomic at the cost
//! of rewriting the buffer; these are explicit user commands, not a hot path.
//! "Range" means the selection's line span, or the whole buffer when there is
//! no selection. See `crates/vix-editor-core/spec/index.md`.

use crate::editor::Editor;

/// The lines before index `a` (all of them when `a` is past the end).
fn before<'a>(lines: &'a [&'a str], a: usize) -> &'a [&'a str] {
    lines.get(..a).unwrap_or(lines)
}

/// The lines from index `a` on (none when `a` is past the end).
fn from<'a>(lines: &'a [&'a str], a: usize) -> &'a [&'a str] {
    lines.get(a..).unwrap_or(&[])
}

/// The lines from index `a` up to but not including `b` (none for a range that is not inside).
fn between<'a>(lines: &'a [&'a str], a: usize, b: usize) -> &'a [&'a str] {
    lines.get(a..b).unwrap_or(&[])
}

/// How many chars the first `upto` lines take up with their newlines.
fn chars_before(lines: &[&str], upto: usize) -> usize {
    before(lines, upto).iter().fold(0, |total, l| {
        add_or(total, add_or(l.chars().count(), 1, usize::MAX), usize::MAX)
    })
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
    /// a single space: the first line's trailing whitespace and each following
    /// line's leading whitespace are trimmed (a later line's own trailing
    /// whitespace is kept), and no space is added next to an empty side.
    /// Records one undoable edit and leaves the cursor at the seam after the
    /// first line's content. No-op when there is nothing to join (a single line
    /// with no line below it).
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
            let below = add_or(a, 1, usize::MAX);
            if below >= n {
                return; // last line, nothing below to join
            }
            b = below;
        }
        let last = sub_or(n, 1, 0);
        a = a.min(last);
        b = b.min(last);

        let mut merged = String::from(lines.get(a).copied().unwrap_or("").trim_end());
        for line in between(&lines, add_or(a, 1, usize::MAX), add_or(b, 1, usize::MAX)) {
            let t = line.trim_start();
            if !merged.is_empty() && !t.is_empty() {
                merged.push(' ');
            }
            merged.push_str(t);
        }
        // Cursor lands at the seam after the first line's content.
        let cursor_col = lines
            .get(a)
            .copied()
            .unwrap_or("")
            .trim_end()
            .chars()
            .count();

        let mut rebuilt: Vec<&str> = before(&lines, a).to_vec();
        rebuilt.push(&merged);
        rebuilt.extend_from_slice(from(&lines, add_or(b, 1, usize::MAX)));
        let mut new_text = rebuilt.join("\n");
        if had_final_newline {
            new_text.push('\n');
        }
        let offset = chars_before(&lines, a);
        self.replace_all(&new_text, add_or(offset, cursor_col, usize::MAX));
    }

    /// Sort the selected lines ascending (byte order), or the whole buffer when
    /// nothing is selected. Records one undoable edit. No-op on fewer than two
    /// lines in range.
    ///
    /// Unlike the other range operations, an *empty* selection (a bare caret
    /// that still counts as `Some`) is treated as a one-line range, so it does
    /// nothing rather than sorting the whole buffer.
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
            None => (0, sub_or(n, 1, 0)),
        };
        let last = sub_or(n, 1, 0);
        let (a, b) = (a.min(last), b.min(last));
        if a >= b {
            return;
        }
        if let Some(range) = lines.get_mut(a..=b) {
            range.sort_unstable();
        }
        let mut new_text = lines.join("\n");
        if had_final_newline {
            new_text.push('\n');
        }
        let offset = chars_before(&lines, a);
        self.replace_all(&new_text, offset);
    }

    /// Strip trailing whitespace from each line in range (selection, or the whole
    /// buffer when nothing is selected).
    pub fn trim_trailing_whitespace(&mut self) {
        self.edit_line_range(|lines| lines.iter().map(|l| l.trim_end().to_string()).collect());
    }

    /// Remove duplicate lines in range, keeping the first occurrence of each.
    pub fn remove_duplicate_lines(&mut self) {
        self.edit_line_range(|lines| {
            let mut seen = std::collections::HashSet::new();
            lines
                .iter()
                .filter(|l| seen.insert(l.to_string()))
                .map(|l| (*l).to_string())
                .collect()
        });
    }

    /// Reverse the order of the lines in range.
    pub fn reverse_lines(&mut self) {
        self.edit_line_range(|lines| lines.iter().rev().map(|l| (*l).to_string()).collect());
    }

    /// Randomly reorder the lines in range (Fisher–Yates, seeded from the OS).
    /// Not reproducible by design; the pure `shuffle` helper is what tests pin.
    pub fn shuffle_lines(&mut self) {
        let seed = random_seed();
        self.edit_line_range(|lines| shuffle(lines, seed));
    }

    /// Sort the lines in range ascending and drop duplicates.
    pub fn sort_unique(&mut self) {
        self.edit_line_range(|lines| {
            let mut v: Vec<String> = lines.iter().map(|l| (*l).to_string()).collect();
            v.sort_unstable();
            v.dedup();
            v
        });
    }

    /// Apply `f` to the lines in range (selection, or the whole buffer when
    /// nothing is selected), replacing them with its result as one undoable edit.
    /// The caret lands at the start of the range.
    fn edit_line_range(&mut self, f: impl FnOnce(&[&str]) -> Vec<String>) {
        let text = {
            let code = self.code_ref();
            code.slice(0, code.len_chars())
        };
        // `split('\n')` yields a trailing empty piece for a final newline;
        // drop it and restore the newline after the transform.
        let had_final_newline = text.ends_with('\n');
        let mut lines: Vec<&str> = text.split('\n').collect();
        if had_final_newline {
            lines.pop();
        }
        let n = lines.len();
        if n == 0 {
            return;
        }
        let (a, b) = match self.get_selection() {
            Some(s) if !s.is_empty() => self.line_span(),
            _ => (0, sub_or(n, 1, 0)),
        };
        let last = sub_or(n, 1, 0);
        let (a, b) = (a.min(last), b.min(last));
        let mut parts: Vec<String> = before(&lines, a).iter().map(|s| (*s).to_string()).collect();
        parts.extend(f(between(&lines, a, add_or(b, 1, usize::MAX))));
        parts.extend(
            from(&lines, add_or(b, 1, usize::MAX))
                .iter()
                .map(|s| (*s).to_string()),
        );
        let mut new_text = parts.join("\n");
        if had_final_newline {
            new_text.push('\n');
        }
        // Char offset of the range start: each earlier line plus its `\n`
        // (offsets are in chars, not bytes).
        let offset = chars_before(&lines, a);
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
                let end = if sel.end > sel.start {
                    sub_or(sel.end, 1, 0)
                } else {
                    sel.end
                };
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
    /// as one undoable edit that clears the selection. The undo state records
    /// the old cursor and selection so undo restores them exactly.
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

    /// Shared body of [`Editor::move_line_up`] / [`Editor::move_line_down`]:
    /// swap the cursor's block (the line, or the whole collapsed fold it heads)
    /// with the adjacent block, keeping the cursor column (clamped) and moving
    /// other collapsed folds inside the swapped region along with their lines.
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

        let last_line = sub_or(n, 1, 0);
        let cur_line = self.code_ref().char_to_line(cursor).min(last_line);

        // T580: a collapsed fold moves as one unit. The moved block is the
        // cursor line plus its hidden body; the neighbour it swaps with is
        // the adjacent visible line plus *its* hidden body, so a fold is
        // never split or moved into.
        let block_end = |line: usize| {
            self.folds
                .iter()
                .find(|f| f.0 == line)
                .map_or(line, |f| f.1.min(last_line))
        };
        let block_start = |line: usize| {
            self.folds
                .iter()
                .find(|f| f.0 < line && line <= f.1)
                .map_or(line, |f| f.0)
        };
        let moved = (cur_line, block_end(cur_line));
        let (first, second) = if down {
            let nb = add_or(moved.1, 1, usize::MAX);
            if nb >= n {
                return;
            }
            (moved, (nb, block_end(nb)))
        } else {
            let Some(nb_end) = moved.0.checked_sub(1) else {
                return;
            };
            ((block_start(nb_end), nb_end), moved)
        };

        // Column within the current line, preserved after the move.
        let line_start = self.code_ref().line_to_char(cur_line);
        let col = sub_or(cursor, line_start, 0);

        let mut order: Vec<&str> = Vec::new();
        order.extend(before(&lines, first.0));
        order.extend(between(&lines, second.0, add_or(second.1, 1, usize::MAX)));
        order.extend(between(&lines, first.0, add_or(first.1, 1, usize::MAX)));
        order.extend(from(&lines, add_or(second.1, 1, usize::MAX)));
        let first_len = add_or(sub_or(first.1, first.0, 0), 1, usize::MAX);
        let second_len = add_or(sub_or(second.1, second.0, 0), 1, usize::MAX);
        // Where the moved block's first line lands.
        let target = if down {
            add_or(first.0, second_len, usize::MAX)
        } else {
            first.0
        };
        let lines = order;
        let mut new_text = lines.join("\n");
        if had_final_newline {
            new_text.push('\n');
        }

        // Carry any other collapsed folds inside the swapped region with
        // their lines.
        for f in &mut self.folds {
            if f.0 >= first.0 && f.1 <= first.1 {
                f.0 = add_or(f.0, second_len, usize::MAX);
                f.1 = add_or(f.1, second_len, usize::MAX);
            } else if f.0 >= second.0 && f.1 <= second.1 {
                f.0 = sub_or(f.0, first_len, 0);
                f.1 = sub_or(f.1, first_len, 0);
            }
        }

        // Offset of the moved line in the rebuilt text, plus the preserved column
        // (clamped to the moved line's new length).
        let offset = chars_before(&lines, target);
        let moved_len = lines.get(target).map_or(0, |l| l.chars().count());
        let new_cursor = add_or(offset, col.min(moved_len), usize::MAX);

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

/// A 64-bit seed from the OS RNG (falls back to a fixed seed if unavailable).
/// A failing RNG must not make the command fail, only make it less random.
fn random_seed() -> u64 {
    let mut buf = [0u8; 8];
    if getrandom::fill(&mut buf).is_err() {
        return 0x9E37_79B9_7F4A_7C15; // golden-ratio constant fallback
    }
    u64::from_le_bytes(buf)
}

/// Fisher–Yates shuffle of `lines` using an xorshift64 PRNG seeded by `seed`.
/// Pure (seed in, permutation out) so it is deterministic and unit-testable.
fn shuffle(lines: &[&str], seed: u64) -> Vec<String> {
    let mut v: Vec<String> = lines.iter().map(|l| (*l).to_string()).collect();
    // xorshift has an all-zero fixed point, so a zero seed would never move.
    let mut state = if seed == 0 {
        0x9E37_79B9_7F4A_7C15
    } else {
        seed
    };
    for i in (1..v.len()).rev() {
        // xorshift64
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        // Modulo bias is negligible for line counts and irrelevant to a
        // shuffle command.
        let j = u64::try_from(i)
            .ok()
            .and_then(|i| i.checked_add(1))
            .and_then(|modulus| state.checked_rem(modulus))
            .and_then(|r| usize::try_from(r).ok())
            .unwrap_or(0);
        v.swap(i, j);
    }
    v
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;

    fn ed(text: &str, cursor: usize) -> Editor {
        let mut e = Editor::new("text", text, Vec::new()).unwrap();
        e.set_cursor(cursor);
        e
    }

    fn text(e: &Editor) -> String {
        e.code_ref().slice(0, e.code_ref().len_chars())
    }

    /// T580: a collapsed fold moves as one unit -- down past a plain line,
    /// up past a plain line, and a plain line moving past a whole fold.
    #[test]
    fn collapsed_fold_moves_as_a_unit() {
        // Lines: 0 head, 1 body1, 2 body2, 3 plain. Fold 0..=2.
        let mut e = ed("head\nb1\nb2\nplain", 0);
        assert!(e.toggle_manual_fold(0, 2));
        e.move_line_down();
        assert_eq!(text(&e), "plain\nhead\nb1\nb2");
        assert_eq!(
            e.code_ref().char_to_line(e.get_cursor()),
            1,
            "cursor on head"
        );
        assert!(e.is_line_hidden(2) && e.is_line_hidden(3) && !e.is_line_hidden(1));
        e.move_line_up();
        assert_eq!(text(&e), "head\nb1\nb2\nplain");
        assert!(e.is_line_hidden(1) && e.is_line_hidden(2) && !e.is_line_hidden(3));

        // A plain line moving up past a collapsed fold jumps the whole fold.
        let mut e = ed("head\nb1\nb2\nplain", 0);
        e.toggle_manual_fold(0, 2);
        let plain = e.code_ref().line_to_char(3);
        e.set_cursor(plain);
        e.move_line_up();
        assert_eq!(text(&e), "plain\nhead\nb1\nb2");
        assert!(e.is_line_hidden(2) && e.is_line_hidden(3) && !e.is_line_hidden(1));
    }

    #[test]
    fn two_adjacent_collapsed_folds_swap_whole() {
        let mut e = ed("a\na1\nb\nb1", 0);
        e.toggle_manual_fold(0, 1);
        e.toggle_manual_fold(2, 3);
        e.move_line_down();
        assert_eq!(text(&e), "b\nb1\na\na1");
        assert!(e.is_line_hidden(1) && e.is_line_hidden(3));
        assert!(!e.is_line_hidden(2));
    }

    #[test]
    fn move_down_swaps_with_next_line() {
        let mut e = ed("aaa\nbbb\nccc", 1); // cursor on line 0
        e.move_line_down();
        assert_eq!(
            e.code_ref().slice(0, e.code_ref().len_chars()),
            "bbb\naaa\nccc"
        );
    }

    #[test]
    fn move_up_swaps_with_previous_line() {
        let mut e = ed("aaa\nbbb\nccc", 5); // cursor on line 1 ("bbb")
        e.move_line_up();
        assert_eq!(
            e.code_ref().slice(0, e.code_ref().len_chars()),
            "bbb\naaa\nccc"
        );
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
        assert_eq!(
            e.code_ref().slice(0, e.code_ref().len_chars()),
            "bbb\naaa\n"
        );
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

    #[test]
    fn trim_trailing_whitespace_clears_line_ends() {
        let mut e = ed("foo  \nbar\t\nbaz", 0);
        e.trim_trailing_whitespace();
        assert_eq!(content(&e), "foo\nbar\nbaz");
    }

    #[test]
    fn remove_duplicate_lines_keeps_first() {
        let mut e = ed("a\nb\na\nc\nb\n", 0);
        e.remove_duplicate_lines();
        assert_eq!(content(&e), "a\nb\nc\n");
    }

    #[test]
    fn reverse_lines_flips_order() {
        let mut e = ed("one\ntwo\nthree\n", 0);
        e.reverse_lines();
        assert_eq!(content(&e), "three\ntwo\none\n");
    }

    #[test]
    fn sort_unique_sorts_and_dedupes() {
        let mut e = ed("banana\napple\nbanana\ncherry\napple", 0);
        e.sort_unique();
        assert_eq!(content(&e), "apple\nbanana\ncherry");
    }

    #[test]
    fn shuffle_is_a_permutation_and_seed_deterministic() {
        let lines = vec!["a", "b", "c", "d", "e"];
        let out = super::shuffle(&lines, 12345);
        // Same multiset of lines.
        let mut sorted = out.clone();
        sorted.sort();
        assert_eq!(sorted, vec!["a", "b", "c", "d", "e"]);
        // Deterministic for a given seed.
        assert_eq!(super::shuffle(&lines, 12345), out);
    }

    #[test]
    fn line_transforms_respect_selection() {
        let mut e = ed("z\na\nkeep\n", 0);
        e.set_selection_range(0, 4); // covers "z","a"
        e.sort_lines();
        assert_eq!(content(&e), "a\nz\nkeep\n");
    }

    #[test]
    fn shuffle_lines_permutes_the_buffer_in_one_undo_step() {
        let src = (0..30).fold(String::new(), |acc, i| acc + &format!("line {i:02}\n"));
        let mut e = ed(&src, 0);
        e.shuffle_lines();
        let out = content(&e);
        let mut got: Vec<&str> = out.lines().collect();
        got.sort_unstable();
        let mut want: Vec<&str> = src.lines().collect();
        want.sort_unstable();
        assert_eq!(got, want, "same lines, new order");
        assert!(out.ends_with('\n'), "final newline kept");
        assert_ne!(out, src, "30 lines staying in place is a 1-in-30! chance");
        e.undo();
        assert_eq!(content(&e), src);
    }

    #[test]
    fn shuffle_with_a_zero_seed_still_moves_lines() {
        // xorshift's all-zero fixed point is avoided by substituting a seed.
        let lines: Vec<&str> = (0..20).map(|_| "x").collect();
        let distinct = ["a", "b", "c", "d", "e", "f", "g", "h"];
        let out = super::shuffle(&distinct, 0);
        assert_ne!(out, distinct.to_vec());
        assert_eq!(super::shuffle(&distinct, 0), out, "deterministic");
        assert_eq!(super::shuffle(&lines, 0).len(), 20);
        assert_eq!(super::shuffle(&[], 7), Vec::<String>::new());
        assert_eq!(super::shuffle(&["only"], 7), vec!["only"]);
    }

    #[test]
    fn sort_lines_with_a_one_line_selection_does_nothing() {
        let mut e = ed("b\na\nc", 0);
        e.set_selection_range(0, 1); // inside the first line only
        e.sort_lines();
        assert_eq!(content(&e), "b\na\nc");
        e.undo();
        assert_eq!(content(&e), "b\na\nc", "nothing was recorded");
    }

    #[test]
    fn move_line_in_a_one_line_buffer_does_nothing() {
        let mut e = ed("only", 2);
        e.move_line_up();
        e.move_line_down();
        assert_eq!((content(&e).as_str(), e.get_cursor()), ("only", 2));
    }

    #[test]
    fn range_transforms_on_an_empty_buffer_do_nothing() {
        let mut e = ed("", 0);
        e.reverse_lines();
        e.sort_unique();
        e.trim_trailing_whitespace();
        e.remove_duplicate_lines();
        e.shuffle_lines();
        e.join_lines();
        e.sort_lines();
        assert_eq!(content(&e), "");
    }

    #[test]
    fn a_selection_ending_at_a_line_start_excludes_that_line() {
        // "b\na\n": selecting exactly the first line (0..2) must leave line 1
        // untouched even though the selection ends where line 1 starts.
        let mut e = ed("c\nb\na\n", 0);
        e.set_selection_range(0, 4); // "c\nb\n" ends at the start of "a"
        e.reverse_lines();
        assert_eq!(content(&e), "b\nc\na\n");
    }

    #[test]
    fn joining_skips_the_separator_around_blank_lines() {
        // A blank second line adds nothing and no space; a blank first line
        // lets the next line start the merged text without a leading space.
        let mut e = ed("a\n\nb\n", 0);
        e.set_selection_range(0, 5);
        e.join_lines();
        assert_eq!(content(&e), "a b\n");

        let mut e = ed("\n  b\n", 0);
        e.join_lines();
        assert_eq!(content(&e), "b\n");
    }

    #[test]
    fn joining_on_the_last_line_has_nothing_below_to_join() {
        let mut e = ed("a\nb", 3);
        e.join_lines();
        assert_eq!(content(&e), "a\nb");
    }
}
