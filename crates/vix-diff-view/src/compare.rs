//! The aligned two-pane model behind side-by-side Compare Files (T660).
//!
//! Both sides are real, editable buffers. This module only computes how their
//! lines correspond: the changed regions ([`Hunk`]s), the line on the other side
//! that matches a given line (for locked scrolling), the next and previous
//! difference, the gutter marks for each side, and the text that results from
//! copying one change across (vimdiff `dp`/`do`). It is pure, so the host
//! recomputes it from the two buffers whenever either changes.
//!
//! Conventions: all line numbers are 0-based; ranges are half-open
//! (`start..end`); a pure insertion or deletion is a hunk with an **empty**
//! range on the side that lacks the lines, positioned where they would go.
//! Spec: `crates/vix-diff-view/spec/index.md`.
//!
//! # Examples
//!
//! ```
//! use vix_diff_view::compare::{Side, copy_hunk, hunks, map_line};
//!
//! let (l, r) = ("a\nb\nc\n", "a\nB\nc\n");
//! let h = hunks(l, r);
//! assert_eq!(h.len(), 1);
//! assert_eq!(map_line(&h, Side::Left, 2), 2);
//! // Copy the right side's change over to the left.
//! assert_eq!(copy_hunk(&h, 0, Side::Right, l, r).unwrap(), r);
//! ```

use std::ops::Range;

/// One side of a comparison.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    /// The left (old) pane.
    Left,
    /// The right (new) pane.
    Right,
}

impl Side {
    /// The opposite side.
    #[must_use]
    pub fn other(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }
}

/// One changed region: the 0-based line range on each side (end exclusive).
/// One range is empty when the change is a pure insertion or deletion; it then
/// marks the position where the lines would go.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Hunk {
    /// Lines of the left text.
    pub left: Range<usize>,
    /// Lines of the right text.
    pub right: Range<usize>,
}

impl Hunk {
    /// This hunk's line range on `side`.
    #[must_use]
    pub fn on(&self, side: Side) -> &Range<usize> {
        match side {
            Side::Left => &self.left,
            Side::Right => &self.right,
        }
    }
}

/// How a line is marked in the compare gutter.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mark {
    /// The line exists on both sides but differs.
    Changed,
    /// The line exists only on this side.
    Only,
    /// Lines exist only on the other side, just before this line.
    Gap,
}

/// The changed regions between `left` and `right`, in order. Empty when the
/// texts are identical.
#[must_use]
pub fn hunks(left: &str, right: &str) -> Vec<Hunk> {
    use similar::{DiffOp, TextDiff};
    let diff = TextDiff::from_lines(left, right);
    let mut out: Vec<Hunk> = Vec::new();
    for op in diff.ops() {
        let (l, r) = match *op {
            DiffOp::Equal { .. } => continue,
            DiffOp::Delete {
                old_index,
                old_len,
                new_index,
            } => (old_index..old_index + old_len, new_index..new_index),
            DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => (old_index..old_index, new_index..new_index + new_len),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => (
                old_index..old_index + old_len,
                new_index..new_index + new_len,
            ),
        };
        // `similar` can emit a delete and an insert back to back; they are one
        // change to the reader.
        if let Some(last) = out.last_mut()
            && last.left.end == l.start
            && last.right.end == r.start
        {
            last.left.end = l.end;
            last.right.end = r.end;
        } else {
            out.push(Hunk { left: l, right: r });
        }
    }
    out
}

/// The filler rows `side` needs to stay row-aligned with the other side (T671),
/// as `(line, count)`: draw `count` blank rows just before real line `line` of
/// this side's text. A hunk where the other side has more lines gives this side
/// the difference, placed right after the hunk's lines here (before the line
/// that follows them; for a pure insertion on the other side, at the position
/// the lines would go).
#[must_use]
pub fn fillers(hunks: &[Hunk], side: Side) -> Vec<(usize, usize)> {
    let other = side.other();
    hunks
        .iter()
        .filter_map(|h| {
            let here = h.on(side);
            let there = h.on(other);
            (there.len() > here.len()).then(|| (here.end, there.len() - here.len()))
        })
        .collect()
}

/// The line on the other side that corresponds to `line` on `side`. Inside a
/// hunk it is the same offset into the other side's range, clamped to it.
#[must_use]
pub fn map_line(hunks: &[Hunk], side: Side, line: usize) -> usize {
    let other = side.other();
    // Lines outside any hunk correspond one to one, shifted by the net size
    // change of every hunk before them; lines inside a hunk map by offset.
    // Lines between hunks shift by the net size of every hunk before them.
    let mut delta: isize = 0;
    for h in hunks {
        let here = h.on(side);
        let there = h.on(other);
        if line < here.start {
            break;
        }
        if line < here.end {
            let off = line - here.start;
            return there.start + off.min(there.len().saturating_sub(1));
        }
        delta += signed(there.len()) - signed(here.len());
    }
    usize::try_from(signed(line) + delta).unwrap_or(0)
}

/// `usize` to `isize`, saturating (line counts never get near the limit; this
/// just avoids a lossy cast).
fn signed(n: usize) -> isize {
    isize::try_from(n).unwrap_or(isize::MAX)
}

/// The index of the hunk at `line` on `side`: the hunk whose range holds it,
/// or a pure insertion/deletion whose empty range sits at that line.
#[must_use]
pub fn hunk_at(hunks: &[Hunk], side: Side, line: usize) -> Option<usize> {
    hunks.iter().position(|h| {
        let r = h.on(side);
        r.contains(&line) || (r.is_empty() && r.start == line)
    })
}

/// The first hunk that starts after `line` on `side`.
#[must_use]
pub fn next_hunk(hunks: &[Hunk], side: Side, line: usize) -> Option<usize> {
    hunks.iter().position(|h| h.on(side).start > line)
}

/// The last hunk that starts before `line` on `side`.
#[must_use]
pub fn prev_hunk(hunks: &[Hunk], side: Side, line: usize) -> Option<usize> {
    hunks.iter().rposition(|h| h.on(side).start < line)
}

/// The gutter marks for `side`, as `(line, mark)` pairs.
#[must_use]
pub fn marks(hunks: &[Hunk], side: Side) -> Vec<(usize, Mark)> {
    let mut out = Vec::new();
    for h in hunks {
        let here = h.on(side);
        let there = h.on(side.other());
        if here.is_empty() {
            out.push((here.start, Mark::Gap));
            continue;
        }
        for (i, line) in here.clone().enumerate() {
            let mark = if i < there.len() {
                Mark::Changed
            } else {
                Mark::Only
            };
            out.push((line, mark));
        }
    }
    out
}

/// The text of the `from` side's opposite after copying hunk `index` across:
/// its lines on that side are replaced by the `from` side's lines. Returns
/// `None` when `index` is out of range.
#[must_use]
pub fn copy_hunk(
    hunks: &[Hunk],
    index: usize,
    from: Side,
    left: &str,
    right: &str,
) -> Option<String> {
    // Splice: keep dst up to the hunk, put src's hunk lines, keep dst after it.
    let h = hunks.get(index)?;
    let (src, dst) = match from {
        Side::Left => (left, right),
        Side::Right => (right, left),
    };
    let src_lines: Vec<&str> = src.split_inclusive('\n').collect();
    let dst_lines: Vec<&str> = dst.split_inclusive('\n').collect();
    let s = h.on(from).clone();
    let d = h.on(from.other()).clone();
    let mut chunk: String = src_lines
        .get(s.start.min(src_lines.len())..s.end.min(src_lines.len()))
        .unwrap_or_default()
        .concat();
    let after = dst_lines.get(d.end..).unwrap_or_default();
    // The source's last line may lack a newline; it needs one when lines follow.
    if !chunk.is_empty() && !chunk.ends_with('\n') && !after.is_empty() {
        chunk.push_str(newline_of(dst));
    }
    let mut out: String = dst_lines
        .get(..d.start.min(dst_lines.len()))
        .unwrap_or_default()
        .concat();
    // Appending after a final line with no newline: give that line one.
    if !chunk.is_empty() && !out.is_empty() && !out.ends_with('\n') {
        out.push_str(newline_of(dst));
    }
    out.push_str(&chunk);
    out.push_str(&after.concat());
    Some(out)
}

/// The line ending `text` uses: CRLF when its first line ends in one, else
/// LF. Used to terminate a final line that lacked a newline when text is
/// appended after it, so copied hunks keep the destination's convention.
fn newline_of(text: &str) -> &'static str {
    match text.find('\n') {
        Some(i) if text[..i].ends_with('\r') => "\r\n",
        _ => "\n",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const L: &str = "a\nb\nc\nd\ne\n";
    const R: &str = "a\nB\nc\nx\ny\nd\ne\n";

    #[test]
    fn fillers_pad_the_shorter_side_of_each_change() {
        let h = hunks(L, R);
        // Right gained "x","y" after "c": the left needs 2 rows before its "d".
        assert_eq!(fillers(&h, Side::Left), [(3, 2)]);
        assert_eq!(fillers(&h, Side::Right), []);
        // A deletion pads the right.
        let h = hunks(R, L);
        assert_eq!(fillers(&h, Side::Right), [(3, 2)]);
        assert_eq!(fillers(&hunks(L, L), Side::Left), []);
    }

    #[test]
    fn finds_changed_and_inserted_regions() {
        let h = hunks(L, R);
        assert_eq!(
            h,
            [
                Hunk {
                    left: 1..2,
                    right: 1..2
                },
                Hunk {
                    left: 3..3,
                    right: 3..5
                },
            ]
        );
        assert_eq!(hunks(L, L), []);
    }

    #[test]
    fn maps_lines_across_hunks() {
        let h = hunks(L, R);
        assert_eq!(map_line(&h, Side::Left, 0), 0);
        assert_eq!(map_line(&h, Side::Left, 2), 2);
        // After the two-line insertion, left 3 ("d") is right 5.
        assert_eq!(map_line(&h, Side::Left, 3), 5);
        assert_eq!(map_line(&h, Side::Right, 5), 3);
        // Inside the insertion, right lines map to the gap on the left.
        assert_eq!(map_line(&h, Side::Right, 4), 3);
    }

    #[test]
    fn navigates_between_hunks() {
        let h = hunks(L, R);
        assert_eq!(next_hunk(&h, Side::Left, 0), Some(0));
        assert_eq!(next_hunk(&h, Side::Left, 1), Some(1));
        assert_eq!(next_hunk(&h, Side::Left, 3), None);
        assert_eq!(prev_hunk(&h, Side::Right, 4), Some(1));
        assert_eq!(prev_hunk(&h, Side::Right, 1), None);
        assert_eq!(hunk_at(&h, Side::Left, 3), Some(1));
        assert_eq!(hunk_at(&h, Side::Right, 2), None);
    }

    #[test]
    fn marks_each_side() {
        let h = hunks(L, R);
        assert_eq!(marks(&h, Side::Left), [(1, Mark::Changed), (3, Mark::Gap)]);
        assert_eq!(
            marks(&h, Side::Right),
            [(1, Mark::Changed), (3, Mark::Only), (4, Mark::Only)]
        );
    }

    #[test]
    fn copies_a_change_either_way() {
        let h = hunks(L, R);
        // Right's insertion copied to the left.
        assert_eq!(
            copy_hunk(&h, 1, Side::Right, L, R).unwrap(),
            "a\nb\nc\nx\ny\nd\ne\n"
        );
        // Left's line copied to the right undoes the change there.
        assert_eq!(
            copy_hunk(&h, 0, Side::Left, L, R).unwrap(),
            "a\nb\nc\nx\ny\nd\ne\n"
        );
        // Copying an empty side deletes the inserted lines.
        assert_eq!(
            copy_hunk(&h, 1, Side::Left, L, R).unwrap(),
            "a\nB\nc\nd\ne\n"
        );
        assert_eq!(copy_hunk(&h, 9, Side::Left, L, R), None);
    }

    #[test]
    fn copy_handles_missing_final_newline_and_crlf() {
        let h = hunks("a\r\nb\r\n", "a\r\nb\r\nc");
        assert_eq!(
            copy_hunk(&h, 0, Side::Right, "a\r\nb\r\n", "a\r\nb\r\nc").unwrap(),
            "a\r\nb\r\nc"
        );
        let h = hunks("a\nz\nc", "a\nb\nc");
        assert_eq!(
            copy_hunk(&h, 0, Side::Left, "a\nz\nc", "a\nb\nc").unwrap(),
            "a\nz\nc"
        );
    }
}
