//! Operators (`spec/index.md`, § Design: operators): `d`/`c`/`y` turn a
//! motion's landing position (plus its [`MotionKind`]) into an actual
//! `[start, end)` char range and a [`RegisterKind`], then delete or copy
//! that range. Pure functions — the host (T114's wiring) applies the result
//! to a live buffer and register; `p`/`P` are not operators (they're
//! Normal-mode commands reading a register), so they live in the host, not
//! here.

use crate::motion::MotionKind;
use crate::register::{RegisterKind, RegisterValue};
use vix_textops::line_ranges;

/// Turn a motion's raw `(before, after)` cursor pair — `before` is where the
/// cursor was, `after` is where the motion landed, order not guaranteed
/// since a backward motion lands before the cursor — into the actual
/// half-open `[start, end)` char range an operator acts on, per `kind`, plus
/// how the extracted text should be classified for `p`/`P` (the spec's own
/// distinction, § Design: registers).
#[must_use]
pub fn operator_range(
    text: &str,
    before: usize,
    after: usize,
    kind: MotionKind,
) -> (usize, usize, RegisterKind) {
    let (lo, hi) = if before <= after {
        (before, after)
    } else {
        (after, before)
    };
    match kind {
        MotionKind::Exclusive => (lo, hi, RegisterKind::Char),
        MotionKind::Inclusive => {
            let n = text.chars().count();
            (lo, (hi + 1).min(n), RegisterKind::Char)
        }
        MotionKind::Linewise => {
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
            (start, end, RegisterKind::Line)
        }
    }
}

/// Extract `range` from `text` and remove it. Returns the rewritten text
/// and the removed slice (for the register) — `d`/`c` use both, `y` uses
/// only the removed slice and discards the rewritten text (the buffer is
/// unchanged).
#[must_use]
pub fn delete_range(text: &str, (start, end): (usize, usize)) -> (String, String) {
    let chars: Vec<char> = text.chars().collect();
    let end = end.min(chars.len());
    let start = start.min(end);
    let removed: String = chars[start..end].iter().collect();
    let mut out: String = chars[..start].iter().collect();
    out.extend(chars[end..].iter());
    (out, removed)
}

/// Insert `content` at char offset `at`. Returns the rewritten text and the
/// cursor position `p`/`P` should land on afterward — `content`'s own end,
/// matching real Vim's "cursor lands on the last inserted character" rule
/// (not just after it, except when `content` is empty).
#[must_use]
pub fn insert_at(text: &str, at: usize, content: &str) -> (String, usize) {
    let chars: Vec<char> = text.chars().collect();
    let at = at.min(chars.len());
    let mut out: String = chars[..at].iter().collect();
    out.push_str(content);
    out.extend(chars[at..].iter());
    let inserted_len = content.chars().count();
    let cursor = if inserted_len == 0 {
        at
    } else {
        at + inserted_len - 1
    };
    (out, cursor)
}

/// Offset of the first non-blank char on the line holding `pos` (the line's
/// start when it is all blank; `pos` itself when no line holds it). Used to
/// place the cursor after a line-wise paste.
fn first_non_blank_at(text: &str, pos: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let lines = line_ranges(&chars);
    let Some(&(s, e)) = lines.iter().find(|&&(s, e)| pos >= s && pos <= e) else {
        return pos;
    };
    (s..e).find(|&i| !chars[i].is_whitespace()).unwrap_or(s)
}

/// The range a Visual-mode operator (`d`/`c`/`y`, T840) acts on, and the
/// register kind it writes. `selection` is the host's half-open selection
/// (`None` or empty right after `v`), whichever way it was extended.
///
/// - Character-wise (`linewise == false`): Vim's Visual selection is
///   inclusive at both ends — the anchor character and the cursor character
///   are both part of it — while the host's selection ends just before the
///   cursor (forward) or just before the anchor (backward). Either way the
///   one character after the host's range belongs to it, so the result is
///   `[start, end + 1)`, clamped to the buffer; an empty selection is the
///   one character under `cursor`.
/// - Line-wise (`linewise == true`): every line the selection touches,
///   whole, as a [`RegisterKind::Line`] range (the same rule as a
///   line-wise motion).
#[must_use]
pub fn visual_range(
    text: &str,
    selection: Option<(usize, usize)>,
    cursor: usize,
    linewise: bool,
) -> (usize, usize, RegisterKind) {
    let (lo, hi) = match selection {
        Some((a, b)) if a != b => (a.min(b), a.max(b)),
        _ => (cursor, cursor),
    };
    if linewise {
        // `hi` is exclusive: its last character (a line break, for a
        // whole-line selection) is the last line the selection touches.
        let last = if hi > lo { hi.saturating_sub(1) } else { hi };
        operator_range(text, lo, last, MotionKind::Linewise)
    } else {
        let n = text.chars().count();
        let lo = lo.min(n);
        (lo, hi.saturating_add(1).min(n).max(lo), RegisterKind::Char)
    }
}

/// `p`/`P` (§ Design: registers — not operators, Normal-mode
/// register-paste commands): where to insert `value` relative to the
/// cursor at `pos`, what to insert, and where the cursor lands afterward.
/// A *plan*, not a splice — the host inserts `.1` at `.0` through its own
/// `InsertText` action (so paste gets the same undo/highlight handling as
/// every other edit) rather than this pure crate rewriting the buffer
/// itself.
///
/// - Character-wise (`value.kind == Char`): inline — `p` right after `pos`,
///   `P` right before it (`before` selects which).
/// - Line-wise (`value.kind == Line`): `value.text` inserted as its own
///   whole line (a trailing newline is added if `value.text` doesn't
///   already have one) — `p` below `pos`'s line, `P` above it — landing the
///   cursor on the new line's first non-blank character (real Vim's own
///   `p`/`P` rule, not just wherever the raw insert's last character
///   lands).
#[must_use]
pub fn paste_plan(
    text: &str,
    pos: usize,
    value: &RegisterValue,
    before: bool,
) -> (usize, String, usize) {
    match value.kind {
        RegisterKind::Char => {
            let n = text.chars().count();
            let at = if before { pos } else { (pos + 1).min(n) };
            let len = value.text.chars().count();
            let cursor = if len == 0 { at } else { at + len - 1 };
            (at, value.text.clone(), cursor)
        }
        RegisterKind::Line => {
            let chars: Vec<char> = text.chars().collect();
            let lines = line_ranges(&chars);
            let row = lines
                .iter()
                .position(|&(s, e)| pos >= s && pos <= e)
                .unwrap_or(lines.len().saturating_sub(1));
            let at = if before {
                lines[row].0
            } else if row + 1 < lines.len() {
                lines[row + 1].0
            } else {
                chars.len()
            };
            let mut content = value.text.clone();
            if !content.ends_with('\n') {
                content.push('\n');
            }
            // `content` alone is exactly the buffer the new line will
            // occupy, so its own first non-blank (relative offset 0) is
            // the cursor's column within it — no need to look at the
            // merged result.
            let cursor = at + first_non_blank_at(&content, 0);
            (at, content, cursor)
        }
    }
}

/// The most text one counted put may insert (64 MiB): [`paste_plan_counted`]
/// refuses a larger result.
pub const PUT_LIMIT_BYTES: usize = 64 * 1024 * 1024;

/// [`paste_plan`] with a count (`3p`, T840): `value` inserted `count`
/// times as one insertion (so one undo removes every copy). A character-wise
/// paste lands on the last inserted character; a line-wise one on the first
/// pasted line's first non-blank, as Vim does. A `count` of 0 is treated as
/// 1, the count a bare `p` has.
///
/// Returns `None`, and nothing should be inserted, when the copies would add
/// up to more than [`PUT_LIMIT_BYTES`]: a count saturates rather than
/// overflowing, so `99999999999999999999p` must be refused, as Vim refuses it
/// with E1240, instead of trying to allocate the result.
#[must_use]
pub fn paste_plan_counted(
    text: &str,
    pos: usize,
    value: &RegisterValue,
    before: bool,
    count: usize,
) -> Option<(usize, String, usize)> {
    let (at, one, cursor) = paste_plan(text, pos, value, before);
    let count = count.max(1);
    if one.len().checked_mul(count)? > PUT_LIMIT_BYTES {
        return None;
    }
    let content = one.repeat(count);
    let cursor = match value.kind {
        RegisterKind::Line => cursor,
        RegisterKind::Char => {
            let len = content.chars().count();
            if len == 0 {
                at
            } else {
                at.saturating_add(len.saturating_sub(1))
            }
        }
    };
    Some((at, content, cursor))
}

/// Vim's `cw` special case (`:help cw`): with the cursor on a non-blank,
/// `cw` changes to the end of the word instead of also taking the white
/// space after it, so `cw` on `old value` keeps the space before
/// `value`. `before` is the cursor and `after` where the `w` motion
/// landed (an exclusive end); the result is the end to change up to —
/// `after` minus the white space just before it, never before `before`'s
/// own character. A cursor on white space keeps `after` unchanged.
#[must_use]
pub fn change_word_end(text: &str, before: usize, after: usize) -> usize {
    let span: Vec<char> = text
        .chars()
        .skip(before)
        .take(after.saturating_sub(before))
        .collect();
    match span.first() {
        Some(c) if !c.is_whitespace() => {
            let trailing = span.iter().rev().take_while(|c| c.is_whitespace()).count();
            after.saturating_sub(trailing)
        }
        _ => after,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_word_end_stops_before_the_white_space_after_the_word() {
        // `w` from 0 in "old value" lands on 'v' (4); `cw` changes "old".
        assert_eq!(change_word_end("old value", 0, 4), 3);
        // Across a line break, and with a count landing two words on.
        assert_eq!(change_word_end("old\n  next", 0, 6), 3);
        assert_eq!(change_word_end("a b c", 0, 4), 3);
        // Punctuation after the word is not white space: nothing to trim.
        assert_eq!(change_word_end("foo.bar", 0, 3), 3);
        // On white space, `cw` is `dw`.
        assert_eq!(change_word_end("a  b", 1, 3), 3);
        // Multi-byte text counts characters, not bytes.
        assert_eq!(change_word_end("ñé x", 0, 3), 2);
    }

    #[test]
    fn exclusive_range_does_not_include_the_landing_char() {
        let (start, end, kind) = operator_range("hello world", 0, 6, MotionKind::Exclusive);
        assert_eq!((start, end), (0, 6));
        assert_eq!(kind, RegisterKind::Char);
    }

    #[test]
    fn inclusive_range_includes_the_landing_char() {
        let (start, end, kind) = operator_range("hello", 0, 2, MotionKind::Inclusive);
        assert_eq!((start, end), (0, 3));
        assert_eq!(kind, RegisterKind::Char);
    }

    #[test]
    fn inclusive_range_clamps_at_the_buffer_end() {
        let (start, end, _) = operator_range("hi", 0, 1, MotionKind::Inclusive);
        assert_eq!((start, end), (0, 2));
    }

    #[test]
    fn a_backward_motion_still_produces_a_forward_sorted_range() {
        // 'b' from the middle of a word lands before the cursor.
        let (start, end, _) = operator_range("foo bar", 4, 0, MotionKind::Exclusive);
        assert_eq!((start, end), (0, 4));
    }

    #[test]
    fn linewise_range_covers_whole_lines_regardless_of_column() {
        let text = "one\ntwo\nthree\n";
        // cursor mid-"one", motion landed mid-"two" (j).
        let (start, end, kind) = operator_range(text, 1, 5, MotionKind::Linewise);
        assert_eq!(&text[start..end], "one\ntwo\n");
        assert_eq!(kind, RegisterKind::Line);
    }

    #[test]
    fn linewise_range_on_the_last_line_reaches_the_buffer_end() {
        let text = "one\ntwo";
        let (start, end, _) = operator_range(text, 0, 5, MotionKind::Linewise);
        assert_eq!(&text[start..end], "one\ntwo");
    }

    #[test]
    fn delete_range_removes_the_slice_and_reports_it() {
        let (rewritten, removed) = delete_range("hello world", (0, 6));
        assert_eq!(rewritten, "world");
        assert_eq!(removed, "hello ");
    }

    #[test]
    fn delete_range_at_the_very_end_leaves_nothing_after() {
        let (rewritten, removed) = delete_range("hello", (2, 5));
        assert_eq!(rewritten, "he");
        assert_eq!(removed, "llo");
    }

    #[test]
    fn insert_at_lands_the_cursor_on_the_last_inserted_char() {
        let (rewritten, cursor) = insert_at("world", 0, "hi ");
        assert_eq!(rewritten, "hi world");
        assert_eq!(cursor, 2, "on the space, the last char of 'hi '");
    }

    #[test]
    fn insert_at_with_empty_content_leaves_the_cursor_at_the_insertion_point() {
        let (rewritten, cursor) = insert_at("world", 2, "");
        assert_eq!(rewritten, "world");
        assert_eq!(cursor, 2);
    }

    #[test]
    fn char_wise_paste_after_goes_right_after_the_cursor() {
        let value = RegisterValue {
            text: "XY".to_string(),
            kind: RegisterKind::Char,
        };
        let (at, content, cursor) = paste_plan("abc", 0, &value, false);
        assert_eq!(at, 1);
        assert_eq!(content, "XY");
        assert_eq!(cursor, 2, "on the last pasted char");
    }

    #[test]
    fn char_wise_paste_before_goes_right_at_the_cursor() {
        let value = RegisterValue {
            text: "XY".to_string(),
            kind: RegisterKind::Char,
        };
        let (at, content, cursor) = paste_plan("abc", 1, &value, true);
        assert_eq!(at, 1);
        assert_eq!(content, "XY");
        assert_eq!(cursor, 2);
    }

    #[test]
    fn line_wise_paste_after_inserts_below_and_lands_on_the_first_non_blank() {
        let value = RegisterValue {
            text: "  two".to_string(),
            kind: RegisterKind::Line,
        };
        let (at, content, cursor) = paste_plan("one\nthree\n", 0, &value, false);
        assert_eq!(at, 4, "right after 'one\\n'");
        assert_eq!(content, "  two\n", "a trailing newline is added");
        assert_eq!(cursor, 6, "the 't' of 'two', not the leading spaces");
    }

    #[test]
    fn line_wise_paste_before_inserts_above() {
        let value = RegisterValue {
            text: "two".to_string(),
            kind: RegisterKind::Line,
        };
        let (at, content, cursor) = paste_plan("one\nthree\n", 5, &value, true);
        assert_eq!(at, 4, "the start of 'three', not the cursor's own column");
        assert_eq!(content, "two\n");
        assert_eq!(cursor, 4);
    }

    #[test]
    fn visual_range_includes_the_cursor_character_forward() {
        // `vl` from 0: the host selects [0, 1), Vim covers 0..=1.
        assert_eq!(
            visual_range("hello", Some((0, 1)), 1, false),
            (0, 2, RegisterKind::Char)
        );
    }

    #[test]
    fn visual_range_includes_the_anchor_character_backward() {
        // `vhh` from 3: the host selects [1, 3), Vim covers 1..=3.
        assert_eq!(
            visual_range("abcdef", Some((1, 3)), 1, false),
            (1, 4, RegisterKind::Char)
        );
    }

    #[test]
    fn an_empty_visual_selection_is_the_cursor_character() {
        assert_eq!(
            visual_range("abc", None, 1, false),
            (1, 2, RegisterKind::Char)
        );
        assert_eq!(
            visual_range("abc", Some((2, 2)), 2, false),
            (2, 3, RegisterKind::Char)
        );
    }

    #[test]
    fn visual_range_crosses_a_newline_and_clamps_at_the_end() {
        assert_eq!(
            visual_range("one\ntwo\n", Some((0, 4)), 0, false),
            (0, 5, RegisterKind::Char)
        );
        assert_eq!(
            visual_range("ab", Some((0, 2)), 2, false),
            (0, 2, RegisterKind::Char)
        );
        assert_eq!(visual_range("", None, 0, false), (0, 0, RegisterKind::Char));
    }

    #[test]
    fn visual_line_range_covers_every_touched_line() {
        let text = "one\ntwo\nthree\n";
        assert_eq!(
            visual_range(text, Some((0, 8)), 4, true),
            (0, 8, RegisterKind::Line)
        );
        assert_eq!(
            visual_range(text, Some((0, 4)), 0, true),
            (0, 4, RegisterKind::Line)
        );
        assert_eq!(
            visual_range(text, None, 9, true),
            (8, 14, RegisterKind::Line)
        );
    }

    #[test]
    fn a_counted_char_paste_repeats_and_lands_on_the_last_character() {
        let v = RegisterValue {
            text: "ab".into(),
            kind: RegisterKind::Char,
        };
        assert_eq!(
            paste_plan_counted("xy", 0, &v, false, 3),
            Some((1, "ababab".into(), 6))
        );
        assert_eq!(
            paste_plan_counted("xy", 0, &v, false, 0),
            Some(paste_plan("xy", 0, &v, false))
        );
    }

    #[test]
    fn a_counted_paste_past_the_limit_is_refused() {
        let v = RegisterValue {
            text: "ab".into(),
            kind: RegisterKind::Char,
        };
        // A saturated count (a long digit run) and one just over the limit.
        assert_eq!(paste_plan_counted("xy", 0, &v, false, usize::MAX), None);
        assert_eq!(
            paste_plan_counted("xy", 0, &v, false, PUT_LIMIT_BYTES / 2 + 1),
            None
        );
    }

    #[test]
    fn a_counted_line_paste_repeats_whole_lines_and_lands_on_the_first() {
        let v = RegisterValue {
            text: "  one\n".into(),
            kind: RegisterKind::Line,
        };
        assert_eq!(
            paste_plan_counted("a\nb\n", 0, &v, false, 2),
            Some((2, "  one\n  one\n".into(), 4))
        );
    }
}
