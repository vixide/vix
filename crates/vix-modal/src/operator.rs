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

fn first_non_blank_at(text: &str, pos: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let lines = line_ranges(&chars);
    let Some(&(s, e)) = lines.iter().find(|&&(s, e)| pos >= s && pos <= e) else {
        return pos;
    };
    (s..e).find(|&i| !chars[i].is_whitespace()).unwrap_or(s)
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
