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
    git_stdout(dir, &["show", &format!("HEAD:{rel_path}")])
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
