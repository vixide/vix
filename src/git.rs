#![allow(clippy::pedantic)] // folded subcrate: kept at its original (non-pedantic) lint level
//! Git integration for Vix.
//!
//! Two layers:
//!
//! - **Pure logic** (unit-tested, no I/O): [`parse_status`] turns
//!   `git status --porcelain` output into [`FileStatus`] rows, and [`diff_marks`]
//!   computes per-line editor-gutter [`LineMark`]s between the committed text and
//!   the current buffer (via the `similar` crate).
//! - **Runners** (shell out to the `git` CLI): [`is_repo`], [`branch`],
//!   [`status`], [`head_blob`], and the staging/commit helpers. Using the user's
//!   own `git` means credential helpers, SSH agents, and hooks all behave exactly
//!   as on the command line.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]

use std::path::Path;
use std::process::Command;

/// A kind of change to a file (in the index or the working tree).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Change {
    /// Newly tracked (staged add).
    Added,
    /// Contents changed.
    Modified,
    /// Removed.
    Deleted,
    /// Renamed.
    Renamed,
    /// Not tracked by git.
    Untracked,
    /// Unmerged / conflicted.
    Conflicted,
}

impl Change {
    /// A single-letter code for compact UI (e.g. file-explorer badges).
    #[must_use]
    pub fn letter(self) -> char {
        match self {
            Change::Added => 'A',
            Change::Modified => 'M',
            Change::Deleted => 'D',
            Change::Renamed => 'R',
            Change::Untracked => '?',
            Change::Conflicted => 'U',
        }
    }

    fn from_code(c: char) -> Option<Change> {
        match c {
            'A' => Some(Change::Added),
            // 'C' (copied) is treated like a modification.
            'M' | 'C' => Some(Change::Modified),
            'D' => Some(Change::Deleted),
            'R' => Some(Change::Renamed),
            '?' => Some(Change::Untracked),
            'U' => Some(Change::Conflicted),
            _ => None,
        }
    }
}

/// One changed path from `git status`, with its staged (index) and unstaged
/// (working-tree) change, if any.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FileStatus {
    /// Path relative to the repository root.
    pub path: String,
    /// The staged (index) change, if any.
    pub staged: Option<Change>,
    /// The unstaged (working-tree) change, if any.
    pub unstaged: Option<Change>,
}

impl FileStatus {
    /// Whether the path has staged changes.
    #[must_use]
    pub fn is_staged(&self) -> bool {
        self.staged.is_some()
    }

    /// The most representative change for a one-letter badge: the staged change
    /// if present, else the unstaged one.
    #[must_use]
    pub fn primary(&self) -> Option<Change> {
        self.staged.or(self.unstaged)
    }
}

/// Parse `git status --porcelain` (v1) output into one [`FileStatus`] per path.
///
/// Each line is `XY <path>`, where `X` is the index status and `Y` the
/// working-tree status (`??` for untracked). Rename lines (`R  old -> new`) are
/// recorded under the new path.
#[must_use]
pub fn parse_status(output: &str) -> Vec<FileStatus> {
    let mut out = Vec::new();
    for line in output.lines() {
        if line.len() < 3 {
            continue;
        }
        let bytes = line.as_bytes();
        let x = bytes[0] as char;
        let y = bytes[1] as char;
        let rest = line[3..].trim_end();
        // Rename/copy show "old -> new"; record the destination.
        let path = match rest.rsplit_once(" -> ") {
            Some((_, new)) => new.to_string(),
            None => rest.to_string(),
        };
        if path.is_empty() {
            continue;
        }
        let (staged, unstaged) = if x == '?' && y == '?' {
            (None, Some(Change::Untracked))
        } else {
            (Change::from_code(x), Change::from_code(y))
        };
        out.push(FileStatus { path, staged, unstaged });
    }
    out
}

/// A per-line change marker for the editor's diff gutter.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LineMark {
    /// The line is new since the committed version.
    Added,
    /// The line replaces a committed line (content changed).
    Modified,
    /// One or more committed lines were removed at this position.
    Deleted,
}

/// Compute per-line gutter marks between the committed text (`head`) and the
/// current buffer (`current`), keyed by **current** line index (0-based).
///
/// Uses a line diff: inserted runs are [`LineMark::Added`], replaced runs are
/// [`LineMark::Modified`], and a deletion records a [`LineMark::Deleted`] at the
/// current line where the removed text used to be.
#[must_use]
pub fn diff_marks(head: &str, current: &str) -> Vec<(usize, LineMark)> {
    use similar::{Algorithm, DiffOp, TextDiff};

    let diff = TextDiff::configure().algorithm(Algorithm::Myers).diff_lines(head, current);
    let current_lines = current.lines().count();
    let mut marks = Vec::new();
    for op in diff.ops() {
        match *op {
            DiffOp::Insert { new_index, new_len, .. } => {
                for i in new_index..new_index + new_len {
                    marks.push((i, LineMark::Added));
                }
            }
            DiffOp::Replace { new_index, new_len, .. } => {
                for i in new_index..new_index + new_len {
                    marks.push((i, LineMark::Modified));
                }
            }
            DiffOp::Delete { new_index, .. } => {
                let at = new_index.min(current_lines.saturating_sub(1));
                marks.push((at, LineMark::Deleted));
            }
            DiffOp::Equal { .. } => {}
        }
    }
    marks
}

/// A contiguous changed region between the committed text and the current
/// buffer, for hunk navigation and revert.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Hunk {
    /// First current-buffer line (0-based) the hunk covers.
    pub current_start: usize,
    /// One past the last current line the hunk covers. Equal to `current_start`
    /// for a pure deletion (nothing remains in the current buffer there).
    pub current_end: usize,
    /// The committed (HEAD) text for this region, with line endings, used to
    /// restore the hunk on revert. Empty for a pure addition.
    pub head_text: String,
}

impl Hunk {
    /// Whether current line `line` falls within this hunk (a pure deletion is
    /// matched at its single anchor line).
    #[must_use]
    pub fn contains(&self, line: usize) -> bool {
        if self.current_start == self.current_end {
            line == self.current_start
        } else {
            line >= self.current_start && line < self.current_end
        }
    }
}

/// Group the line diff between committed text (`head`) and the current buffer
/// into [`Hunk`]s — maximal runs of adjacent changed lines — in current-line
/// order. Pairs with [`diff_marks`] (which colors individual gutter lines).
#[must_use]
pub fn hunks(head: &str, current: &str) -> Vec<Hunk> {
    use similar::{Algorithm, DiffOp, TextDiff};

    let diff = TextDiff::configure().algorithm(Algorithm::Myers).diff_lines(head, current);
    let head_lines: Vec<&str> = head.split_inclusive('\n').collect();
    let mut out: Vec<Hunk> = Vec::new();
    // Accumulated (new_lo, new_hi, old_lo, old_hi) for the run in progress.
    let mut run: Option<(usize, usize, usize, usize)> = None;

    let flush = |run: &mut Option<(usize, usize, usize, usize)>, out: &mut Vec<Hunk>| {
        if let Some((nlo, nhi, olo, ohi)) = run.take() {
            let head_text = head_lines.get(olo..ohi).map(<[&str]>::concat).unwrap_or_default();
            out.push(Hunk { current_start: nlo, current_end: nhi, head_text });
        }
    };

    for op in diff.ops() {
        let (ni, nl, oi, ol) = match *op {
            DiffOp::Equal { .. } => {
                flush(&mut run, &mut out);
                continue;
            }
            DiffOp::Insert { new_index, new_len, old_index, .. } => (new_index, new_len, old_index, 0),
            DiffOp::Delete { new_index, old_index, old_len, .. } => (new_index, 0, old_index, old_len),
            DiffOp::Replace { new_index, new_len, old_index, old_len, .. } => {
                (new_index, new_len, old_index, old_len)
            }
        };
        run = Some(match run {
            Some((nlo, nhi, olo, ohi)) => {
                (nlo.min(ni), nhi.max(ni + nl), olo.min(oi), ohi.max(oi + ol))
            }
            None => (ni, ni + nl, oi, oi + ol),
        });
    }
    flush(&mut run, &mut out);
    out
}

/// One line's `git blame` attribution: the short commit hash, author name,
/// `YYYY-MM-DD` authored date, and the commit summary.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct BlameLine {
    /// Abbreviated commit hash (8 hex chars), or all-zero for uncommitted lines.
    pub hash: String,
    /// Author name (e.g. `Not Committed Yet` for unsaved/unstaged lines).
    pub author: String,
    /// Authored date as `YYYY-MM-DD`, in the author's own time zone.
    pub date: String,
    /// First line of the commit message.
    pub summary: String,
}

impl BlameLine {
    /// Whether this line is not yet committed (zero hash).
    #[must_use]
    pub fn is_uncommitted(&self) -> bool {
        self.hash.chars().all(|c| c == '0')
    }
}

/// Parse one entry of `git blame --line-porcelain` output into a [`BlameLine`].
/// Returns `None` if no header (hash) line is present.
#[must_use]
pub fn parse_blame_porcelain(output: &str) -> Option<BlameLine> {
    let mut hash = None;
    let mut author = String::new();
    let mut summary = String::new();
    let mut time: i64 = 0;
    let mut tz: i32 = 0;
    for line in output.lines() {
        if hash.is_none() {
            // First line: "<40-hex> <orig> <final> [<count>]".
            if let Some(h) = line.split(' ').next().filter(|h| h.len() == 40) {
                hash = Some(h.chars().take(8).collect::<String>());
            }
            continue;
        }
        if let Some(name) = line.strip_prefix("author ") {
            author = name.to_string();
        } else if let Some(secs) = line.strip_prefix("author-time ") {
            time = secs.trim().parse().unwrap_or(0);
        } else if let Some(off) = line.strip_prefix("author-tz ") {
            tz = parse_tz_offset(off.trim());
        } else if let Some(s) = line.strip_prefix("summary ") {
            summary = s.to_string();
        }
    }
    let hash = hash?;
    Some(BlameLine { hash, author, date: epoch_to_date(time, tz), summary })
}

/// Parse a git tz offset like `+0200` / `-0500` into seconds east of UTC.
fn parse_tz_offset(tz: &str) -> i32 {
    let sign = if tz.starts_with('-') { -1 } else { 1 };
    let digits: String = tz.chars().filter(char::is_ascii_digit).collect();
    if digits.len() < 4 {
        return 0;
    }
    let hours: i32 = digits[0..2].parse().unwrap_or(0);
    let mins: i32 = digits[2..4].parse().unwrap_or(0);
    sign * (hours * 3600 + mins * 60)
}

/// Convert a Unix `secs` (+ `tz_offset` seconds east of UTC) to a `YYYY-MM-DD`
/// calendar date, via Howard Hinnant's `civil_from_days` algorithm.
#[must_use]
fn epoch_to_date(secs: i64, tz_offset: i32) -> String {
    let days = (secs + i64::from(tz_offset)).div_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = y + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// `git blame` for a single 1-based line of a repo-relative path, parsed into a
/// [`BlameLine`]. `None` when not a repo, the path is untracked, or on error.
#[must_use]
pub fn blame_line(dir: &Path, rel_path: &str, line: usize) -> Option<BlameLine> {
    let spec = format!("{line},{line}");
    let out = git_stdout(dir, &["blame", "--line-porcelain", "-L", &spec, "--", rel_path])?;
    parse_blame_porcelain(&out)
}

// ----- runners (shell out to `git`) --------------------------------------

fn git(dir: &Path, args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new("git").current_dir(dir).args(args).output()
}

/// Run `git` and return trimmed stdout on success, or `None` on any failure.
fn git_stdout(dir: &Path, args: &[&str]) -> Option<String> {
    let out = git(dir, args).ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
    } else {
        None
    }
}

/// Whether `dir` is inside a git working tree.
#[must_use]
pub fn is_repo(dir: &Path) -> bool {
    git_stdout(dir, &["rev-parse", "--is-inside-work-tree"]).as_deref() == Some("true")
}

/// The current branch name, or `None` when detached / not a repo. A detached
/// HEAD yields the short commit hash via the caller's discretion (here `None`).
#[must_use]
pub fn branch(dir: &Path) -> Option<String> {
    let name = git_stdout(dir, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if name == "HEAD" {
        // Detached: fall back to a short hash.
        git_stdout(dir, &["rev-parse", "--short", "HEAD"])
    } else {
        Some(name)
    }
}

/// Whether the working tree has any staged or unstaged changes (incl. untracked).
#[must_use]
pub fn is_dirty(dir: &Path) -> bool {
    git_stdout(dir, &["status", "--porcelain"]).is_some_and(|s| !s.is_empty())
}

/// The changed files reported by `git status --porcelain`.
#[must_use]
pub fn status(dir: &Path) -> Vec<FileStatus> {
    git_stdout(dir, &["status", "--porcelain"]).map(|s| parse_status(&s)).unwrap_or_default()
}

/// The committed (`HEAD`) contents of a repo-relative path, or `None` if the path
/// is not in HEAD (newly added/untracked) or on any error.
#[must_use]
pub fn head_blob(dir: &Path, rel_path: &str) -> Option<String> {
    // Use the raw (untrimmed) output: a blob's trailing newline is significant
    // for line-accurate diffing, gutter marks, and hunk revert.
    let out = git(dir, &["show", &format!("HEAD:{rel_path}")]).ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Stage a path (`git add -- <path>`). Returns whether the command succeeded.
#[must_use]
pub fn stage(dir: &Path, rel_path: &str) -> bool {
    git(dir, &["add", "--", rel_path]).is_ok_and(|o| o.status.success())
}

/// Unstage a path (`git restore --staged -- <path>`). Returns success.
#[must_use]
pub fn unstage(dir: &Path, rel_path: &str) -> bool {
    git(dir, &["restore", "--staged", "--", rel_path]).is_ok_and(|o| o.status.success())
}

/// The number of commits reachable from HEAD (`git rev-list --count HEAD`), or
/// `None` when not a repo or there are no commits yet.
#[must_use]
pub fn commit_count(dir: &Path) -> Option<u64> {
    git_stdout(dir, &["rev-list", "--count", "HEAD"])?.trim().parse().ok()
}

/// The local branch names, current branch first when it can be determined.
#[must_use]
pub fn local_branches(dir: &Path) -> Vec<String> {
    let out = git_stdout(dir, &["branch", "--format=%(refname:short)"]).unwrap_or_default();
    let mut names: Vec<String> = out.lines().map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect();
    if let Some(cur) = branch(dir) {
        if let Some(pos) = names.iter().position(|n| *n == cur) {
            names.swap(0, pos);
        }
    }
    names
}

/// Check out an existing branch (`git switch <branch>`). Returns `Ok(())` on
/// success, or the captured stderr on failure (e.g. a conflicting dirty tree).
///
/// # Errors
/// Returns the trimmed stderr text when `git switch` exits non-zero or cannot run.
pub fn checkout(dir: &Path, branch: &str) -> Result<(), String> {
    match git(dir, &["switch", branch]) {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => Err(e.to_string()),
    }
}

/// Create a new branch and switch to it (`git switch -c <name>`). Returns
/// `Ok(())` on success, or the captured stderr on failure (e.g. the name already
/// exists or is invalid).
///
/// # Errors
/// Returns the trimmed stderr text when `git switch -c` exits non-zero or cannot run.
pub fn create_branch(dir: &Path, name: &str) -> Result<(), String> {
    match git(dir, &["switch", "-c", name]) {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => Err(e.to_string()),
    }
}

/// Commit the staged changes with `message`. Returns `Ok(())` on success, or the
/// captured stderr on failure (e.g. nothing staged, hook rejected).
///
/// # Errors
/// Returns the trimmed stderr text when `git commit` exits non-zero or cannot run.
pub fn commit(dir: &Path, message: &str) -> Result<(), String> {
    match git(dir, &["commit", "-m", message]) {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hunks_groups_a_modified_line() {
        let h = hunks("a\nb\nc\n", "a\nB\nc\n");
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].current_start, 1);
        assert_eq!(h[0].current_end, 2);
        assert_eq!(h[0].head_text, "b\n");
        assert!(h[0].contains(1));
        assert!(!h[0].contains(0));
    }

    #[test]
    fn hunks_addition_has_empty_head_text() {
        let h = hunks("a\nc\n", "a\nb1\nb2\nc\n");
        assert_eq!(h.len(), 1);
        assert_eq!((h[0].current_start, h[0].current_end), (1, 3));
        assert_eq!(h[0].head_text, ""); // pure addition restores to nothing
    }

    #[test]
    fn hunks_deletion_anchors_a_zero_width_range() {
        let h = hunks("a\nb\nc\n", "a\nc\n");
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].current_start, h[0].current_end); // pure deletion
        assert_eq!(h[0].head_text, "b\n"); // reverting re-inserts the committed line
        assert!(h[0].contains(h[0].current_start));
    }

    #[test]
    fn hunks_finds_multiple_regions() {
        let h = hunks("a\nb\nc\nd\ne\n", "A\nb\nc\nD\ne\n");
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].current_start, 0);
        assert_eq!(h[1].current_start, 3);
    }

    #[test]
    fn hunks_identical_text_is_empty() {
        assert!(hunks("x\ny\n", "x\ny\n").is_empty());
    }

    #[test]
    fn parse_status_reads_xy_codes_and_paths() {
        let out = " M src/app.rs\nA  src/new.rs\n?? notes.txt\nMM both.rs\nD  gone.rs\n";
        let rows = parse_status(out);
        assert_eq!(rows.len(), 5);

        assert_eq!(rows[0].path, "src/app.rs");
        assert_eq!(rows[0].staged, None);
        assert_eq!(rows[0].unstaged, Some(Change::Modified));

        assert_eq!(rows[1].path, "src/new.rs");
        assert_eq!(rows[1].staged, Some(Change::Added));
        assert_eq!(rows[1].unstaged, None);

        assert_eq!(rows[2].path, "notes.txt");
        assert_eq!(rows[2].primary(), Some(Change::Untracked));

        assert_eq!(rows[3].staged, Some(Change::Modified));
        assert_eq!(rows[3].unstaged, Some(Change::Modified));

        assert_eq!(rows[4].path, "gone.rs");
        assert_eq!(rows[4].staged, Some(Change::Deleted));
    }

    #[test]
    fn parse_status_records_rename_destination() {
        let rows = parse_status("R  old/name.rs -> new/name.rs\n");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, "new/name.rs");
        assert_eq!(rows[0].staged, Some(Change::Renamed));
    }

    #[test]
    fn parse_status_skips_blank_and_short_lines() {
        assert!(parse_status("\n\nx\n").is_empty());
    }

    #[test]
    fn diff_marks_flags_added_modified_and_deleted() {
        let head = "alpha\nbeta\ngamma\n";
        // Modify line 2, add a new line 3, keep the rest.
        let current = "alpha\nBETA\ngamma\ndelta\n";
        let marks = diff_marks(head, current);
        assert!(marks.contains(&(1, LineMark::Modified)), "line 2 modified: {marks:?}");
        assert!(marks.contains(&(3, LineMark::Added)), "line 4 added: {marks:?}");
    }

    #[test]
    fn diff_marks_flags_deletion() {
        let head = "a\nb\nc\n";
        let current = "a\nc\n"; // removed "b"
        let marks = diff_marks(head, current);
        assert!(
            marks.iter().any(|&(_, m)| m == LineMark::Deleted),
            "a deletion is marked: {marks:?}"
        );
    }

    #[test]
    fn diff_marks_empty_when_identical() {
        assert!(diff_marks("x\ny\n", "x\ny\n").is_empty());
    }

    #[test]
    fn parse_blame_porcelain_extracts_fields() {
        let out = "1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b 12 12 1\n\
            author Ada Lovelace\n\
            author-mail <ada@example.com>\n\
            author-time 1700000000\n\
            author-tz +0000\n\
            committer Ada Lovelace\n\
            summary Add the analytical engine\n\
            filename engine.rs\n\
            \tlet x = 1;\n";
        let b = parse_blame_porcelain(out).expect("parsed");
        assert_eq!(b.hash, "1a2b3c4d");
        assert_eq!(b.author, "Ada Lovelace");
        assert_eq!(b.date, "2023-11-14");
        assert_eq!(b.summary, "Add the analytical engine");
        assert!(!b.is_uncommitted());
    }

    #[test]
    fn parse_blame_porcelain_flags_uncommitted() {
        let out = "0000000000000000000000000000000000000000 1 1 1\n\
            author Not Committed Yet\n\
            author-time 1700000000\n\
            author-tz +0000\n\
            summary Version of engine.rs from engine.rs\n\
            \tlet y = 2;\n";
        let b = parse_blame_porcelain(out).expect("parsed");
        assert!(b.is_uncommitted());
        assert_eq!(b.author, "Not Committed Yet");
    }

    #[test]
    fn parse_blame_porcelain_none_without_header() {
        assert!(parse_blame_porcelain("not a blame\n").is_none());
    }

    #[test]
    fn epoch_to_date_applies_tz_offset() {
        // 1700000000 is 2023-11-14 22:13:20 UTC; +0200 rolls it into the 15th.
        assert_eq!(epoch_to_date(1_700_000_000, 0), "2023-11-14");
        assert_eq!(epoch_to_date(1_700_000_000, 2 * 3600), "2023-11-15");
        assert_eq!(epoch_to_date(0, 0), "1970-01-01");
    }

    #[test]
    fn change_letters() {
        assert_eq!(Change::Modified.letter(), 'M');
        assert_eq!(Change::Untracked.letter(), '?');
    }

    // Runner smoke test against this very repository. Ignored by default because
    // it assumes `git` is installed and the tests run inside the work tree.
    #[test]
    #[ignore = "needs git and an in-tree checkout"]
    fn runners_report_branch_in_this_repo() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert!(is_repo(dir));
        assert!(branch(dir).is_some());
    }
}
