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
//!
//! T843: every motion reads its text through [`crate::text::Text`], so the
//! host passes the editor's rope (an O(1) share) instead of a copy of the
//! whole buffer, and each motion walks outward from `pos` only as far as
//! its answer needs — the current line, the next word boundary, the lines a
//! `j`/`k`/`{`/`}` crosses. Results are exactly what the original
//! whole-text versions gave (a test keeps those as an oracle); only
//! `(`/`)` still read the whole text.

use crate::text::Text;
use vix_textops::sentence_units;

mod vi;
pub(crate) use vi::grapheme_end;
pub use vi::{
    display_column, line_down_to_column, line_up_to_column, vi_char_left, vi_char_right,
    vi_chars_end, vi_normal_cursor, vi_word_backward, vi_word_end, vi_word_forward,
    vi_word_forward_for_operator,
};

/// How an operator (T114: `d`/`c`/`y`) turns a motion's landing position
/// into the actual range it acts on — real Vim's own three categories
/// (`:help motion.txt`), since "`d{motion}` deletes the range" (§ Design:
/// operators) only reads right once each motion says which kind it is:
///
/// - [`MotionKind::Exclusive`]: the range is `[start, end)` — the landing
///   character itself is not included. Most motions.
/// - [`MotionKind::Inclusive`]: the range is `[start, end]` — the landing
///   character *is* included. `e`, `$`, `f`/`t`/`F`/`T`.
/// - [`MotionKind::Linewise`]: the range expands to cover every whole line
///   it touches, regardless of either endpoint's column. `j`, `k`, `gg`,
///   `G`.
///
/// A pure motion function itself has no opinion on this (`fn(text, pos,
/// count) -> usize` only ever returns a position) — classification is a
/// property of *which* motion was used, so callers (T114's operator
/// dispatch) pair a motion's result with the right variant from this list,
/// matched to the exact same key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionKind {
    /// `[start, end)`.
    Exclusive,
    /// `[start, end]`.
    Inclusive,
    /// Every whole line either endpoint touches.
    Linewise,
}

// ----- Line helpers -------------------------------------------------------
//
// A line here is exactly a [`vix_textops::line_ranges`] entry: `(start,
// end)` where `end` is the `\n`'s own offset (or the text's length on a
// last line without one), only `\n` breaks a line, and a final `\n` does
// not start a further, empty line. The helpers find the line around one
// offset by walking outward from it, so they cost the line's length, not
// the text's. Offset arithmetic follows the figure declarations in
// `spec/index.md` (§ Design: motions, *Offsets*): a sum past `usize` lands
// on the bound the motion clamps to anyway, a difference below zero on 0.

/// Whether `pos` lies on no line at all: past the end, or at the very end
/// of a text whose last character is `\n` (the empty "line" after it,
/// which `line_ranges` does not list).
fn off_the_lines<T: Text + ?Sized>(text: &T, pos: usize) -> bool {
    let len = text.len_chars();
    pos > len || (pos == len && pos > 0 && text.chars_before(len).next() == Some('\n'))
}

/// The start of the line holding `pos`: just after the nearest `\n`
/// before it (or 0).
fn start_of_line_at<T: Text + ?Sized>(text: &T, pos: usize) -> usize {
    let back = text.chars_before(pos).take_while(|&c| c != '\n').count();
    // `back` counts characters before `pos` (clamped to the end): never
    // more than `pos`.
    pos.min(text.len_chars()).saturating_sub(back)
}

/// The end of the line holding `pos`: the next `\n` at or after it, or
/// the text's end.
fn end_of_line_at<T: Text + ?Sized>(text: &T, pos: usize) -> usize {
    let ahead = text.chars_from(pos).take_while(|&c| c != '\n').count();
    pos.saturating_add(ahead).min(text.len_chars())
}

/// The `(start, end)` range of the line holding `pos` — `end` is the
/// newline's own position (or the text's length on a final line with no
/// trailing newline), one past the last content char. An offset on no line
/// ([`off_the_lines`]) gets `(0, len)`, the whole-text fallback the
/// original `line_ranges` search used.
fn current_line<T: Text + ?Sized>(text: &T, pos: usize) -> (usize, usize) {
    if off_the_lines(text, pos) {
        return (0, text.len_chars());
    }
    (start_of_line_at(text, pos), end_of_line_at(text, pos))
}

/// The last line's range (`(0, 0)` for an empty text).
fn last_line<T: Text + ?Sized>(text: &T) -> (usize, usize) {
    let len = text.len_chars();
    match len.checked_sub(1) {
        Some(last) if text.char_at(last) == Some('\n') => (start_of_line_at(text, last), last),
        _ => (start_of_line_at(text, len), len),
    }
}

/// The line holding `pos` for row stepping (`j`/`k`/linewise ranges):
/// like [`current_line`], but an offset on no line belongs to the last
/// line, as the original row search's fallback had it.
fn row_at<T: Text + ?Sized>(text: &T, pos: usize) -> (usize, usize) {
    if off_the_lines(text, pos) {
        last_line(text)
    } else {
        current_line(text, pos)
    }
}

/// The line after `(start, end)`, if there is one.
fn next_line<T: Text + ?Sized>(text: &T, (_, end): (usize, usize)) -> Option<(usize, usize)> {
    let next = end.checked_add(1)?;
    (next < text.len_chars()).then(|| (next, end_of_line_at(text, next)))
}

/// The line before `(start, end)`, if there is one.
fn prev_line<T: Text + ?Sized>(text: &T, (start, _): (usize, usize)) -> Option<(usize, usize)> {
    let end = start.checked_sub(1)?;
    Some((start_of_line_at(text, end), end))
}

/// The last valid Normal-mode cursor column on a `(start, end)` line: the
/// last character, or `start` itself on an empty line — Normal mode never
/// rests the cursor "after" the last character (unlike Insert mode).
fn last_content_pos((start, end): (usize, usize)) -> usize {
    if end > start {
        end.checked_sub(1).unwrap_or(start)
    } else {
        start
    }
}

/// Offset of the first non-whitespace char of the `(start, end)` line, or
/// `start` for a blank line (Vim's `^` rule).
fn first_non_blank<T: Text + ?Sized>(text: &T, (start, end): (usize, usize)) -> usize {
    text.chars_from(start)
        .take(end.saturating_sub(start))
        .position(|c| !c.is_whitespace())
        .map_or(start, |i| start.checked_add(i).unwrap_or(start))
}

/// Whether the `(start, end)` line is empty or whitespace only.
fn line_is_blank<T: Text + ?Sized>(text: &T, (start, end): (usize, usize)) -> bool {
    text.chars_from(start)
        .take(end.saturating_sub(start))
        .all(char::is_whitespace)
}

/// The offset at column `col` of `line`, clamped to the line's last
/// content char (Normal mode never rests past it; see [`last_content_pos`]).
fn clamp_to_line(line: (usize, usize), col: usize) -> usize {
    let last = last_content_pos(line);
    line.0.checked_add(col).map_or(last, |p| p.min(last))
}

/// `pos`'s row (see [`row_at`]) and its column on that row.
fn row_and_col<T: Text + ?Sized>(text: &T, pos: usize) -> ((usize, usize), usize) {
    let row = row_at(text, pos);
    (row, pos.saturating_sub(row.0))
}

// ----- h j k l -------------------------------------------------------------

/// `h` / Left: `count` characters left, clamped to the current line's start
/// (Vim's `h` does not cross into the previous line).
#[must_use]
pub fn char_left(text: &(impl Text + ?Sized), pos: usize, count: usize) -> usize {
    let (start, _) = current_line(text, pos);
    pos.saturating_sub(count).max(start)
}

/// `l` / Right: `count` characters right, clamped to the current line's last
/// character (Vim's `l` does not cross into the next line).
#[must_use]
pub fn char_right(text: &(impl Text + ?Sized), pos: usize, count: usize) -> usize {
    let line = current_line(text, pos);
    let last = last_content_pos(line);
    pos.checked_add(count).map_or(last, |p| p.min(last))
}

/// `j` / Down: `count` lines down, keeping the current column where the
/// target line is long enough, else clamped to that line's last character.
/// v1 has no "sticky column" memory across a run of `j`/`k` — each step
/// re-reads the column from `pos` itself, a deliberate simplification (not
/// in the spec's own cut list, but the spec's `fn(text, pos, count)` motion
/// shape has nowhere to carry that memory without a 5th parameter). The Vi
/// keymap's `j` carries it: [`line_down_to_column`] (T838).
#[must_use]
pub fn line_down(text: &(impl Text + ?Sized), pos: usize, count: usize) -> usize {
    let (mut row, col) = row_and_col(text, pos);
    for _ in 0..count {
        match next_line(text, row) {
            Some(next) => row = next,
            None => break,
        }
    }
    clamp_to_line(row, col)
}

/// `k` / Up: `count` lines up, same column-clamping rule as [`line_down`].
#[must_use]
pub fn line_up(text: &(impl Text + ?Sized), pos: usize, count: usize) -> usize {
    let (mut row, col) = row_and_col(text, pos);
    for _ in 0..count {
        match prev_line(text, row) {
            Some(prev) => row = prev,
            None => break,
        }
    }
    clamp_to_line(row, col)
}

// ----- 0 ^ $ -----------------------------------------------------------

/// `0`: column 0 of the current line. Real Vim's `0` never takes a count
/// (any digit typed first is the start of a count for a *later* key, not an
/// argument to `0` itself), so this takes none either.
#[must_use]
pub fn line_start(text: &(impl Text + ?Sized), pos: usize) -> usize {
    current_line(text, pos).0
}

/// `^`: the first non-blank character of the current line (column 0 itself
/// when the line is blank). Countless, like [`line_start`].
#[must_use]
pub fn line_first_non_blank(text: &(impl Text + ?Sized), pos: usize) -> usize {
    first_non_blank(text, current_line(text, pos))
}

/// `$`: the last character of the current line; `{count}$` first moves down
/// `count - 1` lines, then to that line's end (real Vim's own `$` count
/// behavior — "the end of the count'th line from here").
#[must_use]
pub fn line_end(text: &(impl Text + ?Sized), pos: usize, count: usize) -> usize {
    let moved = line_down(text, pos, count.saturating_sub(1));
    last_content_pos(current_line(text, moved))
}

// ----- gg G ----------------------------------------------------------------

/// `gg`/`G`: go to the first non-blank character of `line` (1-indexed,
/// clamped to the buffer), or the last line when `line` is `None` — the
/// shape `G` needs (no count = last line) that `gg` doesn't (no count =
/// line 1, so callers pass `Some(count.value())` for `gg` unconditionally,
/// and `Some(count.value())`/`None` for `G` depending on
/// [`crate::Count::is_empty`]). `G` reads only the last line; `{n}gg`
/// walks the first `n` lines.
#[must_use]
pub fn goto_line(text: &(impl Text + ?Sized), line: Option<usize>) -> usize {
    let row = match line {
        Some(n) => {
            let mut row = (0, end_of_line_at(text, 0));
            for _ in 1..n {
                match next_line(text, row) {
                    Some(next) => row = next,
                    None => break,
                }
            }
            row
        }
        None => last_line(text),
    };
    first_non_blank(text, row)
}

// ----- w b e -----------------------------------------------------------

/// A `word` character, [`vix_textops::word_units`]'s definition.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The first word start strictly after `pos`: a word char whose
/// predecessor is not one.
fn next_word_start<T: Text + ?Sized>(text: &T, pos: usize) -> Option<usize> {
    let mut chars = text.chars_from(pos);
    let mut prev = chars.next()?;
    let mut at = pos;
    for c in chars {
        at = at.checked_add(1)?;
        if is_word(c) && !is_word(prev) {
            return Some(at);
        }
        prev = c;
    }
    None
}

/// The last word start strictly before `pos`.
fn prev_word_start<T: Text + ?Sized>(text: &T, pos: usize) -> Option<usize> {
    let mut chars = text.chars_before(pos).peekable();
    let mut at = pos.min(text.len_chars());
    while let Some(c) = chars.next() {
        at = at.checked_sub(1)?;
        if is_word(c) && chars.peek().is_none_or(|&b| !is_word(b)) {
            return Some(at);
        }
    }
    None
}

/// The first word end (last char of a word) strictly after `pos`.
fn next_word_end<T: Text + ?Sized>(text: &T, pos: usize) -> Option<usize> {
    let mut at = pos.checked_add(1)?;
    let mut chars = text.chars_from(at).peekable();
    while let Some(c) = chars.next() {
        if is_word(c) && chars.peek().is_none_or(|&n| !is_word(n)) {
            return Some(at);
        }
        at = at.checked_add(1)?;
    }
    None
}

/// Repeat `step` `count` times (at least once) from `pos`, stopping early
/// once a step stops moving (every later step would land on the same
/// offset, so a huge count costs no more than reaching the bound).
fn repeat_step(pos: usize, count: usize, mut step: impl FnMut(usize) -> usize) -> usize {
    let mut p = pos;
    for _ in 0..count.max(1) {
        let next = step(p);
        if next == p {
            break;
        }
        p = next;
    }
    p
}

/// `w`: the start of the `count`-th next word (lands at the buffer end once
/// there are no more words) — [`vix_textops::word_units`]'s lowercase-only
/// `word` definition, per the spec's v1 scope decision.
#[must_use]
pub fn word_forward(text: &(impl Text + ?Sized), pos: usize, count: usize) -> usize {
    let len = text.len_chars();
    repeat_step(pos, count, |p| next_word_start(text, p).unwrap_or(len))
}

/// `b`: the start of the `count`-th previous word (lands at the buffer start
/// once there are no more).
#[must_use]
pub fn word_backward(text: &(impl Text + ?Sized), pos: usize, count: usize) -> usize {
    repeat_step(pos, count, |p| prev_word_start(text, p).unwrap_or(0))
}

/// `e`: the end (last char) of the `count`-th next word.
#[must_use]
pub fn word_end(text: &(impl Text + ?Sized), pos: usize, count: usize) -> usize {
    let last = text.len_chars().saturating_sub(1);
    repeat_step(pos, count, |p| next_word_end(text, p).unwrap_or(last))
}

// ----- { } ( ) -----------------------------------------------------------

/// Whether `line` starts a paragraph: it is not blank, and it is the first
/// line or follows a blank one ([`vix_textops::paragraph_units`]'s runs of
/// non-blank lines).
fn starts_paragraph<T: Text + ?Sized>(text: &T, line: (usize, usize)) -> bool {
    !line_is_blank(text, line) && prev_line(text, line).is_none_or(|prev| line_is_blank(text, prev))
}

/// The first paragraph start strictly after `pos`, walking down from its
/// line.
fn next_paragraph_start<T: Text + ?Sized>(text: &T, pos: usize) -> Option<usize> {
    let mut line = row_at(text, pos);
    let mut prev_blank = line_is_blank(text, line);
    loop {
        line = next_line(text, line)?;
        let blank = line_is_blank(text, line);
        if prev_blank && !blank && line.0 > pos {
            return Some(line.0);
        }
        prev_blank = blank;
    }
}

/// The last paragraph start strictly before `pos`, walking up from its
/// line.
fn prev_paragraph_start<T: Text + ?Sized>(text: &T, pos: usize) -> Option<usize> {
    let mut line = row_at(text, pos);
    loop {
        if line.0 < pos && starts_paragraph(text, line) {
            return Some(line.0);
        }
        line = prev_line(text, line)?;
    }
}

/// `}`: the start of the `count`-th next paragraph (a run of non-blank
/// lines, [`vix_textops::paragraph_units`]'s definition — the same one
/// `dap`/Go → Paragraph already use).
#[must_use]
pub fn paragraph_forward(text: &(impl Text + ?Sized), pos: usize, count: usize) -> usize {
    let len = text.len_chars();
    repeat_step(pos, count, |p| next_paragraph_start(text, p).unwrap_or(len))
}

/// `{`: the start of the `count`-th previous paragraph.
#[must_use]
pub fn paragraph_backward(text: &(impl Text + ?Sized), pos: usize, count: usize) -> usize {
    repeat_step(pos, count, |p| prev_paragraph_start(text, p).unwrap_or(0))
}

/// Walk forward `count` times (at least once) to the next unit start strictly
/// after the running position; falls back to `end` when no unit remains.
fn nth_start_after(units: &[(usize, usize)], pos: usize, count: usize, end: usize) -> usize {
    repeat_step(pos, count, |p| {
        units
            .iter()
            .map(|&(s, _)| s)
            .find(|&s| s > p)
            .unwrap_or(end)
    })
}

/// Walk backward `count` times (at least once) to the previous unit start
/// strictly before the running position; falls back to `0`.
fn nth_start_before(units: &[(usize, usize)], pos: usize, count: usize) -> usize {
    repeat_step(pos, count, |p| {
        units
            .iter()
            .rev()
            .map(|&(s, _)| s)
            .find(|&s| s < p)
            .unwrap_or(0)
    })
}

/// `)`: the start of the `count`-th next sentence
/// ([`vix_textops::sentence_units`]'s definition). Still reads the whole
/// text: a sentence boundary depends on rules (abbreviations, closing
/// quotes) only that whole-text scan applies.
#[must_use]
pub fn sentence_forward(text: &(impl Text + ?Sized), pos: usize, count: usize) -> usize {
    let text = text.whole_string();
    let chars: Vec<char> = text.chars().collect();
    let units = sentence_units(&text, &chars);
    nth_start_after(&units, pos, count, chars.len())
}

/// `(`: the start of the `count`-th previous sentence.
#[must_use]
pub fn sentence_backward(text: &(impl Text + ?Sized), pos: usize, count: usize) -> usize {
    let text = text.whole_string();
    let chars: Vec<char> = text.chars().collect();
    let units = sentence_units(&text, &chars);
    nth_start_before(&units, pos, count)
}

// ----- f t F T ---------------------------------------------------------

/// `f{char}`: the `count`-th occurrence of `target` after `pos`, searching
/// only the current line (Vim's `f` never crosses lines). `None` when there
/// aren't that many.
#[must_use]
pub fn find_char_forward(
    text: &(impl Text + ?Sized),
    pos: usize,
    target: char,
    count: usize,
) -> Option<usize> {
    let (_, end) = current_line(text, pos);
    let mut found = pos;
    for _ in 0..count.max(1) {
        let from = found.checked_add(1)?;
        let span = end.checked_sub(from)?;
        let k = text.chars_from(from).take(span).position(|c| c == target)?;
        found = from.checked_add(k)?;
    }
    Some(found)
}

/// `F{char}`: the `count`-th occurrence of `target` before `pos`, current
/// line only.
#[must_use]
pub fn find_char_backward(
    text: &(impl Text + ?Sized),
    pos: usize,
    target: char,
    count: usize,
) -> Option<usize> {
    let (start, _) = current_line(text, pos);
    let mut found = pos.min(text.len_chars());
    for _ in 0..count.max(1) {
        let span = found.checked_sub(start)?;
        let k = text
            .chars_before(found)
            .take(span)
            .position(|c| c == target)?;
        found = found.checked_sub(k)?.checked_sub(1)?;
    }
    Some(found)
}

/// `t{char}`: one char before the `count`-th occurrence of `target` after
/// `pos` (so a repeated `t` lands short of where a repeated `f` would).
#[must_use]
pub fn till_char_forward(
    text: &(impl Text + ?Sized),
    pos: usize,
    target: char,
    count: usize,
) -> Option<usize> {
    // A hit is always after `pos`, so it is at least 1.
    find_char_forward(text, pos, target, count).and_then(|i| i.checked_sub(1))
}

/// `T{char}`: one char after the `count`-th occurrence of `target` before
/// `pos`.
#[must_use]
pub fn till_char_backward(
    text: &(impl Text + ?Sized),
    pos: usize,
    target: char,
    count: usize,
) -> Option<usize> {
    find_char_backward(text, pos, target, count).and_then(|i| i.checked_add(1))
}

/// The `(start, end)` range a linewise operator covers between offsets
/// `lo` and `hi` (`lo <= hi`): from the start of `lo`'s line to the start
/// of the line after `hi`'s (or the text's end) — read from those two lines
/// only. See [`crate::operator::operator_range`].
#[must_use]
pub fn linewise_span(text: &(impl Text + ?Sized), lo: usize, hi: usize) -> (usize, usize) {
    let start = row_at(text, lo).0;
    let end = next_line(text, row_at(text, hi)).map_or(text.len_chars(), |(s, _)| s);
    (start, end)
}

/// The motions exactly as they were before T843 (whole-text `Vec<char>`
/// decoding plus `vix_textops` unit lists), kept as an oracle: the
/// line-local rewrite above must agree with them everywhere.
#[cfg(test)]
mod reference {
    use vix_textops::{line_ranges, paragraph_units, sentence_units, word_units};

    pub(super) fn operator_range(
        text: &str,
        before: usize,
        after: usize,
        kind: crate::motion::MotionKind,
    ) -> (usize, usize, crate::register::RegisterKind) {
        let (lo, hi) = if before <= after {
            (before, after)
        } else {
            (after, before)
        };
        match kind {
            crate::motion::MotionKind::Exclusive => (lo, hi, crate::register::RegisterKind::Char),
            crate::motion::MotionKind::Inclusive => {
                let n = text.chars().count();
                (lo, (hi + 1).min(n), crate::register::RegisterKind::Char)
            }
            crate::motion::MotionKind::Linewise => {
                let chars: Vec<char> = text.chars().collect();
                let lines = line_ranges(&chars);
                let row_of = |p: usize| {
                    lines
                        .iter()
                        .position(|&(s, e)| p >= s && p <= e)
                        .unwrap_or(lines.len().saturating_sub(1))
                };
                let start_row = row_of(lo);
                let end_row = row_of(hi);
                let start = lines[start_row].0;
                let end = if end_row + 1 < lines.len() {
                    lines[end_row + 1].0
                } else {
                    chars.len()
                };
                (start, end, crate::register::RegisterKind::Line)
            }
        }
    }

    fn chars_of(text: &str) -> Vec<char> {
        text.chars().collect()
    }

    fn current_line(chars: &[char], pos: usize) -> (usize, usize) {
        let lines = line_ranges(chars);
        lines
            .into_iter()
            .find(|&(s, e)| pos >= s && pos <= e)
            .unwrap_or((0, chars.len()))
    }

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

    pub(super) fn char_left(text: &str, pos: usize, count: usize) -> usize {
        let chars = chars_of(text);
        let (start, _) = current_line(&chars, pos);
        pos.saturating_sub(count).max(start)
    }

    pub(super) fn char_right(text: &str, pos: usize, count: usize) -> usize {
        let chars = chars_of(text);
        let line = current_line(&chars, pos);
        (pos + count).min(last_content_pos(line))
    }

    pub(super) fn line_down(text: &str, pos: usize, count: usize) -> usize {
        let chars = chars_of(text);
        let (row, col, lines) = line_and_col(&chars, pos);
        let target = (row + count).min(lines.len().saturating_sub(1));
        clamp_to_row(&lines, target, col)
    }

    pub(super) fn line_up(text: &str, pos: usize, count: usize) -> usize {
        let chars = chars_of(text);
        let (row, col, lines) = line_and_col(&chars, pos);
        let target = row.saturating_sub(count);
        clamp_to_row(&lines, target, col)
    }

    pub(super) fn line_start(text: &str, pos: usize) -> usize {
        let chars = chars_of(text);
        current_line(&chars, pos).0
    }

    pub(super) fn line_first_non_blank(text: &str, pos: usize) -> usize {
        let chars = chars_of(text);
        let line = current_line(&chars, pos);
        first_non_blank(&chars, line)
    }

    pub(super) fn line_end(text: &str, pos: usize, count: usize) -> usize {
        let moved = line_down(text, pos, count.saturating_sub(1));
        let chars = chars_of(text);
        last_content_pos(current_line(&chars, moved))
    }

    pub(super) fn goto_line(text: &str, line: Option<usize>) -> usize {
        let chars = chars_of(text);
        let lines = line_ranges(&chars);
        let row = match line {
            Some(n) => n.saturating_sub(1).min(lines.len().saturating_sub(1)),
            None => lines.len().saturating_sub(1),
        };
        first_non_blank(&chars, lines[row])
    }

    pub(super) fn word_forward(text: &str, pos: usize, count: usize) -> usize {
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

    pub(super) fn word_backward(text: &str, pos: usize, count: usize) -> usize {
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

    pub(super) fn word_end(text: &str, pos: usize, count: usize) -> usize {
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

    pub(super) fn paragraph_forward(text: &str, pos: usize, count: usize) -> usize {
        let chars = chars_of(text);
        let units = paragraph_units(&chars);
        nth_start_after(&units, pos, count, chars.len())
    }

    pub(super) fn paragraph_backward(text: &str, pos: usize, count: usize) -> usize {
        let chars = chars_of(text);
        let units = paragraph_units(&chars);
        nth_start_before(&units, pos, count)
    }

    pub(super) fn sentence_forward(text: &str, pos: usize, count: usize) -> usize {
        let chars = chars_of(text);
        let units = sentence_units(text, &chars);
        nth_start_after(&units, pos, count, chars.len())
    }

    pub(super) fn sentence_backward(text: &str, pos: usize, count: usize) -> usize {
        let chars = chars_of(text);
        let units = sentence_units(text, &chars);
        nth_start_before(&units, pos, count)
    }

    pub(super) fn find_char_forward(
        text: &str,
        pos: usize,
        target: char,
        count: usize,
    ) -> Option<usize> {
        let chars = chars_of(text);
        let (_, end) = current_line(&chars, pos);
        let mut found = pos;
        for _ in 0..count.max(1) {
            found = (found + 1..end).find(|&i| chars[i] == target)?;
        }
        Some(found)
    }

    pub(super) fn find_char_backward(
        text: &str,
        pos: usize,
        target: char,
        count: usize,
    ) -> Option<usize> {
        let chars = chars_of(text);
        let (start, _) = current_line(&chars, pos);
        let mut found = pos;
        for _ in 0..count.max(1) {
            found = (start..found).rev().find(|&i| chars[i] == target)?;
        }
        Some(found)
    }

    pub(super) fn till_char_forward(
        text: &str,
        pos: usize,
        target: char,
        count: usize,
    ) -> Option<usize> {
        find_char_forward(text, pos, target, count).map(|i| i - 1)
    }

    pub(super) fn till_char_backward(
        text: &str,
        pos: usize,
        target: char,
        count: usize,
    ) -> Option<usize> {
        find_char_backward(text, pos, target, count).map(|i| i + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use ropey::Rope;

    /// A tiny deterministic generator (xorshift), so the agreement test
    /// covers many odd texts without a dependency.
    fn texts() -> Vec<String> {
        let alphabet = ['a', 'b', ' ', '\n', '.', 'é', '_', '\r', '\t', '!'];
        let mut out = vec![String::new()];
        // Every text of up to 4 chars over a small alphabet ...
        let small = ['a', ' ', '\n', '.', 'é'];
        let mut level = vec![String::new()];
        for _ in 0..4 {
            let mut next = Vec::new();
            for t in &level {
                for c in small {
                    let mut s = t.clone();
                    s.push(c);
                    next.push(s);
                }
            }
            out.extend(next.iter().cloned());
            level = next;
        }
        // ... and a spread of longer ones over a bigger one.
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        for _ in 0..1500 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let len = usize::try_from(state % 16).unwrap();
            let mut s = String::new();
            for _ in 0..len {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                s.push(alphabet[usize::try_from(state % 10).unwrap()]);
            }
            out.push(s);
        }
        out
    }

    /// Assert that `$new` gives `$old` (the oracle's answer) on both the
    /// `str` and the `Rope` form of the text. `$new` is pasted twice, so
    /// each copy of the closure gets its own argument type.
    macro_rules! same {
        ($s:expr, $rope:expr, $what:expr, $new:expr, $old:expr) => {{
            let old = $old;
            assert_eq!($new($s), old, "{} (str)", $what);
            assert_eq!($new($rope), old, "{} (rope)", $what);
        }};
    }

    /// Check every motion on `s` at every offset and a few counts: the
    /// `str` and `Rope` paths both agree with the pre-T843 oracle.
    fn agree_on(s: &str) {
        let rope = Rope::from_str(s);
        let len = s.chars().count();
        for pos in 0..=len {
            for count in [0, 1, 2, 3, 7] {
                agree_counted(s, &rope, pos, count);
                agree_finds(s, &rope, pos, count);
            }
            for other in 0..=len {
                for kind in [
                    MotionKind::Exclusive,
                    MotionKind::Inclusive,
                    MotionKind::Linewise,
                ] {
                    same!(
                        s,
                        &rope,
                        format!("operator_range {kind:?} on {s:?} {pos}..{other}"),
                        |t| crate::operator::operator_range(t, pos, other, kind),
                        reference::operator_range(s, pos, other, kind)
                    );
                }
            }
            same_countless(s, &rope, pos);
        }
        same!(
            s,
            &rope,
            format!("G on {s:?}"),
            |t| goto_line(t, None),
            reference::goto_line(s, None)
        );
    }

    /// The counted motions for [`agree_on`].
    fn agree_counted(s: &str, rope: &Rope, pos: usize, count: usize) {
        let at = |m: &str| format!("{m} on {s:?} at {pos} x{count}");
        same!(
            s,
            rope,
            at("h"),
            |t| char_left(t, pos, count),
            reference::char_left(s, pos, count)
        );
        same!(
            s,
            rope,
            at("l"),
            |t| char_right(t, pos, count),
            reference::char_right(s, pos, count)
        );
        same!(
            s,
            rope,
            at("j"),
            |t| line_down(t, pos, count),
            reference::line_down(s, pos, count)
        );
        same!(
            s,
            rope,
            at("k"),
            |t| line_up(t, pos, count),
            reference::line_up(s, pos, count)
        );
        same!(
            s,
            rope,
            at("$"),
            |t| line_end(t, pos, count),
            reference::line_end(s, pos, count)
        );
        same!(
            s,
            rope,
            at("w"),
            |t| word_forward(t, pos, count),
            reference::word_forward(s, pos, count)
        );
        same!(
            s,
            rope,
            at("b"),
            |t| word_backward(t, pos, count),
            reference::word_backward(s, pos, count)
        );
        same!(
            s,
            rope,
            at("e"),
            |t| word_end(t, pos, count),
            reference::word_end(s, pos, count)
        );
        same!(
            s,
            rope,
            at("}"),
            |t| paragraph_forward(t, pos, count),
            reference::paragraph_forward(s, pos, count)
        );
        same!(
            s,
            rope,
            at("{"),
            |t| paragraph_backward(t, pos, count),
            reference::paragraph_backward(s, pos, count)
        );
        same!(
            s,
            rope,
            at(")"),
            |t| sentence_forward(t, pos, count),
            reference::sentence_forward(s, pos, count)
        );
        same!(
            s,
            rope,
            at("("),
            |t| sentence_backward(t, pos, count),
            reference::sentence_backward(s, pos, count)
        );
        same!(
            s,
            rope,
            at("gg"),
            |t| goto_line(t, Some(count)),
            reference::goto_line(s, Some(count))
        );
    }

    /// `f`/`F`/`t`/`T` for [`agree_on`].
    fn agree_finds(s: &str, rope: &Rope, pos: usize, count: usize) {
        for target in ['a', '.', ' '] {
            let at = |m: &str| format!("{m}{target} on {s:?} at {pos} x{count}");
            same!(
                s,
                rope,
                at("f"),
                |t| find_char_forward(t, pos, target, count),
                reference::find_char_forward(s, pos, target, count)
            );
            same!(
                s,
                rope,
                at("F"),
                |t| find_char_backward(t, pos, target, count),
                reference::find_char_backward(s, pos, target, count)
            );
            same!(
                s,
                rope,
                at("t"),
                |t| till_char_forward(t, pos, target, count),
                reference::till_char_forward(s, pos, target, count)
            );
            same!(
                s,
                rope,
                at("T"),
                |t| till_char_backward(t, pos, target, count),
                reference::till_char_backward(s, pos, target, count)
            );
        }
    }

    /// The countless motions (`0`, `^`) for [`agree_on`].
    fn same_countless(s: &str, rope: &Rope, pos: usize) {
        assert_eq!(
            line_start(s, pos),
            reference::line_start(s, pos),
            "0 on {s:?} at {pos}"
        );
        assert_eq!(
            line_start(rope, pos),
            reference::line_start(s, pos),
            "0 on {s:?} at {pos}"
        );
        let caret = reference::line_first_non_blank(s, pos);
        assert_eq!(line_first_non_blank(s, pos), caret, "^ on {s:?} at {pos}");
        assert_eq!(
            line_first_non_blank(rope, pos),
            caret,
            "^ on {s:?} at {pos}"
        );
    }

    #[test]
    fn line_local_motions_agree_with_the_whole_text_originals() {
        for s in texts() {
            agree_on(&s);
        }
    }

    /// T843: a motion on one line gives the same answer, relative to the
    /// line, whether the line is the whole buffer or sits in the middle of
    /// a megabyte of unrelated text.
    #[test]
    fn motions_on_a_large_buffer_match_the_same_line_alone() {
        let line = "  let value = compute(alpha, beta.gamma); // x.y z\n";
        let filler = "unrelated words, more words. And more!\n\n".repeat(25_000);
        let big = format!("{filler}{line}{filler}");
        let rope = Rope::from_str(&big);
        let small = Rope::from_str(line);
        let off = filler.chars().count();
        // Content offsets strictly inside the line, so every motion below
        // stays on it.
        for col in 4..line.chars().count() - 6 {
            for count in [1, 2, 3] {
                let pairs = [
                    (
                        char_left(&rope, off + col, count),
                        char_left(&small, col, count),
                    ),
                    (
                        char_right(&rope, off + col, count),
                        char_right(&small, col, count),
                    ),
                    (line_start(&rope, off + col), line_start(&small, col)),
                    (
                        line_first_non_blank(&rope, off + col),
                        line_first_non_blank(&small, col),
                    ),
                    (line_end(&rope, off + col, 1), line_end(&small, col, 1)),
                ];
                for (i, (b, s)) in pairs.into_iter().enumerate() {
                    assert_eq!(b - off, s, "motion #{i} at column {col} x{count}");
                }
                for target in ['a', '.', ' '] {
                    let shift = |p: Option<usize>| p.map(|p| p - off);
                    assert_eq!(
                        shift(find_char_forward(&rope, off + col, target, count)),
                        find_char_forward(&small, col, target, count)
                    );
                    assert_eq!(
                        shift(find_char_backward(&rope, off + col, target, count)),
                        find_char_backward(&small, col, target, count)
                    );
                    assert_eq!(
                        shift(till_char_forward(&rope, off + col, target, count)),
                        till_char_forward(&small, col, target, count)
                    );
                    assert_eq!(
                        shift(till_char_backward(&rope, off + col, target, count)),
                        till_char_backward(&small, col, target, count)
                    );
                }
            }
            // Word motions that land inside the line agree too.
            let (w, b, e) = (
                word_forward(&small, col, 1),
                word_backward(&small, col, 1),
                word_end(&small, col, 1),
            );
            assert_eq!(word_forward(&rope, off + col, 1) - off, w, "w at {col}");
            if b > 0 {
                assert_eq!(word_backward(&rope, off + col, 1) - off, b, "b at {col}");
            }
            assert_eq!(word_end(&rope, off + col, 1) - off, e, "e at {col}");
        }
    }

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
