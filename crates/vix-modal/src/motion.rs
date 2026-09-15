//! Pure motions (`spec/index.md`, § Design: motions): `fn(text, pos, count)
//! -> usize`, operating on **character offsets**, not `(line, col)` and not
//! byte offsets — matching `vix-find-panel`'s and `vix-script`'s convention.
//! None of these touch an `Editor`; they're the composable building blocks
//! operators (T114) and dot-repeat (T115) need instead of a hardcoded
//! `self.editor.cursor_*()` call per key, per the T111 audit's finding that
//! "operators don't compose" precisely because motions weren't functions.
//!
//! v1 set, exactly `tasks.md`'s T113 list plus `(`/`)` (sentence motions —
//! the fuller § Design: motions list includes them alongside `{`/`}`, a
//! natural pairing at near-zero extra cost once paragraph motions exist;
//! `tasks.md`'s own terse bullet just doesn't spell them out). `%` (matching
//! bracket) is left for a small follow-on: it's a different kind of scan
//! (delimiter matching, not char/word/line position) than everything else
//! here, and the existing `edit.match_bracket` action already covers it via
//! the old table's fallthrough — not a gap this slice needs to close.

use vix_textops::{line_ranges, paragraph_units, sentence_units, word_units};

fn chars_of(text: &str) -> Vec<char> {
    text.chars().collect()
}

/// The `(start, end)` char range of the line holding `pos` — `end` is the
/// newline's own position (or `chars.len()` on a final line with no trailing
/// newline), i.e. one past the last content char, matching
/// [`vix_textops::line_ranges`].
fn current_line(chars: &[char], pos: usize) -> (usize, usize) {
    let lines = line_ranges(chars);
    lines
        .into_iter()
        .find(|&(s, e)| pos >= s && pos <= e)
        .unwrap_or((0, chars.len()))
}

/// The last valid Normal-mode cursor column on a `(start, end)` line: the
/// last character, or `start` itself on an empty line — Normal mode never
/// rests the cursor "after" the last character (unlike Insert mode).
fn last_content_pos((start, end): (usize, usize)) -> usize {
    if end > start { end - 1 } else { start }
}

fn first_non_blank(chars: &[char], (start, end): (usize, usize)) -> usize {
    (start..end)
        .find(|&i| !chars[i].is_whitespace())
        .unwrap_or(start)
}

fn line_and_col(chars: &[char], pos: usize) -> (usize, usize, Vec<(usize, usize)>) {
    let lines = line_ranges(chars);
    let row = lines
        .iter()
        .position(|&(s, e)| pos >= s && pos <= e)
        .unwrap_or(lines.len().saturating_sub(1));
    let col = pos - lines[row].0;
    (row, col, lines)
}

fn clamp_to_row(lines: &[(usize, usize)], row: usize, col: usize) -> usize {
    let (start, end) = lines[row];
    start + col.min(last_content_pos((start, end)) - start)
}

// ----- h j k l -------------------------------------------------------------

/// `h` / Left: `count` characters left, clamped to the current line's start
/// (Vim's `h` does not cross into the previous line).
#[must_use]
pub fn char_left(text: &str, pos: usize, count: usize) -> usize {
    let chars = chars_of(text);
    let (start, _) = current_line(&chars, pos);
    pos.saturating_sub(count).max(start)
}

/// `l` / Right: `count` characters right, clamped to the current line's last
/// character (Vim's `l` does not cross into the next line).
#[must_use]
pub fn char_right(text: &str, pos: usize, count: usize) -> usize {
    let chars = chars_of(text);
    let line = current_line(&chars, pos);
    (pos + count).min(last_content_pos(line))
}

/// `j` / Down: `count` lines down, keeping the current column where the
/// target line is long enough, else clamped to that line's last character.
/// v1 has no "sticky column" memory across a run of `j`/`k` — each step
/// re-reads the column from `pos` itself, a deliberate simplification (not
/// in the spec's own cut list, but the spec's `fn(text, pos, count)` motion
/// shape has nowhere to carry that memory without a 5th parameter).
#[must_use]
pub fn line_down(text: &str, pos: usize, count: usize) -> usize {
    let chars = chars_of(text);
    let (row, col, lines) = line_and_col(&chars, pos);
    let target = (row + count).min(lines.len().saturating_sub(1));
    clamp_to_row(&lines, target, col)
}

/// `k` / Up: `count` lines up, same column-clamping rule as [`line_down`].
#[must_use]
pub fn line_up(text: &str, pos: usize, count: usize) -> usize {
    let chars = chars_of(text);
    let (row, col, lines) = line_and_col(&chars, pos);
    let target = row.saturating_sub(count);
    clamp_to_row(&lines, target, col)
}

// ----- 0 ^ $ -----------------------------------------------------------

/// `0`: column 0 of the current line. Real Vim's `0` never takes a count
/// (any digit typed first is the start of a count for a *later* key, not an
/// argument to `0` itself), so this takes none either.
#[must_use]
pub fn line_start(text: &str, pos: usize) -> usize {
    let chars = chars_of(text);
    current_line(&chars, pos).0
}

/// `^`: the first non-blank character of the current line (column 0 itself
/// when the line is blank). Countless, like [`line_start`].
#[must_use]
pub fn line_first_non_blank(text: &str, pos: usize) -> usize {
    let chars = chars_of(text);
    let line = current_line(&chars, pos);
    first_non_blank(&chars, line)
}

/// `$`: the last character of the current line; `{count}$` first moves down
/// `count - 1` lines, then to that line's end (real Vim's own `$` count
/// behavior — "the end of the count'th line from here").
#[must_use]
pub fn line_end(text: &str, pos: usize, count: usize) -> usize {
    let moved = line_down(text, pos, count.saturating_sub(1));
    let chars = chars_of(text);
    last_content_pos(current_line(&chars, moved))
}

// ----- gg G ----------------------------------------------------------------

/// `gg`/`G`: go to the first non-blank character of `line` (1-indexed,
/// clamped to the buffer), or the last line when `line` is `None` — the
/// shape `G` needs (no count = last line) that `gg` doesn't (no count =
/// line 1, so callers pass `Some(count.value())` for `gg` unconditionally,
/// and `Some(count.value())`/`None` for `G` depending on
/// [`crate::Count::is_empty`]).
#[must_use]
pub fn goto_line(text: &str, line: Option<usize>) -> usize {
    let chars = chars_of(text);
    let lines = line_ranges(&chars);
    let row = match line {
        Some(n) => n.saturating_sub(1).min(lines.len().saturating_sub(1)),
        None => lines.len().saturating_sub(1),
    };
    first_non_blank(&chars, lines[row])
}

// ----- w b e -----------------------------------------------------------

/// `w`: the start of the `count`-th next word (lands at the buffer end once
/// there are no more words) — [`vix_textops::word_units`]'s lowercase-only
/// `word` definition, per the spec's v1 scope decision.
#[must_use]
pub fn word_forward(text: &str, pos: usize, count: usize) -> usize {
    let chars = chars_of(text);
    let units = word_units(&chars);
    let mut p = pos;
    for _ in 0..count.max(1) {
        p = units
            .iter()
            .map(|&(s, _)| s)
            .find(|&s| s > p)
            .unwrap_or(chars.len());
    }
    p
}

/// `b`: the start of the `count`-th previous word (lands at the buffer start
/// once there are no more).
#[must_use]
pub fn word_backward(text: &str, pos: usize, count: usize) -> usize {
    let chars = chars_of(text);
    let units = word_units(&chars);
    let mut p = pos;
    for _ in 0..count.max(1) {
        p = units
            .iter()
            .rev()
            .map(|&(s, _)| s)
            .find(|&s| s < p)
            .unwrap_or(0);
    }
    p
}

/// `e`: the end (last char) of the `count`-th next word.
#[must_use]
pub fn word_end(text: &str, pos: usize, count: usize) -> usize {
    let chars = chars_of(text);
    let units = word_units(&chars);
    let mut p = pos;
    for _ in 0..count.max(1) {
        p = units
            .iter()
            .map(|&(_, e)| e.saturating_sub(1))
            .find(|&end| end > p)
            .unwrap_or(chars.len().saturating_sub(1));
    }
    p
}

// ----- { } ( ) -----------------------------------------------------------

fn nth_start_after(units: &[(usize, usize)], pos: usize, count: usize, end: usize) -> usize {
    let mut p = pos;
    for _ in 0..count.max(1) {
        p = units
            .iter()
            .map(|&(s, _)| s)
            .find(|&s| s > p)
            .unwrap_or(end);
    }
    p
}

fn nth_start_before(units: &[(usize, usize)], pos: usize, count: usize) -> usize {
    let mut p = pos;
    for _ in 0..count.max(1) {
        p = units
            .iter()
            .rev()
            .map(|&(s, _)| s)
            .find(|&s| s < p)
            .unwrap_or(0);
    }
    p
}

/// `}`: the start of the `count`-th next paragraph (a run of non-blank
/// lines, [`vix_textops::paragraph_units`]'s definition — the same one
/// `dap`/Go → Paragraph already use).
#[must_use]
pub fn paragraph_forward(text: &str, pos: usize, count: usize) -> usize {
    let chars = chars_of(text);
    let units = paragraph_units(&chars);
    nth_start_after(&units, pos, count, chars.len())
}

/// `{`: the start of the `count`-th previous paragraph.
#[must_use]
pub fn paragraph_backward(text: &str, pos: usize, count: usize) -> usize {
    let chars = chars_of(text);
    let units = paragraph_units(&chars);
    nth_start_before(&units, pos, count)
}

/// `)`: the start of the `count`-th next sentence
/// ([`vix_textops::sentence_units`]'s definition).
#[must_use]
pub fn sentence_forward(text: &str, pos: usize, count: usize) -> usize {
    let chars = chars_of(text);
    let units = sentence_units(text, &chars);
    nth_start_after(&units, pos, count, chars.len())
}

/// `(`: the start of the `count`-th previous sentence.
#[must_use]
pub fn sentence_backward(text: &str, pos: usize, count: usize) -> usize {
    let chars = chars_of(text);
    let units = sentence_units(text, &chars);
    nth_start_before(&units, pos, count)
}

// ----- f t F T ---------------------------------------------------------

/// `f{char}`: the `count`-th occurrence of `target` after `pos`, searching
/// only the current line (Vim's `f` never crosses lines). `None` when there
/// aren't that many.
#[must_use]
pub fn find_char_forward(text: &str, pos: usize, target: char, count: usize) -> Option<usize> {
    let chars = chars_of(text);
    let (_, end) = current_line(&chars, pos);
    let mut found = pos;
    for _ in 0..count.max(1) {
        found = (found + 1..end).find(|&i| chars[i] == target)?;
    }
    Some(found)
}

/// `F{char}`: the `count`-th occurrence of `target` before `pos`, current
/// line only.
#[must_use]
pub fn find_char_backward(text: &str, pos: usize, target: char, count: usize) -> Option<usize> {
    let chars = chars_of(text);
    let (start, _) = current_line(&chars, pos);
    let mut found = pos;
    for _ in 0..count.max(1) {
        found = (start..found).rev().find(|&i| chars[i] == target)?;
    }
    Some(found)
}

/// `t{char}`: one char before the `count`-th occurrence of `target` after
/// `pos` (so a repeated `t` lands short of where a repeated `f` would).
#[must_use]
pub fn till_char_forward(text: &str, pos: usize, target: char, count: usize) -> Option<usize> {
    find_char_forward(text, pos, target, count).map(|i| i - 1)
}

/// `T{char}`: one char after the `count`-th occurrence of `target` before
/// `pos`.
#[must_use]
pub fn till_char_backward(text: &str, pos: usize, target: char, count: usize) -> Option<usize> {
    find_char_backward(text, pos, target, count).map(|i| i + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h_stops_at_the_line_start_without_crossing_lines() {
        let text = "ab\ncd";
        assert_eq!(char_left(text, 4, 1), 3); // within "cd"
        assert_eq!(char_left(text, 3, 5), 3, "stops at 'c', not into 'ab'");
    }

    #[test]
    fn l_stops_at_the_last_char_without_crossing_lines() {
        let text = "ab\ncd";
        assert_eq!(char_right(text, 0, 1), 1);
        assert_eq!(char_right(text, 0, 5), 1, "stops at 'b', not into 'cd'");
    }

    #[test]
    fn l_on_an_empty_line_stays_put() {
        assert_eq!(char_right("a\n\nb", 2, 3), 2);
    }

    #[test]
    fn j_and_k_keep_the_column_when_the_target_line_is_long_enough() {
        let text = "abcdef\nwxyzab\nabcdef";
        // pos 3 = 'd' on line 0.
        let down = line_down(text, 3, 1);
        assert_eq!(down, 7 + 3, "same column 3 on line 1");
    }

    #[test]
    fn j_clamps_the_column_to_a_shorter_line() {
        let text = "abcdef\nxy\nabcdef";
        let down = line_down(text, 5, 1); // col 5 on line 0
        // line 1 is "xy" (len 2), last valid col is 1.
        assert_eq!(down, 7 + 1);
    }

    #[test]
    fn k_moves_up_and_clamps_past_the_first_line() {
        let text = "ab\ncd";
        assert_eq!(line_up(text, 3, 10), 0);
    }

    #[test]
    fn count_multiplies_j() {
        let text = "a\nb\nc\nd\ne";
        assert_eq!(line_down(text, 0, 3), 6, "3 lines down lands on 'd'");
    }

    #[test]
    fn zero_goes_to_column_zero() {
        assert_eq!(line_start("  abc", 4), 0);
    }

    #[test]
    fn caret_goes_to_first_non_blank() {
        assert_eq!(line_first_non_blank("  abc", 4), 2);
    }

    #[test]
    fn caret_on_an_all_blank_line_stays_at_column_zero() {
        assert_eq!(line_first_non_blank("   \nabc", 1), 0);
    }

    #[test]
    fn dollar_goes_to_the_last_character() {
        assert_eq!(line_end("abc\nxy", 0, 1), 2);
    }

    #[test]
    fn dollar_with_a_count_moves_down_first() {
        let text = "abc\nxy\nhello";
        assert_eq!(line_end(text, 0, 3), text.len() - 1, "3$ -> end of line 3");
    }

    #[test]
    fn gg_with_no_count_argument_goes_to_line_one() {
        let text = "one\ntwo\nthree";
        assert_eq!(goto_line(text, Some(1)), 0);
    }

    #[test]
    fn gg_with_a_count_goes_to_that_line() {
        let text = "one\ntwo\nthree";
        assert_eq!(goto_line(text, Some(2)), 4);
    }

    #[test]
    fn g_uppercase_with_no_count_goes_to_the_last_line() {
        let text = "one\ntwo\nthree";
        assert_eq!(goto_line(text, None), 8);
    }

    #[test]
    fn goto_line_lands_on_the_first_non_blank() {
        assert_eq!(goto_line("  indented", Some(1)), 2);
    }

    #[test]
    fn goto_line_clamps_past_the_last_line() {
        let text = "one\ntwo";
        assert_eq!(goto_line(text, Some(99)), 4);
    }

    #[test]
    fn w_moves_to_the_next_word_start() {
        let text = "foo bar baz";
        assert_eq!(word_forward(text, 0, 1), 4);
        assert_eq!(word_forward(text, 0, 2), 8);
    }

    #[test]
    fn w_at_the_last_word_lands_at_the_buffer_end() {
        let text = "foo bar";
        assert_eq!(word_forward(text, 4, 1), 7);
    }

    #[test]
    fn b_moves_to_the_previous_word_start() {
        let text = "foo bar baz";
        assert_eq!(word_backward(text, 8, 1), 4);
        assert_eq!(word_backward(text, 8, 2), 0);
    }

    #[test]
    fn e_moves_to_the_next_word_end() {
        let text = "foo bar baz";
        assert_eq!(word_end(text, 0, 1), 2, "end of 'foo'");
        assert_eq!(word_end(text, 2, 1), 6, "already at foo's end -> bar's end");
    }

    #[test]
    fn paragraph_forward_and_backward_use_blank_line_separated_runs() {
        let text = "one\ntwo\n\nthree\nfour";
        assert_eq!(
            paragraph_forward(text, 0, 1),
            9,
            "start of the 2nd paragraph"
        );
        assert_eq!(paragraph_backward(text, 9, 1), 0);
    }

    #[test]
    fn sentence_forward_and_backward_split_on_terminators() {
        let text = "One. Two. Three.";
        assert_eq!(sentence_forward(text, 0, 1), 5, "start of 'Two.'");
        assert_eq!(sentence_backward(text, 10, 1), 5);
    }

    #[test]
    fn f_finds_the_nth_occurrence_on_the_current_line_only() {
        let text = "a.b.c\nd.e";
        assert_eq!(find_char_forward(text, 0, '.', 1), Some(1));
        assert_eq!(find_char_forward(text, 0, '.', 2), Some(3));
        assert_eq!(
            find_char_forward(text, 0, '.', 3),
            None,
            "no 3rd '.' on this line"
        );
    }

    #[test]
    fn capital_f_searches_backward_on_the_current_line() {
        let text = "a.b.c";
        assert_eq!(find_char_backward(text, 4, '.', 1), Some(3));
        assert_eq!(find_char_backward(text, 4, '.', 2), Some(1));
    }

    #[test]
    fn t_lands_one_short_of_the_target() {
        let text = "a.b.c";
        assert_eq!(till_char_forward(text, 0, '.', 1), Some(0));
        assert_eq!(till_char_forward(text, 0, '.', 2), Some(2));
    }

    #[test]
    fn capital_t_lands_one_past_the_target_searching_backward() {
        let text = "a.b.c";
        assert_eq!(till_char_backward(text, 4, '.', 1), Some(4));
        assert_eq!(till_char_backward(text, 4, '.', 2), Some(2));
    }

    #[test]
    fn find_char_never_crosses_a_newline() {
        let text = "abc\n.xyz";
        assert_eq!(find_char_forward(text, 0, '.', 1), None);
    }
}
