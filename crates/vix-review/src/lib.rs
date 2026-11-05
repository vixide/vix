//! Branch review (T665): turn the unified diff of a revision range into one
//! multi-file buffer, keep line comments against it, and export them.
//!
//! The crate is pure: it parses diff text and renders text. Running `git`
//! lives in `vix-git`, and reading or writing the comment file is the host's
//! job (it goes through the filesystem authority). See `spec/index.md`.
#![deny(missing_docs)]

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

/// Which side of the diff a comment is anchored to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Side {
    /// The old file (a removed line).
    Old,
    /// The new file (an added or unchanged line).
    New,
}

/// One saved line comment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    /// Repository-relative path of the file.
    pub file: String,
    /// Side of the diff the line number counts on.
    pub side: Side,
    /// 1-based line number on that side.
    pub line: usize,
    /// The diff line's text when the comment was written, so an export reads
    /// without the diff beside it.
    pub code: String,
    /// The comment itself.
    pub text: String,
}

/// Kind of one hunk line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LineKind {
    /// Unchanged context.
    Context,
    /// Added in the new file.
    Added,
    /// Removed from the old file.
    Removed,
}

/// One line of a hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    /// Context, added or removed.
    pub kind: LineKind,
    /// Text without the leading `+`, `-` or space.
    pub text: String,
    /// 1-based line number in the old file (not for added lines).
    pub old: Option<usize>,
    /// 1-based line number in the new file (not for removed lines).
    pub new: Option<usize>,
    /// Git printed `\ No newline at end of file` right after this line.
    pub no_newline: bool,
}

/// One `@@` hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// The `@@ -a,b +c,d @@ ...` header line.
    pub header: String,
    /// The hunk's lines.
    pub lines: Vec<DiffLine>,
}

/// One changed file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    /// Repository-relative path (the new path, or the old one for a deletion).
    pub path: String,
    /// `true` for a binary file, which has no hunks.
    pub binary: bool,
    /// The file's hunks.
    pub hunks: Vec<Hunk>,
}

/// The narrowest terminal width, in columns, at which the two-pane view is
/// drawn; below it the host falls back to the inline buffer.
pub const MIN_SIDE_BY_SIDE_WIDTH: u16 = 60;

/// Columns the line number takes at the start of a pair cell (`"{:>4} "`);
/// the text of a cell gets the cell's width minus this.
pub const GUTTER: usize = 5;

/// Split `text` into pieces of at most `width` display columns, as char
/// ranges `(start, end)` into `text`. Empty text is one empty piece, so a
/// wrapped line is never zero rows tall. A character wider than `width` gets a
/// piece of its own.
#[must_use]
pub fn wrap_text(text: &str, width: usize) -> Vec<(usize, usize)> {
    use unicode_width::UnicodeWidthChar;
    let width = width.max(1);
    let mut out = Vec::new();
    let (mut start, mut used, mut count) = (0usize, 0usize, 0usize);
    for ch in text.chars() {
        let w = ch.width().unwrap_or(0);
        if used > 0 && used + w > width {
            out.push((start, count));
            start = count;
            used = 0;
        }
        used += w;
        count += 1;
    }
    out.push((start, count));
    out
}

/// One side of a two-pane row: a numbered line of the old or new file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// 1-based line number on this side.
    pub number: usize,
    /// The line's text, without the diff marker.
    pub text: String,
    /// How the line changed: [`LineKind::Removed`] on the left,
    /// [`LineKind::Added`] on the right, [`LineKind::Context`] on both.
    pub kind: LineKind,
    /// The row of the inline buffer ([`Review::render`]) this line is on,
    /// which is where a click on the cell puts the cursor.
    pub inline: usize,
    /// Char ranges (`start..end`, in chars of `text`) that differ from the line
    /// paired with this one, as `vix_diff_view` finds them word by word. Empty
    /// for an unpaired line, a context line, and a line that differs entirely.
    pub marks: Vec<(usize, usize)>,
}

/// One half of a two-pane row: where a click landed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Half {
    /// The old text.
    Left,
    /// The new text.
    Right,
}

/// What a two-pane row shows.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PaneKind {
    /// The title, a file heading, a `(binary file)` note or a hunk header:
    /// one text across the full width.
    Heading,
    /// A saved comment, across the full width.
    Comment,
    /// A line pair: old text on the left, new text on the right. Either side
    /// is `None` when the other has no counterpart (a pure addition or
    /// removal).
    Pair {
        /// The old file's line.
        left: Option<Cell>,
        /// The new file's line.
        right: Option<Cell>,
    },
}

/// One row of the two-pane view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneRow {
    /// What the row shows.
    pub kind: PaneKind,
    /// The text of a [`PaneKind::Heading`] or [`PaneKind::Comment`] row;
    /// empty for a pair.
    pub text: String,
    /// First row of the inline buffer ([`Review::render`]) this row stands
    /// for. The inline buffer keeps the cursor, so the two-pane view scrolls
    /// and highlights by these.
    pub first: usize,
    /// Last inline row this row stands for (inclusive).
    pub last: usize,
}

/// The review laid out for the two-pane view: old text left, new text right,
/// aligned hunk by hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SideBySide {
    /// The rows, top to bottom.
    pub rows: Vec<PaneRow>,
}

impl PaneRow {
    /// How many screen lines this row takes when each pair cell is wrapped
    /// within its pane (`left` and `right` columns wide, as
    /// [`SideBySide::halves`] gives); the longer side decides and the shorter
    /// is padded. Headings and comments stay one line (they are cut).
    #[must_use]
    pub fn height(&self, left: usize, right: usize) -> usize {
        let PaneKind::Pair { left: l, right: r } = &self.kind else {
            return 1;
        };
        let lines = |c: &Option<Cell>, w: usize| {
            c.as_ref()
                .map_or(1, |c| wrap_text(&c.text, w.saturating_sub(GUTTER)).len())
        };
        lines(l, left).max(lines(r, right))
    }
}

impl SideBySide {
    /// Screen lines of row `index` in a view `width` columns wide: its
    /// wrapped height when `wrap` is on, else 1. A missing row is 1.
    #[must_use]
    pub fn row_height(&self, index: usize, width: usize, wrap: bool) -> usize {
        if !wrap {
            return 1;
        }
        let (l, r) = Self::halves(width);
        self.rows.get(index).map_or(1, |row| row.height(l, r))
    }

    /// The row under screen line `y` of a view that starts at row `first`,
    /// and which line of that row it is (0 for its first line); `None` past
    /// the last row. `row_height` gives each row's screen lines.
    #[must_use]
    pub fn row_at(
        &self,
        first: usize,
        y: usize,
        row_height: impl Fn(usize) -> usize,
    ) -> Option<(usize, usize)> {
        let mut top = 0;
        for index in first..self.rows.len() {
            let h = row_height(index).max(1);
            if y < top + h {
                return Some((index, y - top));
            }
            top += h;
        }
        None
    }

    /// The largest first row that still fills a view `height` lines tall with
    /// rows to the end (0 when everything fits), for bounding the wheel.
    #[must_use]
    pub fn last_first(&self, height: usize, row_height: impl Fn(usize) -> usize) -> usize {
        let (mut index, mut used) = (self.rows.len(), 0);
        while index > 0 && used + row_height(index - 1).max(1) <= height {
            index -= 1;
            used += row_height(index).max(1);
        }
        index.min(self.rows.len().saturating_sub(1))
    }

    /// Index of the first row that covers inline row `inline` or comes after
    /// it; the last row when `inline` is past the end. This is how the two
    /// views stay scroll-synced.
    #[must_use]
    pub fn row_for_inline(&self, inline: usize) -> usize {
        self.rows
            .iter()
            .position(|r| r.last >= inline)
            .unwrap_or_else(|| self.rows.len().saturating_sub(1))
    }

    /// The widths of the left and right panes in a view `width` columns wide:
    /// one column goes to the divider, the right pane takes the odd one.
    #[must_use]
    pub fn halves(width: usize) -> (usize, usize) {
        let usable = width.saturating_sub(1);
        let left = usable / 2;
        (left, usable - left)
    }

    /// The first row to show in a view `height` screen lines tall: the row
    /// for the buffer's top line `top_inline`, moved only as far as needed to
    /// keep the row of `cursor` (an inline row) inside the view. `row_height`
    /// gives the screen lines of a row (see [`Self::row_height`]).
    #[must_use]
    pub fn view_start(
        &self,
        top_inline: usize,
        cursor: usize,
        height: usize,
        row_height: impl Fn(usize) -> usize,
    ) -> usize {
        let mut first = self.row_for_inline(top_inline);
        let selected = self.row_for_inline(cursor);
        if selected < first {
            return selected;
        }
        if height == 0 {
            return first;
        }
        // Screen lines of the rows from `first` down to the cursor's row.
        let mut used: usize = (first..=selected).map(|i| row_height(i).max(1)).sum();
        while used > height && first < selected {
            used -= row_height(first).max(1);
            first += 1;
        }
        first
    }

    /// The inline buffer's top line that makes row `index` the first one
    /// [`Self::row_for_inline`] finds, so that scrolling the two-pane view can
    /// move the inline buffer's offset. Paired rows overlap in inline rows, so
    /// this is one past the largest `last` of the rows above, or that row's
    /// own `last` when that is not possible.
    #[must_use]
    pub fn top_for_row(&self, index: usize) -> usize {
        let Some(row) = self.rows.get(index) else {
            return self.rows.last().map_or(0, |r| r.last);
        };
        let above = self.rows[..index].iter().map(|r| r.last + 1).max();
        above.map_or(0, |a| a.min(row.last))
    }

    /// The inline row a click on `half` of row `index` stands for: that cell's
    /// own line, the other cell's when this one is blank, and the row's first
    /// inline row for a heading or comment. `None` past the last row.
    #[must_use]
    pub fn inline_at(&self, index: usize, half: Half) -> Option<usize> {
        let row = self.rows.get(index)?;
        let PaneKind::Pair { left, right } = &row.kind else {
            return Some(row.first);
        };
        let (want, other) = match half {
            Half::Left => (left, right),
            Half::Right => (right, left),
        };
        Some(
            want.as_ref()
                .or(other.as_ref())
                .map_or(row.first, |c| c.inline),
        )
    }
}

/// What a row of the rendered buffer stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Target {
    /// A title or blank row.
    None,
    /// A file heading.
    File(usize),
    /// A hunk header.
    Hunk(usize, usize),
    /// A diff line that can carry a comment.
    Line {
        /// Index into [`Review::files`].
        file: usize,
        /// Side the number counts on.
        side: Side,
        /// 1-based line number on that side.
        line: usize,
    },
    /// A rendered comment row.
    Comment(usize),
}

/// The rendered multi-file buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    /// Buffer text.
    pub text: String,
    /// One target per buffer line.
    pub targets: Vec<Target>,
}

impl Rendered {
    /// 0-based buffer lines of every hunk header, in order.
    #[must_use]
    pub fn hunk_rows(&self) -> Vec<usize> {
        self.targets
            .iter()
            .enumerate()
            .filter(|(_, t)| matches!(t, Target::Hunk(..)))
            .map(|(i, _)| i)
            .collect()
    }

    /// The `(file, hunk)` indexes of the hunk that `row` is in: its header
    /// row or any diff or comment row under it. `None` on a title, blank or
    /// file-heading row, and on rows before the first hunk of a file.
    #[must_use]
    pub fn hunk_at(&self, row: usize) -> Option<(usize, usize)> {
        let upto = row.min(self.targets.len().checked_sub(1)?);
        for target in self.targets[..=upto].iter().rev() {
            match target {
                Target::Hunk(f, h) => return Some((*f, *h)),
                Target::File(_) => return None,
                _ => {}
            }
        }
        None
    }

    /// The first hunk header after `row` (wrapping to the first), if any.
    #[must_use]
    pub fn next_hunk(&self, row: usize) -> Option<usize> {
        let rows = self.hunk_rows();
        rows.iter()
            .copied()
            .find(|&r| r > row)
            .or_else(|| rows.first().copied())
    }

    /// The last hunk header before `row` (wrapping to the last), if any.
    #[must_use]
    pub fn prev_hunk(&self, row: usize) -> Option<usize> {
        let rows = self.hunk_rows();
        rows.iter()
            .rev()
            .copied()
            .find(|&r| r < row)
            .or_else(|| rows.last().copied())
    }
}

/// A review of one revision range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Review {
    /// The range as the user typed it (`main..feature`, a commit, `stash@{0}`).
    pub range: String,
    /// The changed files.
    pub files: Vec<FileDiff>,
    /// The saved comments.
    pub comments: Vec<Comment>,
}

impl Review {
    /// Build a review from the unified diff of `range`.
    #[must_use]
    pub fn new(range: &str, diff: &str) -> Self {
        Self {
            range: range.to_string(),
            files: parse(diff),
            comments: Vec::new(),
        }
    }

    /// Total number of changed lines (added plus removed).
    #[must_use]
    pub fn changed_lines(&self) -> usize {
        self.files
            .iter()
            .flat_map(|f| &f.hunks)
            .flat_map(|h| &h.lines)
            .filter(|l| l.kind != LineKind::Context)
            .count()
    }

    /// Comments on `file` at `side`/`line`, with their indexes.
    fn comments_at<'a>(
        &'a self,
        file: &'a str,
        side: Side,
        line: usize,
    ) -> impl Iterator<Item = (usize, &'a Comment)> {
        self.comments
            .iter()
            .enumerate()
            .filter(move |(_, c)| c.file == file && c.side == side && c.line == line)
    }

    /// Set the comment on a diff line: a non-empty `text` replaces any comment
    /// already there, an empty one removes it. Returns `true` when a comment
    /// now exists on the line.
    pub fn set_comment(
        &mut self,
        file: &str,
        side: Side,
        line: usize,
        code: &str,
        text: &str,
    ) -> bool {
        self.comments
            .retain(|c| !(c.file == file && c.side == side && c.line == line));
        let text = text.trim();
        if text.is_empty() {
            return false;
        }
        self.comments.push(Comment {
            file: file.to_string(),
            side,
            line,
            code: code.to_string(),
            text: text.to_string(),
        });
        true
    }

    /// The comment text on a diff line, if any (for pre-filling an edit).
    #[must_use]
    pub fn comment_text(&self, file: &str, side: Side, line: usize) -> Option<&str> {
        self.comments
            .iter()
            .find(|c| c.file == file && c.side == side && c.line == line)
            .map(|c| c.text.as_str())
    }

    /// Render the whole review as one buffer, with each comment on a row
    /// right under its line.
    #[must_use]
    pub fn render(&self) -> Rendered {
        let mut text = String::new();
        let mut targets = Vec::new();
        let mut push = |s: &str, t: Target| {
            text.push_str(s);
            text.push('\n');
            targets.push(t);
        };
        push(
            &format!(
                "Review {}: {} files, {} changed lines, {} comments",
                self.range,
                self.files.len(),
                self.changed_lines(),
                self.comments.len()
            ),
            Target::None,
        );
        for (fi, f) in self.files.iter().enumerate() {
            push("", Target::None);
            push(&format!("=== {}", f.path), Target::File(fi));
            if f.binary {
                push("(binary file)", Target::None);
            }
            for (hi, h) in f.hunks.iter().enumerate() {
                push(&h.header, Target::Hunk(fi, hi));
                for l in &h.lines {
                    let (mark, side, n) = match l.kind {
                        LineKind::Context => (' ', Side::New, l.new),
                        LineKind::Added => ('+', Side::New, l.new),
                        LineKind::Removed => ('-', Side::Old, l.old),
                    };
                    let n = n.unwrap_or(0);
                    push(
                        &format!("{mark}{}", l.text),
                        Target::Line {
                            file: fi,
                            side,
                            line: n,
                        },
                    );
                    if l.no_newline {
                        push(NO_NEWLINE, Target::None);
                    }
                    for (ci, c) in self.comments_at(&f.path, side, n) {
                        push(
                            &format!("    >> {}", c.text.replace('\n', " ")),
                            Target::Comment(ci),
                        );
                    }
                }
            }
        }
        Rendered { text, targets }
    }

    /// Lay the review out for the two-pane view (see [`SideBySide`]). Within
    /// a hunk a run of removed lines is paired with the run of added lines
    /// that follows it, line by line; the longer run leaves blank cells on the
    /// other side. Context lines show on both sides. Comments follow the pair
    /// group they belong to. Row counting mirrors [`Self::render`] exactly, so
    /// every row's `first`/`last` indexes the inline buffer.
    #[must_use]
    pub fn side_by_side(&self) -> SideBySide {
        let mut rows: Vec<PaneRow> = Vec::new();
        let mut row = 0usize; // next inline row
        let heading = |rows: &mut Vec<PaneRow>, row: &mut usize, text: String| {
            rows.push(PaneRow {
                kind: PaneKind::Heading,
                text,
                first: *row,
                last: *row,
            });
            *row += 1;
        };
        heading(
            &mut rows,
            &mut row,
            format!(
                "Review {}: {} files, {} changed lines, {} comments",
                self.range,
                self.files.len(),
                self.changed_lines(),
                self.comments.len()
            ),
        );
        for f in &self.files {
            row += 1; // the blank row before the file heading
            heading(&mut rows, &mut row, format!("=== {}", f.path));
            if f.binary {
                heading(&mut rows, &mut row, "(binary file)".to_string());
            }
            for h in &f.hunks {
                heading(&mut rows, &mut row, h.header.clone());
                self.pane_hunk(f, h, &mut row, &mut rows);
            }
        }
        SideBySide { rows }
    }

    /// Append the rows of one hunk (see [`Self::side_by_side`]).
    fn pane_hunk(&self, f: &FileDiff, h: &Hunk, row: &mut usize, rows: &mut Vec<PaneRow>) {
        // A pending change group: removed and added lines with their inline
        // rows, plus the comments that follow them.
        let mut removed: Vec<(Cell, usize)> = Vec::new();
        let mut added: Vec<(Cell, usize)> = Vec::new();
        let mut comments: Vec<(String, usize)> = Vec::new();
        for l in &h.lines {
            let at = *row;
            *row += 1 + usize::from(l.no_newline);
            let (side, n) = match l.kind {
                LineKind::Removed => (Side::Old, l.old.unwrap_or(0)),
                _ => (Side::New, l.new.unwrap_or(0)),
            };
            let cell = |number: usize| Cell {
                number,
                text: l.text.clone(),
                kind: l.kind,
                inline: at,
                marks: Vec::new(),
            };
            // Inline, a line's comments sit right under it (and its marker).
            let mut here = Vec::new();
            for (_, c) in self.comments_at(&f.path, side, n) {
                here.push((format!("    >> {}", c.text.replace('\n', " ")), *row));
                *row += 1;
            }
            match l.kind {
                LineKind::Removed => {
                    // A removal after additions starts a new group.
                    if !added.is_empty() {
                        flush_group(rows, &mut removed, &mut added, &mut comments);
                    }
                    removed.push((cell(n), at));
                    comments.extend(here);
                }
                LineKind::Added => {
                    added.push((cell(n), at));
                    comments.extend(here);
                }
                LineKind::Context => {
                    flush_group(rows, &mut removed, &mut added, &mut comments);
                    let c = cell(n);
                    rows.push(PaneRow {
                        kind: PaneKind::Pair {
                            left: Some(Cell {
                                number: l.old.unwrap_or(0),
                                ..c.clone()
                            }),
                            right: Some(c),
                        },
                        text: String::new(),
                        first: at,
                        last: at + usize::from(l.no_newline),
                    });
                    for (text, r) in here {
                        rows.push(PaneRow {
                            kind: PaneKind::Comment,
                            text,
                            first: r,
                            last: r,
                        });
                    }
                }
            }
        }
        flush_group(rows, &mut removed, &mut added, &mut comments);
    }

    /// A one-hunk unified patch for `git apply`: file headers, the hunk header
    /// and its lines. A hunk that starts at `-0,0` is a new file
    /// (`--- /dev/null`) and one that ends at `+0,0` a deletion
    /// (`+++ /dev/null`). `None` for a binary file or an index out of range.
    /// A `\ No newline at end of file` marker is kept after the line it
    /// follows, so such a hunk applies.
    #[must_use]
    pub fn hunk_patch(&self, file: usize, hunk: usize) -> Option<String> {
        let f = self.files.get(file)?;
        if f.binary {
            return None;
        }
        let h = f.hunks.get(hunk)?;
        let mut out = patch_header(&f.path, h);
        push_hunk(&mut out, h);
        Some(out)
    }

    /// One patch holding every hunk of every text file (binary files and
    /// files without hunks are skipped), for staging a whole stash or range
    /// at once. `None` when there is nothing to apply.
    #[must_use]
    pub fn full_patch(&self) -> Option<String> {
        let mut out = String::new();
        for f in self.files.iter().filter(|f| !f.binary) {
            let Some(first) = f.hunks.first() else {
                continue;
            };
            out.push_str(&patch_header(&f.path, first));
            for h in &f.hunks {
                push_hunk(&mut out, h);
            }
        }
        (!out.is_empty()).then_some(out)
    }

    /// How many hunks and files [`Self::full_patch`] covers: `(hunks, files)`
    /// over the text files that have at least one hunk. The discard-all
    /// confirmation names these.
    #[must_use]
    pub fn patch_counts(&self) -> (usize, usize) {
        self.files
            .iter()
            .filter(|f| !f.binary && !f.hunks.is_empty())
            .fold((0, 0), |(h, n), f| (h + f.hunks.len(), n + 1))
    }

    /// Replace the diff with a freshly computed one for the same range
    /// (after a stage or discard), keeping every comment. Comments whose line
    /// is gone stay stored but are not shown.
    pub fn replace_diff(&mut self, diff: &str) {
        self.files = parse(diff);
    }

    /// The code text of a [`Target::Line`], for storing with a comment.
    #[must_use]
    pub fn line_text(&self, file: usize, side: Side, line: usize) -> Option<&str> {
        self.files
            .get(file)?
            .hunks
            .iter()
            .flat_map(|h| &h.lines)
            .find(|l| match side {
                Side::New => l.new == Some(line) && l.kind != LineKind::Removed,
                Side::Old => l.old == Some(line) && l.kind == LineKind::Removed,
            })
            .map(|l| l.text.as_str())
    }

    /// The comments as Markdown, grouped by file, ready to paste into a pull
    /// request or an agent prompt.
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let mut out = format!("# Review of `{}`\n", self.range);
        if self.comments.is_empty() {
            out.push_str("\nNo comments.\n");
            return out;
        }
        let mut sorted: Vec<&Comment> = self.comments.iter().collect();
        sorted.sort_by(|a, b| (&a.file, a.line, a.side).cmp(&(&b.file, b.line, b.side)));
        let mut current = "";
        for c in sorted {
            if c.file != current {
                let _ = write!(out, "\n## `{}`\n", c.file);
                current = &c.file;
            }
            let side = match c.side {
                Side::Old => " (old)",
                Side::New => "",
            };
            let _ = write!(out, "\n- **Line {}{side}**: {}\n", c.line, c.text);
            if !c.code.trim().is_empty() {
                let _ = write!(out, "  ```\n  {}\n  ```\n", c.code);
            }
        }
        out
    }

    /// The comments as JSON (`{"range": .., "comments": [..]}`).
    #[must_use]
    pub fn to_json(&self) -> String {
        // The on-disk shape of an exported review: a private DTO so the JSON
        // format stays decoupled from `Review`'s in-memory fields.
        #[derive(Serialize)]
        struct Out<'a> {
            range: &'a str,
            comments: &'a [Comment],
        }
        serde_json::to_string_pretty(&Out {
            range: &self.range,
            comments: &self.comments,
        })
        .unwrap_or_default()
    }

    /// Load comments saved by [`Review::to_json`] into this review. Text that
    /// does not parse leaves the review unchanged and returns `false`.
    pub fn load_json(&mut self, json: &str) -> bool {
        // Mirror of `Out`; `default` makes a file with no `comments` key load as
        // an empty review instead of failing.
        #[derive(Deserialize)]
        struct In {
            #[serde(default)]
            comments: Vec<Comment>,
        }
        match serde_json::from_str::<In>(json) {
            Ok(v) => {
                self.comments = v.comments;
                true
            }
            Err(_) => false,
        }
    }
}

/// Emit the pending change group as aligned rows (removed lines paired with
/// added ones), then its comments, and clear it.
fn flush_group(
    rows: &mut Vec<PaneRow>,
    removed: &mut Vec<(Cell, usize)>,
    added: &mut Vec<(Cell, usize)>,
    comments: &mut Vec<(String, usize)>,
) {
    let (mut rem, mut add) = (std::mem::take(removed), std::mem::take(added));
    for (l, r) in rem.iter_mut().zip(add.iter_mut()) {
        let (a, b) = word_marks(&l.0.text, &r.0.text);
        l.0.marks = a;
        r.0.marks = b;
    }
    for i in 0..rem.len().max(add.len()) {
        let (l, r) = (rem.get(i), add.get(i));
        let at: Vec<usize> = [l, r].into_iter().flatten().map(|(_, at)| *at).collect();
        rows.push(PaneRow {
            kind: PaneKind::Pair {
                left: l.map(|(c, _)| c.clone()),
                right: r.map(|(c, _)| c.clone()),
            },
            text: String::new(),
            first: at.iter().copied().min().unwrap_or(0),
            last: at.iter().copied().max().unwrap_or(0),
        });
    }
    for (text, r) in std::mem::take(comments) {
        rows.push(PaneRow {
            kind: PaneKind::Comment,
            text,
            first: r,
            last: r,
        });
    }
}

/// The char ranges of `old` and `new` that differ word by word, from
/// `vix_diff_view::build` on the two lines. A side that differs from end to
/// end gets none: marking every character says nothing.
fn word_marks(old: &str, new: &str) -> (Vec<(usize, usize)>, Vec<(usize, usize)>) {
    use vix_diff_view::Kind;
    let lines = vix_diff_view::build(&format!("{old}\n"), &format!("{new}\n"));
    let pick = |kind: Kind, text: &str| -> Vec<(usize, usize)> {
        let len = text.chars().count();
        let marks = lines
            .iter()
            .find(|l| l.kind == kind)
            .map(|l| l.emphasis.clone())
            .unwrap_or_default();
        if marks.iter().any(|&(a, b)| a == 0 && b >= len) {
            Vec::new()
        } else {
            marks
        }
    };
    (pick(Kind::Del, old), pick(Kind::Add, new))
}

/// The file name (without extension) a range's comments are stored under in
/// `.vix/review/`: every character other than a letter, digit, `.`, `-` or
/// `_` becomes `_`, and leading dots are dropped so the name never hides.
#[must_use]
pub fn store_name(range: &str) -> String {
    let s: String = range
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let s = s.trim_start_matches('.');
    if s.is_empty() {
        "review".to_string()
    } else {
        s.to_string()
    }
}

/// Which way a reduced patch is going to be applied: staging applies it to the
/// index as written, discarding applies it reversed to the working tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LineMode {
    /// `git apply --cached`: unselected removed lines stay (as context),
    /// unselected added lines are left out.
    Stage,
    /// `git apply --reverse`: unselected added lines stay (as context),
    /// unselected removed lines are left out.
    Discard,
}

/// Why a set of rows cannot become a line-level patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SelectError {
    /// No added or removed line is in the selected rows.
    NoLines,
    /// The selected lines are in more than one hunk (or file).
    SpansHunks,
    /// The file is binary, or the hunk is gone.
    NoHunk,
    /// Leaving a line out would change whether the file's last line ends in a
    /// newline, which a reduced patch cannot express; select the removed and
    /// the added copy of that line together.
    NoNewline,
}

/// The changed lines picked out of one hunk for a line-level stage or discard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineSelection {
    /// Index into [`Review::files`].
    pub file: usize,
    /// Index into the file's hunks.
    pub hunk: usize,
    /// Indexes into the hunk's lines of the added and removed lines picked.
    pub lines: Vec<usize>,
}

impl Review {
    /// The added and removed lines on buffer rows `first..=last` of `rendered`
    /// (context lines, headings and comments in the range are ignored).
    ///
    /// # Errors
    /// [`SelectError::NoLines`] when the rows hold no changed line and
    /// [`SelectError::SpansHunks`] when they hold lines of two hunks.
    pub fn select_rows(
        &self,
        rendered: &Rendered,
        first: usize,
        last: usize,
    ) -> Result<LineSelection, SelectError> {
        let mut found: Option<LineSelection> = None;
        for row in first..=last.min(rendered.targets.len().saturating_sub(1)) {
            let Some(Target::Line { file, side, line }) = rendered.targets.get(row) else {
                continue;
            };
            let Some((f, h)) = rendered.hunk_at(row) else {
                continue;
            };
            if f != *file {
                continue;
            }
            let Some(index) = self.files[f].hunks.get(h).and_then(|hunk| {
                hunk.lines.iter().position(|l| match side {
                    Side::Old => l.kind == LineKind::Removed && l.old == Some(*line),
                    Side::New => l.kind != LineKind::Removed && l.new == Some(*line),
                })
            }) else {
                continue;
            };
            if self.files[f].hunks[h].lines[index].kind == LineKind::Context {
                continue;
            }
            match &mut found {
                None => {
                    found = Some(LineSelection {
                        file: f,
                        hunk: h,
                        lines: vec![index],
                    });
                }
                Some(sel) if sel.file == f && sel.hunk == h => sel.lines.push(index),
                Some(_) => return Err(SelectError::SpansHunks),
            }
        }
        found.ok_or(SelectError::NoLines)
    }

    /// A one-hunk patch holding only the lines of `sel`, built the way
    /// `git add -p` edits a hunk, for `git apply --cached` ([`LineMode::Stage`])
    /// or `git apply --reverse` ([`LineMode::Discard`]). The hunk header's
    /// counts are recomputed; a `\ No newline at end of file` marker goes with
    /// its line.
    ///
    /// # Errors
    /// [`SelectError::NoHunk`] for a binary file or a missing hunk and
    /// [`SelectError::NoNewline`] when an unselected line that carries the
    /// marker would have to become context.
    pub fn lines_patch(&self, sel: &LineSelection, mode: LineMode) -> Result<String, SelectError> {
        let f = self.files.get(sel.file).ok_or(SelectError::NoHunk)?;
        let h = f
            .hunks
            .get(sel.hunk)
            .filter(|_| !f.binary)
            .ok_or(SelectError::NoHunk)?;
        let mut lines: Vec<DiffLine> = Vec::new();
        let (mut old_count, mut new_count) = (0usize, 0usize);
        for (i, l) in h.lines.iter().enumerate() {
            let picked = sel.lines.contains(&i);
            // The kind this line has in the reduced patch, or dropped.
            let kind = match (l.kind, picked, mode) {
                (LineKind::Context, _, _)
                | (LineKind::Removed, false, LineMode::Stage)
                | (LineKind::Added, false, LineMode::Discard) => LineKind::Context,
                (LineKind::Added, false, LineMode::Stage)
                | (LineKind::Removed, false, LineMode::Discard) => continue,
                (kind, true, _) => kind,
            };
            if kind == LineKind::Context && l.kind != LineKind::Context && l.no_newline {
                return Err(SelectError::NoNewline);
            }
            match kind {
                LineKind::Context => {
                    old_count += 1;
                    new_count += 1;
                }
                LineKind::Removed => old_count += 1,
                LineKind::Added => new_count += 1,
            }
            lines.push(DiffLine { kind, ..l.clone() });
        }
        let header = reduced_header(&h.header, mode, old_count, new_count);
        let reduced = Hunk { header, lines };
        let mut out = patch_header(&f.path, &reduced);
        push_hunk(&mut out, &reduced);
        Ok(out)
    }
}

/// The `@@` line for a reduced hunk: the original's function-context suffix,
/// the new counts, and the start of the side the patch is matched against
/// (the old side to stage, the new side to discard) kept; the other start is
/// derived from it, following git's convention for an empty side (the line
/// before the change, or 0 for a whole file).
fn reduced_header(original: &str, mode: LineMode, old_count: usize, new_count: usize) -> String {
    let words: Vec<&str> = original.split_whitespace().collect();
    let start = |w: Option<&&str>, sign: char| -> usize {
        w.and_then(|w| w.strip_prefix(sign))
            .and_then(|w| w.split(',').next())
            .and_then(|n| n.parse().ok())
            .unwrap_or(0)
    };
    let (old_start, new_start) = (start(words.get(1), '-'), start(words.get(2), '+'));
    let (base, base_count, other_count) = match mode {
        LineMode::Stage => (old_start, old_count, new_count),
        LineMode::Discard => (new_start, new_count, old_count),
    };
    let other = if other_count == 0 {
        base.saturating_sub(1)
    } else if base_count == 0 {
        base + 1
    } else {
        base
    };
    let (o, n) = match mode {
        LineMode::Stage => (base, other),
        LineMode::Discard => (other, base),
    };
    let suffix = original
        .match_indices("@@")
        .nth(1)
        .map_or("", |(i, _)| &original[i + 2..]);
    format!("@@ -{o},{old_count} +{n},{new_count} @@{suffix}")
}

/// The marker git prints after a line that has no trailing newline.
const NO_NEWLINE: &str = "\\ No newline at end of file";

/// The `---`/`+++` file header for a patch whose first hunk is `h`.
fn patch_header(path: &str, h: &Hunk) -> String {
    let words: Vec<&str> = h.header.split_whitespace().collect();
    let is_new = words.get(1).is_some_and(|w| w.starts_with("-0,0"));
    let is_gone = words.get(2).is_some_and(|w| w.starts_with("+0,0"));
    let old = if is_new {
        "/dev/null".to_string()
    } else {
        format!("a/{path}")
    };
    let new = if is_gone {
        "/dev/null".to_string()
    } else {
        format!("b/{path}")
    };
    format!("--- {old}\n+++ {new}\n")
}

/// Append hunk `h` (header, lines, no-newline markers) to a patch.
fn push_hunk(out: &mut String, h: &Hunk) {
    out.push_str(&h.header);
    out.push('\n');
    for l in &h.lines {
        out.push(match l.kind {
            LineKind::Context => ' ',
            LineKind::Added => '+',
            LineKind::Removed => '-',
        });
        out.push_str(&l.text);
        out.push('\n');
        if l.no_newline {
            out.push_str(NO_NEWLINE);
            out.push('\n');
        }
    }
}

/// Parse a unified diff (`git diff` output) into files and hunks.
#[must_use]
pub fn parse(diff: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let (mut old_n, mut new_n) = (0usize, 0usize);
    let mut in_hunk = false;
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let path = rest.rsplit_once(" b/").map_or(rest, |(_, p)| p).to_string();
            files.push(FileDiff {
                path,
                binary: false,
                hunks: Vec::new(),
            });
            in_hunk = false;
            continue;
        }
        let Some(file) = files.last_mut() else {
            continue;
        };
        if line.starts_with("@@") {
            (old_n, new_n) = parse_hunk_header(line);
            in_hunk = true;
            file.hunks.push(Hunk {
                header: line.to_string(),
                lines: Vec::new(),
            });
        } else if in_hunk {
            let Some(hunk) = file.hunks.last_mut() else {
                continue;
            };
            let (kind, text) = match line.split_at_checked(1) {
                Some(("+", t)) => (LineKind::Added, t),
                Some(("-", t)) => (LineKind::Removed, t),
                Some((" ", t)) => (LineKind::Context, t),
                // "\ No newline at end of file" marks the line before it.
                Some(("\\", _)) => {
                    if let Some(last) = hunk.lines.last_mut() {
                        last.no_newline = true;
                    }
                    continue;
                }
                _ => continue,
            };
            let (old, new) = match kind {
                LineKind::Context => (Some(old_n), Some(new_n)),
                LineKind::Added => (None, Some(new_n)),
                LineKind::Removed => (Some(old_n), None),
            };
            if old.is_some() {
                old_n += 1;
            }
            if new.is_some() {
                new_n += 1;
            }
            hunk.lines.push(DiffLine {
                kind,
                text: text.to_string(),
                old,
                new,
                no_newline: false,
            });
        } else if line.starts_with("Binary files") || line.starts_with("GIT binary patch") {
            file.binary = true;
        }
    }
    files
}

/// `(old start, new start)` from an `@@ -a,b +c,d @@` header (1 when absent).
fn parse_hunk_header(h: &str) -> (usize, usize) {
    let num = |marker: char| {
        h.split_whitespace()
            .find_map(|w| w.strip_prefix(marker))
            .and_then(|w| w.split(',').next())
            .and_then(|w| w.parse().ok())
            .unwrap_or(1)
    };
    (num('-'), num('+'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = "diff --git a/src/a.rs b/src/a.rs\nindex 1..2 100644\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1,3 +1,3 @@ fn x\n keep\n-old\n+new\n tail\n@@ -10,1 +10,2 @@\n ctx\n+more\ndiff --git a/img.png b/img.png\nBinary files a/img.png and b/img.png differ\n";

    #[test]
    fn parses_files_hunks_and_line_numbers() {
        let files = parse(DIFF);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "src/a.rs");
        assert_eq!(files[0].hunks.len(), 2);
        let l = &files[0].hunks[0].lines;
        assert_eq!(
            (l[1].kind, l[1].old, l[1].new),
            (LineKind::Removed, Some(2), None)
        );
        assert_eq!(
            (l[2].kind, l[2].old, l[2].new),
            (LineKind::Added, None, Some(2))
        );
        assert_eq!(files[0].hunks[1].lines[1].new, Some(11));
        assert!(files[1].binary && files[1].hunks.is_empty());
    }

    #[test]
    fn render_maps_rows_and_navigates_hunks() {
        let r = Review::new("a..b", DIFF);
        let out = r.render();
        assert_eq!(out.text.lines().count(), out.targets.len());
        let hunks = out.hunk_rows();
        assert_eq!(hunks.len(), 2);
        assert_eq!(out.next_hunk(0), Some(hunks[0]));
        assert_eq!(out.next_hunk(hunks[1]), Some(hunks[0]));
        assert_eq!(out.prev_hunk(hunks[0]), Some(hunks[1]));
        assert_eq!(out.prev_hunk(hunks[1]), Some(hunks[0]));
    }

    #[test]
    fn comments_render_under_their_line_and_can_be_removed() {
        let mut r = Review::new("a..b", DIFF);
        assert!(r.set_comment("src/a.rs", Side::New, 2, "new", "why?"));
        let out = r.render();
        let row = out.text.lines().position(|l| l == "+new").unwrap();
        assert_eq!(out.text.lines().nth(row + 1), Some("    >> why?"));
        assert!(matches!(out.targets[row + 1], Target::Comment(0)));
        assert_eq!(r.comment_text("src/a.rs", Side::New, 2), Some("why?"));
        // Replace, then clear with blank text.
        assert!(r.set_comment("src/a.rs", Side::New, 2, "new", "better"));
        assert_eq!(r.comments.len(), 1);
        assert!(!r.set_comment("src/a.rs", Side::New, 2, "new", "  "));
        assert_eq!(r.comments.len(), 0);
    }

    #[test]
    fn line_text_finds_each_side() {
        let r = Review::new("a..b", DIFF);
        assert_eq!(r.line_text(0, Side::Old, 2), Some("old"));
        assert_eq!(r.line_text(0, Side::New, 2), Some("new"));
        assert_eq!(r.line_text(0, Side::New, 99), None);
    }

    #[test]
    fn markdown_and_json_round_trip() {
        let mut r = Review::new("main..feat", DIFF);
        r.set_comment("src/a.rs", Side::Old, 2, "old", "restore this");
        r.set_comment("src/a.rs", Side::New, 2, "new", "rename");
        let md = r.to_markdown();
        assert!(md.starts_with("# Review of `main..feat`"));
        assert!(md.contains("## `src/a.rs`"));
        assert!(md.contains("- **Line 2 (old)**: restore this"));
        assert!(md.contains("- **Line 2**: rename"));
        let mut back = Review::new("main..feat", DIFF);
        assert!(back.load_json(&r.to_json()));
        assert_eq!(back.comments, r.comments);
        assert!(!back.load_json("not json"));
        assert_eq!(
            Review::new("x", "").to_markdown(),
            "# Review of `x`\n\nNo comments.\n"
        );
    }

    #[test]
    fn store_name_is_a_safe_file_name() {
        assert_eq!(store_name("main..feature/x"), "main..feature_x");
        assert_eq!(store_name("stash@{0}"), "stash__0_");
        assert_eq!(store_name("../.."), "_..");
        assert_eq!(store_name(""), "review");
    }

    #[test]
    fn hunk_at_finds_the_enclosing_hunk() {
        let r = Review::new("a..b", DIFF).render();
        let rows = r.hunk_rows();
        assert_eq!(r.hunk_at(rows[0]), Some((0, 0)));
        assert_eq!(r.hunk_at(rows[0] + 2), Some((0, 0)));
        assert_eq!(r.hunk_at(rows[1] + 1), Some((0, 1)));
        assert_eq!(r.hunk_at(0), None);
        assert_eq!(r.hunk_at(rows[0] - 1), None, "the file heading is no hunk");
    }

    #[test]
    fn hunk_patch_rebuilds_a_one_hunk_patch() {
        let review = Review::new("a..b", DIFF);
        assert_eq!(
            review.hunk_patch(0, 1).unwrap(),
            "--- a/src/a.rs\n+++ b/src/a.rs\n@@ -10,1 +10,2 @@\n ctx\n+more\n"
        );
        assert!(review.hunk_patch(1, 0).is_none(), "binary file");
        assert!(review.hunk_patch(0, 5).is_none(), "no such hunk");
        let new_file = Review::new(
            "a..b",
            "diff --git a/n.txt b/n.txt\nnew file mode 100644\n--- /dev/null\n+++ b/n.txt\n@@ -0,0 +1,1 @@\n+hi\n",
        );
        assert!(
            new_file
                .hunk_patch(0, 0)
                .unwrap()
                .starts_with("--- /dev/null\n+++ b/n.txt\n")
        );
    }

    const NO_NL: &str = "diff --git a/n.txt b/n.txt\n--- a/n.txt\n+++ b/n.txt\n@@ -1,2 +1,2 @@\n keep\n-old\n\\ No newline at end of file\n+new\n\\ No newline at end of file\n";

    #[test]
    fn no_newline_markers_are_kept_rendered_and_patched() {
        let review = Review::new("a..b", NO_NL);
        let l = &review.files[0].hunks[0].lines;
        assert_eq!(
            l.iter().map(|x| x.no_newline).collect::<Vec<_>>(),
            [false, true, true]
        );
        let out = review.render();
        assert_eq!(out.text.lines().count(), out.targets.len());
        assert_eq!(out.text.matches("No newline at end of file").count(), 2);
        let patch = review.hunk_patch(0, 0).unwrap();
        assert!(
            patch.ends_with(
                "-old\n\\ No newline at end of file\n+new\n\\ No newline at end of file\n"
            )
        );
    }

    #[test]
    fn full_patch_holds_every_text_hunk_and_skips_binary() {
        let review = Review::new("stash@{0}", DIFF);
        let patch = review.full_patch().unwrap();
        assert_eq!(
            patch.matches("--- a/src/a.rs").count(),
            1,
            "one header per file"
        );
        assert_eq!(patch.matches("@@ -").count(), 2);
        assert!(!patch.contains("img.png"));
        assert!(Review::new("x", "").full_patch().is_none());
    }

    #[test]
    fn replace_diff_keeps_comments_and_drops_staged_hunks() {
        let mut review = Review::new("a..b", DIFF);
        review.set_comment("src/a.rs", Side::New, 11, "more", "keep me");
        review.replace_diff("diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -10,1 +10,2 @@\n ctx\n+more\n");
        assert_eq!(review.files.len(), 1);
        assert_eq!(review.files[0].hunks.len(), 1);
        assert_eq!(
            review.comment_text("src/a.rs", Side::New, 11),
            Some("keep me")
        );
    }

    #[test]
    fn side_by_side_pairs_removed_with_added_and_aligns_context() {
        let r = Review::new("a..b", DIFF);
        let sbs = r.side_by_side();
        let pairs: Vec<_> = sbs
            .rows
            .iter()
            .filter_map(|row| match &row.kind {
                PaneKind::Pair { left, right } => Some((
                    left.as_ref().map(|c| c.text.as_str()),
                    right.as_ref().map(|c| c.text.as_str()),
                )),
                _ => None,
            })
            .collect();
        assert_eq!(
            pairs,
            vec![
                (Some("keep"), Some("keep")),
                (Some("old"), Some("new")),
                (Some("tail"), Some("tail")),
                (Some("ctx"), Some("ctx")),
                (None, Some("more")),
            ]
        );
    }

    #[test]
    fn two_pane_geometry_maps_clicks_and_scrolls() {
        let sbs = Review::new("a..b", DIFF).side_by_side();
        assert_eq!(SideBySide::halves(81), (40, 40));
        assert_eq!(SideBySide::halves(0), (0, 0));
        // The `-old` / `+new` pair: each half maps to its own inline row.
        let i = sbs
            .rows
            .iter()
            .position(
                |r| matches!(&r.kind, PaneKind::Pair { left: Some(c), .. } if c.text == "old"),
            )
            .unwrap();
        let (a, b) = (
            sbs.inline_at(i, Half::Left).unwrap(),
            sbs.inline_at(i, Half::Right).unwrap(),
        );
        assert_eq!(b, a + 1);
        let out = Review::new("a..b", DIFF).render();
        assert!(matches!(
            out.targets[a],
            Target::Line {
                side: Side::Old,
                ..
            }
        ));
        assert!(matches!(
            out.targets[b],
            Target::Line {
                side: Side::New,
                ..
            }
        ));
        // A blank side falls back to the other cell.
        let more = sbs.rows.len() - 1;
        assert_eq!(
            sbs.inline_at(more, Half::Left),
            sbs.inline_at(more, Half::Right)
        );
        assert_eq!(sbs.inline_at(sbs.rows.len(), Half::Left), None);
        // top_for_row makes each row the one the view starts at.
        assert_eq!(sbs.top_for_row(0), 0);
        for idx in 0..sbs.rows.len() {
            assert!(sbs.row_for_inline(sbs.top_for_row(idx)) <= idx);
        }
        assert_eq!(sbs.row_for_inline(sbs.top_for_row(i)), i);
    }

    const LINES: &str =
        "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,4 +1,5 @@ fn f\n a\n-b\n-c\n+B\n+C\n+D\n e\n";

    fn pick(lines: &[usize]) -> LineSelection {
        LineSelection {
            file: 0,
            hunk: 0,
            lines: lines.to_vec(),
        }
    }

    #[test]
    fn staging_lines_keeps_unselected_removals_as_context() {
        let r = Review::new("r", LINES);
        // -b and +B only: -c stays (context), +C and +D are left out.
        let p = r.lines_patch(&pick(&[1, 3]), LineMode::Stage).unwrap();
        assert_eq!(
            p,
            "--- a/f\n+++ b/f\n@@ -1,4 +1,4 @@ fn f\n a\n-b\n c\n+B\n e\n"
        );
        // Everything selected is the hunk itself.
        let all = r
            .lines_patch(&pick(&[1, 2, 3, 4, 5]), LineMode::Stage)
            .unwrap();
        assert_eq!(all, r.hunk_patch(0, 0).unwrap());
    }

    #[test]
    fn discarding_lines_keeps_unselected_additions_as_context() {
        let r = Review::new("r", LINES);
        // Undo only +B: the other additions stay, the removals are not undone.
        let p = r.lines_patch(&pick(&[3]), LineMode::Discard).unwrap();
        assert_eq!(
            p,
            "--- a/f\n+++ b/f\n@@ -1,4 +1,5 @@ fn f\n a\n+B\n C\n D\n e\n"
        );
        let all = r
            .lines_patch(&pick(&[1, 2, 3, 4, 5]), LineMode::Discard)
            .unwrap();
        assert_eq!(all, r.hunk_patch(0, 0).unwrap());
    }

    #[test]
    fn line_patches_handle_whole_file_hunks() {
        let added = "diff --git a/n b/n\n--- /dev/null\n+++ b/n\n@@ -0,0 +1,3 @@\n+x\n+y\n+z\n";
        let r = Review::new("r", added);
        let p = r.lines_patch(&pick(&[1]), LineMode::Stage).unwrap();
        assert_eq!(p, "--- /dev/null\n+++ b/n\n@@ -0,0 +1,1 @@\n+y\n");
        // Discarding one added line of a new file keeps the file.
        let d = r.lines_patch(&pick(&[1]), LineMode::Discard).unwrap();
        assert_eq!(d, "--- a/n\n+++ b/n\n@@ -1,2 +1,3 @@\n x\n+y\n z\n");
        let gone = "diff --git a/g b/g\n--- a/g\n+++ /dev/null\n@@ -1,2 +0,0 @@\n-p\n-q\n";
        let r = Review::new("r", gone);
        // Staging one removal of a deleted file keeps the other as context.
        let p = r.lines_patch(&pick(&[0]), LineMode::Stage).unwrap();
        assert_eq!(p, "--- a/g\n+++ b/g\n@@ -1,2 +1,1 @@\n-p\n q\n");
        // All of them is still the deletion.
        let p = r.lines_patch(&pick(&[0, 1]), LineMode::Stage).unwrap();
        assert_eq!(p, "--- a/g\n+++ /dev/null\n@@ -1,2 +0,0 @@\n-p\n-q\n");
    }

    #[test]
    fn a_missing_final_newline_cannot_be_split_from_its_partner() {
        let d = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n a\n-b\n\\ No newline at end of file\n+b\n";
        let r = Review::new("r", d);
        // Only +b: -b would stay as context while still carrying the marker.
        assert_eq!(
            r.lines_patch(&pick(&[2]), LineMode::Stage),
            Err(SelectError::NoNewline)
        );
        // Only -b when discarding: +b would stay, which is fine to build (git
        // judges the result), because the marker line itself is selected.
        assert!(r.lines_patch(&pick(&[1]), LineMode::Discard).is_ok());
        // Both together keep the marker with its line.
        let both = r.lines_patch(&pick(&[1, 2]), LineMode::Stage).unwrap();
        assert_eq!(both, r.hunk_patch(0, 0).unwrap());
        assert!(both.contains("\\ No newline at end of file\n"));
    }

    #[test]
    fn select_rows_finds_changed_lines_in_one_hunk() {
        let r = Review::new("r", LINES);
        let out = r.render();
        let row = |s: &str| out.text.lines().position(|l| l == s).unwrap();
        let sel = r.select_rows(&out, row("-b"), row("+C")).unwrap();
        assert_eq!(sel, pick(&[1, 2, 3, 4]));
        // Context and heading rows are ignored; none left is an error.
        assert_eq!(
            r.select_rows(&out, 0, row("-b") - 1),
            Err(SelectError::NoLines)
        );
        assert_eq!(
            r.select_rows(&out, row(" a"), row(" a")),
            Err(SelectError::NoLines)
        );
        let two = Review::new("r", DIFF);
        let out = two.render();
        assert_eq!(
            two.select_rows(&out, 0, out.targets.len() - 1),
            Err(SelectError::SpansHunks)
        );
    }

    #[test]
    fn wrap_text_splits_by_display_width() {
        assert_eq!(wrap_text("", 5), vec![(0, 0)]);
        assert_eq!(wrap_text("abc", 5), vec![(0, 3)]);
        assert_eq!(wrap_text("abcdefgh", 3), vec![(0, 3), (3, 6), (6, 8)]);
        // Wide characters take two columns and are never split.
        assert_eq!(wrap_text("日本語", 5), vec![(0, 2), (2, 3)]);
        // Nothing fits in zero columns: one character per piece.
        assert_eq!(wrap_text("ab", 0), vec![(0, 1), (1, 2)]);
    }

    #[test]
    fn wrapped_rows_are_as_tall_as_their_longer_side() {
        let long = "x".repeat(30);
        let d = format!(
            "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,3 +1,3 @@\n keep\n-{long}\n+short\n tail\n"
        );
        let sbs = Review::new("r", &d).side_by_side();
        let (l, r) = SideBySide::halves(20); // 9 and 10 columns: 4 and 5 for text
        let heights: Vec<usize> = sbs.rows.iter().map(|row| row.height(l, r)).collect();
        // title, blank-less heading rows are 1; the long pair takes ceil(30/4).
        assert!(heights.contains(&8), "{heights:?}");
        assert_eq!(sbs.row_height(0, 20, false), 1);
        let pair = heights.iter().position(|&h| h == 8).unwrap();
        assert_eq!(sbs.row_height(pair, 20, true), 8);
        // row_at walks lines, last_first fills the view to the end.
        let h = |i: usize| sbs.row_height(i, 20, true);
        assert_eq!(sbs.row_at(pair, 0, h), Some((pair, 0)));
        assert_eq!(sbs.row_at(pair, 7, h), Some((pair, 7)));
        assert_eq!(sbs.row_at(pair, 8, h), Some((pair + 1, 0)));
        assert_eq!(sbs.row_at(0, 10_000, h), None);
        assert_eq!(sbs.last_first(100, h), 0);
        let lf = sbs.last_first(9, h);
        assert!((lf..sbs.rows.len()).map(h).sum::<usize>() <= 9);
        assert!(lf == 0 || (lf - 1..sbs.rows.len()).map(h).sum::<usize>() > 9);
        // The view keeps a tall cursor row in sight.
        let cursor = sbs.rows[pair].last;
        let first = sbs.view_start(0, cursor, 9, h);
        assert!((first..=pair).map(h).sum::<usize>() <= 9);
        assert!(first <= pair);
    }

    #[test]
    fn paired_lines_carry_word_level_marks() {
        let d = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,3 +1,3 @@\n keep\n-the quick fox\n+the slow fox\n x\n";
        let sbs = Review::new("r", d).side_by_side();
        let (l, r) = sbs
            .rows
            .iter()
            .find_map(|row| match &row.kind {
                PaneKind::Pair {
                    left: Some(l),
                    right: Some(r),
                } if l.kind == LineKind::Removed => Some((l.clone(), r.clone())),
                _ => None,
            })
            .unwrap();
        // "quick" is chars 4..9 of the old line, "slow" 4..8 of the new one.
        assert_eq!(l.marks, vec![(4, 9)]);
        assert_eq!(r.marks, vec![(4, 8)]);
        // Context cells and lines that differ completely carry none.
        let whole = Review::new("r", DIFF).side_by_side();
        for row in &whole.rows {
            if let PaneKind::Pair { left, right } = &row.kind {
                for c in [left, right].into_iter().flatten() {
                    assert!(c.marks.is_empty(), "{c:?}");
                }
            }
        }
        // An unpaired addition has no counterpart to differ from.
        let extra = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,1 +1,2 @@\n a\n+b\n";
        let rows = Review::new("r", extra).side_by_side().rows;
        assert!(rows.iter().all(|r| !matches!(&r.kind,
            PaneKind::Pair { right: Some(c), .. } if !c.marks.is_empty())));
    }

    #[test]
    fn side_by_side_leaves_blank_cells_for_uneven_runs() {
        let diff = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,3 +1,1 @@\n-a\n-b\n-c\n+x\n";
        let sbs = Review::new("r", diff).side_by_side();
        let pairs: Vec<_> = sbs
            .rows
            .iter()
            .filter_map(|row| match &row.kind {
                PaneKind::Pair { left, right } => Some((
                    left.as_ref().map(|c| c.number),
                    right.as_ref().map(|c| c.number),
                )),
                _ => None,
            })
            .collect();
        assert_eq!(
            pairs,
            vec![(Some(1), Some(1)), (Some(2), None), (Some(3), None)]
        );
    }

    #[test]
    fn side_by_side_rows_index_the_inline_buffer() {
        let mut r = Review::new("a..b", DIFF);
        r.set_comment("src/a.rs", Side::New, 2, "new", "why?");
        r.set_comment("src/a.rs", Side::Old, 2, "old", "gone?");
        let out = r.render();
        let sbs = r.side_by_side();
        for row in &sbs.rows {
            assert!(row.first <= row.last && row.last < out.targets.len());
            match &row.kind {
                PaneKind::Heading => assert!(matches!(
                    out.targets[row.first],
                    Target::None | Target::File(_) | Target::Hunk(..)
                )),
                PaneKind::Comment => {
                    assert!(matches!(out.targets[row.first], Target::Comment(_)));
                    assert_eq!(out.text.lines().nth(row.first), Some(row.text.as_str()));
                }
                PaneKind::Pair { .. } => {
                    assert!(matches!(out.targets[row.first], Target::Line { .. }));
                }
            }
        }
        assert_eq!(
            sbs.rows
                .iter()
                .filter(|r| r.kind == PaneKind::Comment)
                .count(),
            2
        );
        assert_eq!(sbs.row_for_inline(0), 0);
        let hunk_row = out.hunk_rows()[1];
        let at = sbs.row_for_inline(hunk_row);
        assert_eq!(sbs.rows[at].first, hunk_row);
    }

    #[test]
    fn patch_counts_name_hunks_and_files() {
        assert_eq!(Review::new("a..b", DIFF).patch_counts(), (2, 1));
        let none = Review::new(
            "a..b",
            "diff --git a/i b/i\nBinary files a/i and b/i differ\n",
        );
        assert_eq!(none.patch_counts(), (0, 0));
    }
}
