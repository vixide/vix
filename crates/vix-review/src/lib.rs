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
}
