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

#[cfg(feature = "process")]
use std::ffi::OsString;
#[cfg(feature = "process")]
use std::path::Path;
#[cfg(feature = "process")]
use std::path::PathBuf;
#[cfg(feature = "process")]
use std::process::Command;
#[cfg(feature = "process")]
use std::sync::{Mutex, OnceLock};

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
    /// Path relative to the directory [`status`] was asked about (the
    /// workspace root), not to the repository root (T650): a changed file
    /// outside that directory but inside the repository reads `../other`.
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
        out.push(FileStatus {
            path,
            staged,
            unstaged,
        });
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

/// Buffers longer than this get no gutter diff marks (T574).
const GUTTER_DIFF_MAX_LINES: usize = 50_000;

/// Time budget for one gutter diff before `similar` falls back to a coarse
/// result (T574).
const GUTTER_DIFF_DEADLINE: std::time::Duration = std::time::Duration::from_millis(40);

/// Compute per-line gutter marks between the committed text (`head`) and the
/// current buffer (`current`), keyed by **current** line index (0-based).
///
/// Uses a line diff: inserted runs are [`LineMark::Added`], replaced runs are
/// [`LineMark::Modified`], and a deletion records a [`LineMark::Deleted`] at the
/// current line where the removed text used to be.
#[must_use]
pub fn diff_marks(head: &str, current: &str) -> Vec<(usize, LineMark)> {
    use similar::{Algorithm, DiffOp, TextDiff};

    let current_lines = current.lines().count();
    // T574: this runs on the UI thread every refresh. Skip the gutter for
    // huge buffers and put a deadline on Myers (O((N+M)·D)) so a heavily
    // rewritten file degrades to a coarse diff instead of stalling a frame.
    if current_lines.max(head.lines().count()) > GUTTER_DIFF_MAX_LINES {
        return Vec::new();
    }
    let diff = TextDiff::configure()
        .algorithm(Algorithm::Myers)
        .timeout(GUTTER_DIFF_DEADLINE)
        .diff_lines(head, current);
    let mut marks = Vec::new();
    for op in diff.ops() {
        match *op {
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                for i in new_index..new_index + new_len {
                    marks.push((i, LineMark::Added));
                }
            }
            DiffOp::Replace {
                new_index, new_len, ..
            } => {
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
    /// First HEAD line (0-based) the hunk's committed text begins at — used to
    /// locate the region when staging the hunk into the index.
    pub head_start: usize,
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

    let diff = TextDiff::configure()
        .algorithm(Algorithm::Myers)
        .diff_lines(head, current);
    let head_lines: Vec<&str> = head.split_inclusive('\n').collect();
    let mut out: Vec<Hunk> = Vec::new();
    // Accumulated (new_lo, new_hi, old_lo, old_hi) for the run in progress.
    let mut run: Option<(usize, usize, usize, usize)> = None;

    let flush = |run: &mut Option<(usize, usize, usize, usize)>, out: &mut Vec<Hunk>| {
        if let Some((nlo, nhi, olo, ohi)) = run.take() {
            let head_text = head_lines
                .get(olo..ohi)
                .map(<[&str]>::concat)
                .unwrap_or_default();
            out.push(Hunk {
                current_start: nlo,
                current_end: nhi,
                head_start: olo,
                head_text,
            });
        }
    };

    for op in diff.ops() {
        let (ni, nl, oi, ol) = match *op {
            DiffOp::Equal { .. } => {
                flush(&mut run, &mut out);
                continue;
            }
            DiffOp::Insert {
                new_index,
                new_len,
                old_index,
                ..
            } => (new_index, new_len, old_index, 0),
            DiffOp::Delete {
                new_index,
                old_index,
                old_len,
                ..
            } => (new_index, 0, old_index, old_len),
            DiffOp::Replace {
                new_index,
                new_len,
                old_index,
                old_len,
                ..
            } => (new_index, new_len, old_index, old_len),
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
    Some(BlameLine {
        hash,
        author,
        date: epoch_to_date(time, tz),
        summary,
    })
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
/// calendar date.
///
/// Delegates to `vix-civil-date` (Run H, T520) — this crate,
/// `vix-file-information-panel`, and `vix-org` used to each hand-roll an
/// independent copy of the same day-count-to-calendar-date algorithm.
#[must_use]
fn epoch_to_date(secs: i64, tz_offset: i32) -> String {
    let days = (secs + i64::from(tz_offset)).div_euclid(86_400);
    let (y, m, d) = vix_civil_date::civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(feature = "process")]
/// `git blame` for a single 1-based line of a repo-relative path, parsed into a
/// [`BlameLine`]. `None` when not a repo, the path is untracked, or on error.
///
/// `contents` is the buffer's text when it has unsaved edits (T657): git then
/// blames that text (`--contents -`) instead of the copy on disk, so `line`
/// means the same line the user sees, and edited lines come back uncommitted.
#[must_use]
pub fn blame_line(
    dir: &Path,
    rel_path: &str,
    line: usize,
    contents: Option<&str>,
) -> Option<BlameLine> {
    let spec = format!("{line},{line}");
    let (cwd, path) = repo_target(dir, rel_path);
    let out = match contents {
        None => git_stdout(
            &cwd,
            &["blame", "--line-porcelain", "-L", &spec, "--", &path],
        )?,
        Some(text) => git_stdin(
            &cwd,
            &[
                "blame",
                "--line-porcelain",
                "-L",
                &spec,
                "--contents",
                "-",
                "--",
                &path,
            ],
            text,
        )
        .ok()?,
    };
    parse_blame_porcelain(&out)
}

// ----- runners (shell out to `git`) --------------------------------------

#[cfg(feature = "process")]
/// Absolute path to the `git` executable, resolved from `PATH` exactly once.
///
/// Every git runner sets `current_dir(dir)` to the opened repository, which is
/// attacker-controllable content. On Windows, `Command::new("git")` (a bare
/// name) is resolved by `CreateProcessW` using a search order that includes the
/// child's current directory — so a repo shipping a `git.exe`/`git.bat` in its
/// root would run that planted binary. Resolving against `PATH` only (never the
/// cwd) closes this binary-planting vector. Falls back to the bare name if the
/// lookup fails (behaviour unchanged on systems where `git` isn't on `PATH`).
fn git_program() -> &'static OsString {
    static PROG: OnceLock<OsString> = OnceLock::new();
    PROG.get_or_init(|| which_on_path("git").unwrap_or_else(|| "git".into()))
}

#[cfg(feature = "process")]
/// Find `name` on `PATH` (honouring `PATHEXT` on Windows), returning its
/// absolute path. Never consults the current directory.
fn which_on_path(name: &str) -> Option<OsString> {
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE".into())
            .split(';')
            .map(str::to_string)
            .collect()
    } else {
        vec![String::new()]
    };
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        if dir.as_os_str().is_empty() {
            continue; // an empty PATH entry means cwd — never search it
        }
        for ext in &exts {
            let candidate = dir.join(format!("{name}{ext}"));
            if candidate.is_file() {
                return Some(candidate.into_os_string());
            }
        }
    }
    None
}

#[cfg(feature = "process")]
/// Whether `name` is safe to pass to `git` as a positional ref argument: not
/// empty, not option-shaped (a leading `-` would be parsed as a flag), and free
/// of whitespace/control characters. git itself forbids most of these in real
/// ref names, but validating before spawning stops argument injection through
/// crafted `.git/refs` entries or prompt input.
fn valid_ref_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && !name.chars().any(|c| c.is_whitespace() || c.is_control())
}

#[cfg(feature = "process")]
/// Why a git command failed (T673).
#[derive(Debug)]
#[non_exhaustive]
pub enum GitError {
    /// `git` could not be found: it isn't installed, or isn't on `PATH`.
    NotInstalled,
    /// `git` could not be started or talked to for another reason.
    Io(std::io::Error),
    /// git ran and exited unsuccessfully.
    Failed {
        /// The exit code, when the process wasn't killed by a signal.
        code: Option<i32>,
        /// git's own error output, trimmed.
        stderr: String,
    },
    /// An argument was refused before running git (an option-shaped or
    /// malformed branch name).
    InvalidArgument(String),
}

#[cfg(feature = "process")]
impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInstalled => f.write_str("git is not installed (or not on PATH)"),
            Self::Io(e) => write!(f, "{e}"),
            Self::Failed { stderr, .. } if !stderr.is_empty() => f.write_str(stderr),
            Self::Failed { code: Some(c), .. } => write!(f, "git exited with status {c}"),
            Self::Failed { code: None, .. } => f.write_str("git was terminated"),
            Self::InvalidArgument(why) => f.write_str(why),
        }
    }
}

#[cfg(feature = "process")]
impl std::error::Error for GitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

#[cfg(feature = "process")]
impl From<std::io::Error> for GitError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::NotFound {
            Self::NotInstalled
        } else {
            Self::Io(e)
        }
    }
}

#[cfg(feature = "process")]
impl GitError {
    /// The error for a git run that exited unsuccessfully.
    fn from_output(out: &std::process::Output) -> Self {
        Self::Failed {
            code: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        }
    }
}

#[cfg(feature = "process")]
fn git(dir: &Path, args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new(git_program())
        .current_dir(dir)
        // T651: every path Vix hands git is a literal file name, never a
        // pattern -- without this, staging `app/[id]/p` also stages `app/i/p`.
        .env("GIT_LITERAL_PATHSPECS", "1")
        .args(args)
        .output()
}

#[cfg(feature = "process")]
/// Where a directory sits inside its git working tree: the tree's top level
/// and the directory's own path below it (`git rev-parse --show-prefix`,
/// e.g. `"sub/"`, empty at the top level).
#[derive(Clone, Debug)]
struct RepoLoc {
    top: PathBuf,
    prefix: String,
}

#[cfg(feature = "process")]
/// Locate `dir` inside its working tree (T650), cached per directory next to
/// the repo check: one `git rev-parse` the first time, a map lookup after.
/// Only successes are cached, so a directory that later becomes a repository
/// (`git init`) is found on the next call. `None` outside a working tree.
fn locate(dir: &Path) -> Option<RepoLoc> {
    static CACHE: OnceLock<Mutex<std::collections::HashMap<PathBuf, RepoLoc>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    if let Some(loc) = cache.lock().ok()?.get(dir) {
        return Some(loc.clone());
    }
    let out = git(dir, &["rev-parse", "--show-toplevel", "--show-prefix"]).ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines();
    let top = PathBuf::from(lines.next().filter(|t| !t.is_empty())?);
    let prefix = lines.next().unwrap_or("").to_string();
    let loc = RepoLoc { top, prefix };
    if let Ok(mut map) = cache.lock() {
        map.insert(dir.to_path_buf(), loc.clone());
    }
    Some(loc)
}

#[cfg(feature = "process")]
/// Join `prefix` (a `--show-prefix` value) and `rel` (relative to the
/// directory that prefix names) into a repository-root-relative path,
/// resolving `.` and `..` lexically. `None` when the result would climb
/// above the repository root or `rel` is absolute.
fn join_repo_path(prefix: &str, rel: &str) -> Option<String> {
    let rel = rel.replace('\\', "/");
    if rel.starts_with('/') || Path::new(&rel).is_absolute() {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in prefix.split('/').chain(rel.split('/')) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    Some(parts.join("/"))
}

#[cfg(feature = "process")]
/// The inverse of [`join_repo_path`]: re-express a repository-root-relative
/// `path` (as `git status --porcelain` prints it) relative to the directory
/// whose `--show-prefix` is `prefix`, climbing with `..` when it lies outside.
fn relative_to_prefix(prefix: &str, path: &str) -> String {
    let base: Vec<&str> = prefix.split('/').filter(|p| !p.is_empty()).collect();
    let target: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    let common = base.iter().zip(&target).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<&str> = vec![".."; base.len() - common];
    parts.extend(&target[common..]);
    parts.join("/")
}

#[cfg(feature = "process")]
/// Where to run a git command naming `rel_path` (relative to `dir`, the
/// workspace root), and the path to give it (T650). Inside a working tree
/// that is the tree's top level and the root-relative path, so every git
/// syntax agrees on it: pathspecs after `--`, `HEAD:<path>`/`:<path>` blob
/// names (always root-relative), and `update-index --cacheinfo`. Outside
/// one -- or for a path that can't be mapped -- it is `dir` and `rel_path`
/// unchanged, so git reports its own error.
fn repo_target(dir: &Path, rel_path: &str) -> (PathBuf, String) {
    locate(dir)
        .and_then(|loc| join_repo_path(&loc.prefix, rel_path).map(|p| (loc.top, p)))
        .unwrap_or_else(|| (dir.to_path_buf(), rel_path.to_string()))
}

#[cfg(feature = "process")]
/// Run `git` and return trimmed stdout on success, or `None` on any failure.
fn git_stdout(dir: &Path, args: &[&str]) -> Option<String> {
    let out = git(dir, args).ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
    } else {
        None
    }
}

#[cfg(feature = "process")]
/// Whether `dir` is inside a git working tree.
#[must_use]
pub fn is_repo(dir: &Path) -> bool {
    git_stdout(dir, &["rev-parse", "--is-inside-work-tree"]).as_deref() == Some("true")
}

#[cfg(feature = "process")]
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

#[cfg(feature = "process")]
/// Whether the working tree has any staged or unstaged changes (incl. untracked).
#[must_use]
pub fn is_dirty(dir: &Path) -> bool {
    git_stdout(dir, &["status", "--porcelain"]).is_some_and(|s| !s.is_empty())
}

#[cfg(feature = "process")]
/// The changed files reported by `git status --porcelain`.
#[must_use]
pub fn status(dir: &Path) -> Vec<FileStatus> {
    let mut rows = git_stdout(dir, &["status", "--porcelain"])
        .map(|s| parse_status(&s))
        .unwrap_or_default();
    // Porcelain paths are always repository-root-relative; callers work
    // relative to `dir` (T650).
    if let Some(loc) = locate(dir).filter(|l| !l.prefix.is_empty()) {
        for row in &mut rows {
            row.path = relative_to_prefix(&loc.prefix, &row.path);
        }
    }
    rows
}

#[cfg(feature = "process")]
/// The committed (`HEAD`) contents of `rel_path` (relative to `dir`), or `None`
/// if the path is not in HEAD (newly added/untracked) or on any error.
#[must_use]
pub fn head_blob(dir: &Path, rel_path: &str) -> Option<String> {
    // Use the raw (untrimmed) output: a blob's trailing newline is significant
    // for line-accurate diffing, gutter marks, and hunk revert.
    let (cwd, path) = repo_target(dir, rel_path);
    let out = git(&cwd, &["show", &format!("HEAD:{path}")]).ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(feature = "process")]
/// Stage every change in the working tree (`git add -A`).
///
/// # Errors
///
/// Returns git's own stderr on failure.
pub fn stage_all(dir: &Path) -> Result<(), GitError> {
    let out = git(dir, &["add", "-A"])?;
    if out.status.success() {
        Ok(())
    } else {
        Err(GitError::from_output(&out))
    }
}

#[cfg(feature = "process")]
/// Stage a path (`git add -- <path>`).
///
/// # Errors
///
/// Returns git's own stderr on failure (Run H, T540 — this used to collapse
/// to a bare `bool`, so the caller could only show a static "git failed"
/// with no reason, inconsistent with the sibling [`stage_content`], which
/// already reports the real error).
pub fn stage(dir: &Path, rel_path: &str) -> Result<(), GitError> {
    let (cwd, path) = repo_target(dir, rel_path);
    let out = git(&cwd, &["add", "--", &path])?;
    if out.status.success() {
        Ok(())
    } else {
        Err(GitError::from_output(&out))
    }
}

#[cfg(feature = "process")]
/// The staged (index) version of `rel_path` (`git show :path`), trailing newline
/// preserved. `None` when the file is not in the index.
#[must_use]
pub fn index_blob(dir: &Path, rel_path: &str) -> Option<String> {
    let (cwd, path) = repo_target(dir, rel_path);
    let out = git(&cwd, &["show", &format!(":{path}")]).ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(feature = "process")]
/// Replace the index entry for `rel_path` with exactly `content` (used to stage
/// a single hunk): hash the content into a blob and point the index at it,
/// keeping the working tree untouched.
///
/// # Errors
/// Returns an error when the git plumbing commands fail.
pub fn stage_content(dir: &Path, rel_path: &str, content: &str) -> Result<(), GitError> {
    // T650: `--cacheinfo` takes a repository-root-relative path whatever the
    // cwd, so run from the top level with the root-relative path throughout.
    let (cwd, path) = repo_target(dir, rel_path);
    let hash = git_stdin(
        &cwd,
        &["hash-object", "-w", "--path", &path, "--stdin"],
        content,
    )?;
    let sha = hash.trim();
    // Preserve the file's existing index mode when known; default to a regular
    // non-executable file.
    let mode = git_stdout(&cwd, &["ls-files", "-s", "--", &path])
        .and_then(|s| s.split_whitespace().next().map(str::to_string))
        .unwrap_or_else(|| "100644".to_string());
    let info = format!("{mode},{sha},{path}");
    let out = git(&cwd, &["update-index", "--add", "--cacheinfo", &info])?;
    if out.status.success() {
        Ok(())
    } else {
        Err(GitError::from_output(&out))
    }
}

#[cfg(feature = "process")]
/// Run a git command feeding `input` on stdin; return its stdout on success.
fn git_stdin(dir: &Path, args: &[&str], input: &str) -> Result<String, GitError> {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(git_program())
        .current_dir(dir)
        .env("GIT_LITERAL_PATHSPECS", "1") // T651, as in `git`
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or_else(|| GitError::Io(std::io::Error::other("no stdin")))?
        .write_all(input.as_bytes())?;
    let out = child.wait_with_output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(GitError::from_output(&out))
    }
}

#[cfg(feature = "process")]
/// Discard a path's unstaged changes: restore the working-tree file from the
/// index (`git restore --worktree -- <path>`, T657). Staged changes stay.
///
/// # Errors
///
/// [`GitError::Failed`] when git refuses (for example, an untracked path).
pub fn discard(dir: &Path, rel_path: &str) -> Result<(), GitError> {
    let (cwd, path) = repo_target(dir, rel_path);
    let out = git(&cwd, &["restore", "--worktree", "--", &path])?;
    if out.status.success() {
        Ok(())
    } else {
        Err(GitError::from_output(&out))
    }
}

#[cfg(feature = "process")]
/// Unstage a path (`git restore --staged -- <path>`).
///
/// # Errors
///
/// Returns git's own stderr on failure (Run H, T540 — see [`stage`]'s doc
/// comment for why).
pub fn unstage(dir: &Path, rel_path: &str) -> Result<(), GitError> {
    let (cwd, path) = repo_target(dir, rel_path);
    let out = git(&cwd, &["restore", "--staged", "--", &path])?;
    if out.status.success() {
        Ok(())
    } else {
        Err(GitError::from_output(&out))
    }
}

#[cfg(feature = "process")]
/// The staged diff (`git diff --staged`), for feeding to an AI commit-message
/// generator (T125) — `None` on any failure, or when nothing is staged
/// (an empty diff is not useful context and would only prompt a generic
/// reply).
#[must_use]
pub fn staged_diff(dir: &Path) -> Option<String> {
    git_stdout(dir, &["diff", "--staged"]).filter(|d| !d.is_empty())
}

#[cfg(feature = "process")]
/// The number of commits reachable from HEAD (`git rev-list --count HEAD`), or
/// `None` when not a repo or there are no commits yet.
#[must_use]
pub fn commit_count(dir: &Path) -> Option<u64> {
    git_stdout(dir, &["rev-list", "--count", "HEAD"])?
        .trim()
        .parse()
        .ok()
}

#[cfg(feature = "process")]
/// The local branch names, current branch first when it can be determined.
#[must_use]
pub fn local_branches(dir: &Path) -> Vec<String> {
    let out = git_stdout(dir, &["branch", "--format=%(refname:short)"]).unwrap_or_default();
    let mut names: Vec<String> = out
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    if let Some(cur) = branch(dir)
        && let Some(pos) = names.iter().position(|n| *n == cur)
    {
        names.swap(0, pos);
    }
    names
}

#[cfg(feature = "process")]
/// Check out an existing branch (`git switch <branch>`). Returns `Ok(())` on
/// success, or the captured stderr on failure (e.g. a conflicting dirty tree).
///
/// # Errors
/// Returns the trimmed stderr text when `git switch` exits non-zero or cannot run.
pub fn checkout(dir: &Path, branch: &str) -> Result<(), GitError> {
    if !valid_ref_name(branch) {
        return Err(GitError::InvalidArgument(format!(
            "invalid branch name: {branch:?}"
        )));
    }
    // `--end-of-options` guarantees the ref can never be read as a flag, even
    // if it slipped past `valid_ref_name`.
    match git(dir, &["switch", "--end-of-options", branch]) {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(GitError::from_output(&o)),
        Err(e) => Err(e.into()),
    }
}

#[cfg(feature = "process")]
/// Create a new branch and switch to it (`git switch -c <name>`). Returns
/// `Ok(())` on success, or the captured stderr on failure (e.g. the name already
/// exists or is invalid).
///
/// # Errors
/// Returns the trimmed stderr text when `git switch -c` exits non-zero or cannot run.
pub fn create_branch(dir: &Path, name: &str) -> Result<(), GitError> {
    if !valid_ref_name(name) {
        return Err(GitError::InvalidArgument(format!(
            "invalid branch name: {name:?}"
        )));
    }
    match git(dir, &["switch", "-c", name]) {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(GitError::from_output(&o)),
        Err(e) => Err(e.into()),
    }
}

#[cfg(feature = "process")]
/// Commit the staged changes with `message`. Returns `Ok(())` on success, or the
/// captured stderr on failure (e.g. nothing staged, hook rejected).
///
/// # Errors
/// Returns the trimmed stderr text when `git commit` exits non-zero or cannot run.
pub fn commit(dir: &Path, message: &str) -> Result<(), GitError> {
    run_ok(dir, &["commit", "-m", message])
}

/// Whether a `gpg-agent.conf` selects a terminal-based pinentry
/// (`pinentry-curses`, `pinentry-tty`): the last `pinentry-program` line wins.
/// Such a program draws on and reads from `/dev/tty` directly, so running it
/// under Vix's raw-mode screen corrupts the display and steals keystrokes
/// (T570).
#[must_use]
pub fn pinentry_wants_tty(agent_conf: &str) -> bool {
    agent_conf
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.starts_with('#') {
                return None;
            }
            l.strip_prefix("pinentry-program")
                .filter(|rest| rest.starts_with(char::is_whitespace))
                .map(str::trim)
        })
        .next_back()
        .is_some_and(|program| {
            let name = program.rsplit(['/', '\\']).next().unwrap_or(program);
            name.contains("curses") || name.contains("tty")
        })
}

#[cfg(feature = "process")]
/// Whether committing in `dir` would sign with a terminal pinentry, which
/// cannot run under Vix's raw-mode screen (T570): `commit.gpgsign` is true, the
/// signing format is GPG, and the user's `gpg-agent.conf` selects a
/// curses/tty pinentry.
#[must_use]
pub fn commit_signing_needs_tty(dir: &Path) -> bool {
    if git_stdout(dir, &["config", "--bool", "--get", "commit.gpgsign"]).as_deref() != Some("true")
    {
        return false;
    }
    if git_stdout(dir, &["config", "--get", "gpg.format"]).as_deref() == Some("ssh") {
        return false;
    }
    let home = std::env::var_os("GNUPGHOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".gnupg")));
    home.and_then(|h| std::fs::read_to_string(h.join("gpg-agent.conf")).ok())
        .is_some_and(|conf| pinentry_wants_tty(&conf))
}

#[cfg(feature = "process")]
/// Amend the previous commit, folding in currently-staged changes and keeping
/// its message (`git commit --amend --no-edit`).
///
/// # Errors
/// Returns git's stderr on failure (e.g. no commit to amend).
pub fn commit_amend(dir: &Path) -> Result<(), GitError> {
    run_ok(dir, &["commit", "--amend", "--no-edit"])
}

#[cfg(feature = "process")]
/// Stash the working tree and index (`git stash push`).
///
/// # Errors
/// Returns git's stderr on failure.
pub fn stash_push(dir: &Path) -> Result<(), GitError> {
    run_ok(dir, &["stash", "push"])
}

// ----- Log / history (T207) --------------------------------------------

/// One `git log` row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEntry {
    /// Full commit hash.
    pub sha: String,
    /// Abbreviated commit hash, as git itself would print it.
    pub abbrev: String,
    /// Author name.
    pub author: String,
    /// Commit date (`YYYY-MM-DD`, the author date).
    pub date: String,
    /// The commit message's first line.
    pub subject: String,
}

/// Where a [`LogPanel`]'s entries came from, and what its Enter should diff
/// against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogScope {
    /// The whole repository's history (**Git → Log**).
    Repo,
    /// One file's history (**Git → File History**), by its
    /// workspace-relative path -- so Enter can pass it to [`show_commit`]
    /// and diff just that file, not the whole commit.
    File(String),
}

/// A commit-list overlay: [`log`]/[`file_log`] entries, a highlighted row,
/// and a scroll offset.
pub struct LogPanel {
    /// Commit rows, most recent first (as `git log` orders them).
    pub entries: Vec<LogEntry>,
    /// Where these entries came from.
    pub scope: LogScope,
    /// Index of the highlighted row.
    pub selected: usize,
    /// First visible row.
    pub scroll: usize,
}

impl LogPanel {
    /// A fresh panel over `entries`, selecting the first (most recent) row.
    #[must_use]
    pub fn new(entries: Vec<LogEntry>, scope: LogScope) -> Self {
        LogPanel {
            entries,
            scope,
            selected: 0,
            scroll: 0,
        }
    }

    /// Move the highlight up one row, stopping at the top.
    pub fn up(&mut self) {
        self.selected = vix_list_state::up(self.selected);
    }

    /// Move the highlight down one row, stopping at the bottom.
    pub fn down(&mut self) {
        self.selected = vix_list_state::down(self.selected, self.entries.len());
    }

    /// Move the highlight up one page, stopping at the top.
    pub fn page_up(&mut self, page: usize) {
        self.selected = vix_list_state::page_up(self.selected, page);
    }

    /// Move the highlight down one page, stopping at the bottom.
    pub fn page_down(&mut self, page: usize) {
        self.selected = vix_list_state::page_down(self.selected, page, self.entries.len());
    }

    /// Keep the highlighted row within a window of `height` visible rows.
    pub fn ensure_visible(&mut self, height: usize) {
        self.scroll =
            vix_list_state::ensure_visible(self.selected, self.scroll, height, self.entries.len());
    }

    /// The highlighted commit's full hash, if any.
    #[must_use]
    pub fn selected_sha(&self) -> Option<&str> {
        self.entries.get(self.selected).map(|e| e.sha.as_str())
    }

    /// The highlighted commit's abbreviated hash, if any.
    #[must_use]
    pub fn selected_abbrev(&self) -> Option<&str> {
        self.entries.get(self.selected).map(|e| e.abbrev.as_str())
    }
}

#[cfg(feature = "process")]
/// Delimiters for [`parse_log`]'s custom `git log --format`: ASCII unit
/// separator between fields, record separator between commits. Neither can
/// appear in ordinary commit metadata, so no escaping is needed.
const LOG_FIELD_SEP: &str = "\u{1f}";
#[cfg(feature = "process")]
const LOG_RECORD_SEP: &str = "\u{1e}";

#[cfg(feature = "process")]
/// Parse [`log`]/[`file_log`]'s `git log` output (the `LOG_FORMAT` shape)
/// into rows. Malformed records (the wrong number of fields) are skipped
/// rather than panicking.
fn parse_log(output: &str) -> Vec<LogEntry> {
    output
        .split(LOG_RECORD_SEP)
        .filter_map(|record| {
            let record = record.trim_start_matches('\n');
            let mut fields = record.split(LOG_FIELD_SEP);
            let sha = fields.next()?.to_string();
            let abbrev = fields.next()?.to_string();
            let author = fields.next()?.to_string();
            let date = fields.next()?.to_string();
            let subject = fields.next()?.to_string();
            if sha.is_empty() {
                return None;
            }
            Some(LogEntry {
                sha,
                abbrev,
                author,
                date,
                subject,
            })
        })
        .collect()
}

#[cfg(feature = "process")]
/// `git log --format=<LOG_FORMAT>`'s format string: full/abbreviated hash,
/// author name, author date, subject, then the record separator.
fn log_format_arg() -> String {
    format!(
        "--format=%H{LOG_FIELD_SEP}%h{LOG_FIELD_SEP}%an{LOG_FIELD_SEP}%ad{LOG_FIELD_SEP}%s{LOG_RECORD_SEP}"
    )
}

#[cfg(feature = "process")]
/// The repository's `max` most recent commits on `HEAD` (**Git → Log**).
#[must_use]
pub fn log(dir: &Path, max: usize) -> Vec<LogEntry> {
    let n = max.to_string();
    let fmt = log_format_arg();
    git_stdout(dir, &["log", &fmt, "--date=short", "-n", &n])
        .map(|s| parse_log(&s))
        .unwrap_or_default()
}

#[cfg(feature = "process")]
/// The `max` most recent commits touching `rel_path`, following renames
/// (**Git → File History**).
#[must_use]
pub fn file_log(dir: &Path, rel_path: &str, max: usize) -> Vec<LogEntry> {
    let n = max.to_string();
    let fmt = log_format_arg();
    let (cwd, path) = repo_target(dir, rel_path);
    git_stdout(
        &cwd,
        &[
            "log",
            &fmt,
            "--date=short",
            "--follow",
            "-n",
            &n,
            "--",
            &path,
        ],
    )
    .map(|s| parse_log(&s))
    .unwrap_or_default()
}

#[cfg(feature = "process")]
/// A commit's own unified diff (`git show <sha>`), or just the part touching
/// `rel_path` when given. `None` on any failure, including an invalid `sha`
/// (rejected before shelling out, the same guard [`checkout`] uses).
#[must_use]
pub fn show_commit(dir: &Path, sha: &str, rel_path: Option<&str>) -> Option<String> {
    if !valid_ref_name(sha) {
        return None;
    }
    // `--end-of-options` guarantees `sha` can never be read as a flag, even
    // if it slipped past `valid_ref_name` (the same defense `checkout` uses).
    let mut args = vec!["show", "--end-of-options", sha];
    let target = rel_path.map(|p| repo_target(dir, p));
    let cwd = match &target {
        Some((cwd, path)) => {
            args.push("--");
            args.push(path);
            cwd.as_path()
        }
        None => dir,
    };
    git_stdout(cwd, &args)
}

#[cfg(feature = "process")]
/// `rel_path`'s content at `revision` (`git show <revision>:<rel_path>`, e.g.
/// **Git → Open File at Revision…**). `None` when `revision` doesn't resolve,
/// the path isn't in that tree, or `revision` fails the same validation
/// [`checkout`] applies to a ref name.
#[must_use]
pub fn show_file_at(dir: &Path, revision: &str, rel_path: &str) -> Option<String> {
    if !valid_ref_name(revision) {
        return None;
    }
    let (cwd, path) = repo_target(dir, rel_path);
    let out = git(
        &cwd,
        &["show", "--end-of-options", &format!("{revision}:{path}")],
    )
    .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(feature = "process")]
/// Resolve `revision` (a branch, tag, or any commit-ish) to its abbreviated
/// hash (`git rev-parse --short`), for titling an Open File at Revision tab.
/// `None` when it doesn't resolve or fails the ref-name validation.
#[must_use]
pub fn resolve_short_sha(dir: &Path, revision: &str) -> Option<String> {
    if !valid_ref_name(revision) {
        return None;
    }
    git_stdout(dir, &["rev-parse", "--short", "--end-of-options", revision])
}

#[cfg(feature = "process")]
/// Restore the most recent stash (`git stash pop`).
///
/// # Errors
/// Returns git's stderr on failure (e.g. no stash, or a conflict).
pub fn stash_pop(dir: &Path) -> Result<(), GitError> {
    run_ok(dir, &["stash", "pop"])
}

#[cfg(feature = "process")]
/// Run a git command, mapping a non-zero exit to [`GitError::Failed`].
fn run_ok(dir: &Path, args: &[&str]) -> Result<(), GitError> {
    match git(dir, args) {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(GitError::from_output(&o)),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "process")]
    #[test]
    fn valid_ref_name_rejects_option_shaped_and_control_input() {
        assert!(valid_ref_name("main"));
        assert!(valid_ref_name("feature/x-y"));
        // Option-shaped names that git would otherwise interpret as flags.
        assert!(!valid_ref_name("--detach"));
        assert!(!valid_ref_name("-oProxyCommand=x"));
        assert!(!valid_ref_name("--upload-pack=evil"));
        // Empty / whitespace / control characters.
        assert!(!valid_ref_name(""));
        assert!(!valid_ref_name("a b"));
        assert!(!valid_ref_name("a\nb"));
    }

    #[cfg(feature = "process")]
    #[test]
    fn checkout_and_create_branch_refuse_dash_prefixed_names() {
        // No git process is spawned: validation fails first, so any temp dir works.
        let dir = std::env::temp_dir();
        assert!(checkout(&dir, "--detach").is_err());
        assert!(create_branch(&dir, "-c").is_err());
        assert!(create_branch(&dir, "--orphan").is_err());
    }

    #[cfg(feature = "process")]
    #[test]
    fn show_commit_and_show_file_at_refuse_option_shaped_revisions() {
        // No git process is spawned: validation fails first, so any temp dir works.
        let dir = std::env::temp_dir();
        assert!(show_commit(&dir, "--upload-pack=evil", None).is_none());
        assert!(show_file_at(&dir, "--upload-pack=evil", "a.rs").is_none());
        assert!(resolve_short_sha(&dir, "--upload-pack=evil").is_none());
    }

    #[cfg(feature = "process")]
    #[test]
    fn stage_and_unstage_report_the_real_git_error() {
        // T540 (Run H): these used to collapse to a bare `bool`, so a caller
        // could only show a generic "git failed" with no reason. Any git
        // command run outside a repository fails immediately with a real,
        // git-provided message -- no repo fixture needed to prove it's
        // captured now instead of discarded.
        let dir = std::env::temp_dir().join(format!("vix-git-notrepo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for err in [
            stage(&dir, "a.txt").expect_err("not a repo, so this must fail"),
            unstage(&dir, "a.txt").expect_err("not a repo, so this must fail"),
        ] {
            assert!(
                matches!(&err, GitError::Failed { stderr, .. } if !stderr.is_empty()),
                "a real message, not silently discarded: {err:?}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(feature = "process")]
    #[test]
    fn a_missing_git_is_not_installed_rather_than_a_failure() {
        // T673: callers can tell "git isn't there" from "git failed".
        let missing = std::io::Error::new(std::io::ErrorKind::NotFound, "no such file");
        assert!(matches!(GitError::from(missing), GitError::NotInstalled));
        let other = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        assert!(matches!(GitError::from(other), GitError::Io(_)));
        let failed = GitError::Failed {
            code: Some(128),
            stderr: String::new(),
        };
        assert_eq!(failed.to_string(), "git exited with status 128");
        assert!(matches!(
            checkout(Path::new("."), "--evil"),
            Err(GitError::InvalidArgument(_))
        ));
    }

    #[cfg(feature = "process")]
    #[test]
    fn parse_log_splits_records_and_fields() {
        let output = format!(
            "abc123{LOG_FIELD_SEP}abc{LOG_FIELD_SEP}Ada Lovelace{LOG_FIELD_SEP}2026-01-02\
             {LOG_FIELD_SEP}Add the analytical engine{LOG_RECORD_SEP}\
             def456{LOG_FIELD_SEP}def{LOG_FIELD_SEP}Grace Hopper{LOG_FIELD_SEP}2026-01-01\
             {LOG_FIELD_SEP}Initial commit{LOG_RECORD_SEP}"
        );
        let entries = parse_log(&output);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].sha, "abc123");
        assert_eq!(entries[0].abbrev, "abc");
        assert_eq!(entries[0].author, "Ada Lovelace");
        assert_eq!(entries[0].date, "2026-01-02");
        assert_eq!(entries[0].subject, "Add the analytical engine");
        assert_eq!(entries[1].author, "Grace Hopper");
    }

    #[cfg(feature = "process")]
    #[test]
    fn parse_log_skips_a_trailing_empty_record() {
        // git log's own trailing record-separator leaves one empty record at
        // the end; it must not become a bogus all-empty-field entry.
        let output = format!(
            "abc123{LOG_FIELD_SEP}abc{LOG_FIELD_SEP}Ada Lovelace{LOG_FIELD_SEP}2026-01-02\
             {LOG_FIELD_SEP}subject{LOG_RECORD_SEP}"
        );
        assert_eq!(parse_log(&output).len(), 1);
    }

    #[cfg(feature = "process")]
    #[test]
    fn parse_log_ignores_a_truncated_record() {
        let output = format!("abc123{LOG_FIELD_SEP}abc{LOG_FIELD_SEP}only three fields");
        assert_eq!(parse_log(&output), [] as [LogEntry; 0]);
    }

    #[test]
    fn log_panel_navigation_reports_the_selected_hashes() {
        let mut p = LogPanel::new(
            vec![
                LogEntry {
                    sha: "aaa111".into(),
                    abbrev: "aaa".into(),
                    author: "A".into(),
                    date: "2026-01-01".into(),
                    subject: "first".into(),
                },
                LogEntry {
                    sha: "bbb222".into(),
                    abbrev: "bbb".into(),
                    author: "B".into(),
                    date: "2026-01-02".into(),
                    subject: "second".into(),
                },
            ],
            LogScope::Repo,
        );
        assert_eq!(p.selected_sha(), Some("aaa111"));
        assert_eq!(p.selected_abbrev(), Some("aaa"));
        p.down();
        assert_eq!(p.selected_sha(), Some("bbb222"));
        p.down(); // stays clamped at the last row
        assert_eq!(p.selected_sha(), Some("bbb222"));
        p.up();
        assert_eq!(p.selected_sha(), Some("aaa111"));
    }

    #[cfg(feature = "process")]
    #[test]
    fn which_on_path_never_returns_a_relative_or_cwd_path() {
        // Whatever it resolves (git may or may not be installed in CI), it must
        // be an absolute path — never a bare name resolvable against the cwd.
        if let Some(p) = which_on_path("git") {
            assert!(
                Path::new(&p).is_absolute(),
                "git resolved to non-absolute: {p:?}"
            );
        }
    }

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
        assert_eq!(hunks("x\ny\n", "x\ny\n"), [] as [Hunk; 0]);
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
        assert_eq!(parse_status("\n\nx\n"), [] as [FileStatus; 0]);
    }

    #[test]
    fn diff_marks_flags_added_modified_and_deleted() {
        let head = "alpha\nbeta\ngamma\n";
        // Modify line 2, add a new line 3, keep the rest.
        let current = "alpha\nBETA\ngamma\ndelta\n";
        let marks = diff_marks(head, current);
        assert!(
            marks.contains(&(1, LineMark::Modified)),
            "line 2 modified: {marks:?}"
        );
        assert!(
            marks.contains(&(3, LineMark::Added)),
            "line 4 added: {marks:?}"
        );
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
    fn diff_marks_skips_huge_buffers_and_finishes_fast_on_rewrites() {
        // Over the line cap: no marks at all, and no work done.
        let big = "x\n".repeat(GUTTER_DIFF_MAX_LINES + 1);
        assert_eq!(diff_marks("", &big), vec![]);
        // A fully rewritten mid-size file must come back inside a generous
        // multiple of the deadline rather than running Myers to completion.
        let head = (0..20_000).fold(String::new(), |a, i| a + &format!("a{i}\n"));
        let cur = (0..20_000).fold(String::new(), |a, i| {
            a + &format!(
                "b{}
",
                i * 7 % 20_000
            )
        });
        let start = std::time::Instant::now();
        let _ = diff_marks(&head, &cur);
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
    }

    #[test]
    fn pinentry_wants_tty_reads_the_last_program_line() {
        assert!(pinentry_wants_tty(
            "pinentry-program /usr/bin/pinentry-curses\n"
        ));
        assert!(pinentry_wants_tty(
            "pinentry-program /opt/homebrew/bin/pinentry-tty"
        ));
        assert!(!pinentry_wants_tty(
            "pinentry-program /usr/bin/pinentry-mac\n"
        ));
        assert!(!pinentry_wants_tty("default-cache-ttl 600\n"));
        // A comment does not count, and the last real line wins.
        assert!(!pinentry_wants_tty("# pinentry-program pinentry-curses\n"));
        assert!(!pinentry_wants_tty(
            "pinentry-program /usr/bin/pinentry-curses\npinentry-program /usr/bin/pinentry-gtk-2\n"
        ));
        assert!(pinentry_wants_tty(
            "pinentry-program /usr/bin/pinentry-gtk-2\npinentry-program /usr/bin/pinentry-curses\n"
        ));
        assert!(!pinentry_wants_tty(""));
    }

    #[test]
    fn diff_marks_empty_when_identical() {
        assert_eq!(diff_marks("x\ny\n", "x\ny\n"), [] as [(usize, LineMark); 0]);
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

    #[cfg(feature = "process")]
    #[test]
    fn join_repo_path_resolves_dots_and_refuses_to_escape() {
        assert_eq!(join_repo_path("sub/", "f").as_deref(), Some("sub/f"));
        assert_eq!(join_repo_path("", "a/b").as_deref(), Some("a/b"));
        assert_eq!(join_repo_path("sub/", "../f").as_deref(), Some("f"));
        assert_eq!(join_repo_path("a/b/", "./c/../d").as_deref(), Some("a/b/d"));
        assert_eq!(join_repo_path("sub/", "a\\b").as_deref(), Some("sub/a/b"));
        assert_eq!(join_repo_path("sub/", "../../f"), None);
        assert_eq!(join_repo_path("sub/", "/etc/passwd"), None);
    }

    #[cfg(feature = "process")]
    #[test]
    fn relative_to_prefix_inverts_join_repo_path() {
        assert_eq!(relative_to_prefix("sub/", "sub/f"), "f");
        assert_eq!(relative_to_prefix("sub/", "f"), "../f");
        assert_eq!(relative_to_prefix("a/b/", "a/c/f"), "../c/f");
        assert_eq!(relative_to_prefix("", "a/f"), "a/f");
        for (prefix, path) in [("sub/", "f"), ("a/b/", "a/c/f"), ("x/", "x/y/z")] {
            let rel = relative_to_prefix(prefix, path);
            assert_eq!(join_repo_path(prefix, &rel).as_deref(), Some(path));
        }
    }

    /// A throwaway repository: `f` and `sub/f` at the top level and in a
    /// subdirectory, with distinct contents, committed. Returns the top level.
    #[cfg(feature = "process")]
    fn scratch_repo(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vix-git-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("f"), "root one\nroot two\n").unwrap();
        std::fs::write(dir.join("sub/f"), "sub one\nsub two\n").unwrap();
        for args in [
            &["init", "-q"][..],
            &["add", "."],
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=t@example.com",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "-m",
                "seed",
            ],
        ] {
            let ok = Command::new("git")
                .current_dir(&dir)
                .env("GIT_TERMINAL_PROMPT", "0")
                .args(args)
                .status()
                .unwrap()
                .success();
            assert!(ok, "git {args:?}");
        }
        dir
    }

    #[cfg(feature = "process")]
    #[test]
    #[ignore = "needs git; creates a throwaway repo"]
    fn runners_resolve_paths_from_a_subdirectory_root() {
        // T650: Vix rooted at `sub/` names files relative to `sub/`, but git
        // reads `HEAD:<p>`, `:<p>` and `--cacheinfo` paths from the top level.
        let top = scratch_repo("subdir");
        let sub = top.join("sub");
        assert_eq!(head_blob(&sub, "f").as_deref(), Some("sub one\nsub two\n"));
        assert_eq!(index_blob(&sub, "f").as_deref(), Some("sub one\nsub two\n"));
        assert_eq!(
            head_blob(&sub, "../f").as_deref(),
            Some("root one\nroot two\n")
        );

        // Stage Hunk's plumbing must write `sub/f`, leaving the root `f` alone.
        stage_content(&sub, "f", "sub one\nstaged\n").unwrap();
        assert_eq!(index_blob(&sub, "f").as_deref(), Some("sub one\nstaged\n"));
        assert_eq!(
            index_blob(&top, "f").as_deref(),
            Some("root one\nroot two\n")
        );
        let rows = status(&sub);
        assert_eq!(rows.len(), 1, "only sub/f changed: {rows:?}");
        assert_eq!(rows[0].path, "f", "reported relative to the subdirectory");
        assert_eq!(status(&top)[0].path, "sub/f");

        // Stage/unstage take the paths `status` reports, including `../`.
        std::fs::write(top.join("f"), "root edited\n").unwrap();
        unstage(&sub, "f").unwrap();
        let rows = status(&sub);
        let root_row = rows
            .iter()
            .find(|r| r.path == "../f")
            .expect("root f listed");
        assert!(!root_row.is_staged());
        stage(&sub, &root_row.path).unwrap();
        assert!(
            status(&sub)
                .iter()
                .any(|r| r.path == "../f" && r.is_staged())
        );

        // History and blame resolve the subdirectory's file, not the root one.
        assert_eq!(file_log(&sub, "f", 10).len(), 1);
        assert_eq!(
            show_file_at(&sub, "HEAD", "f").as_deref(),
            Some("sub one\nsub two\n")
        );
        let sha = log(&sub, 1)[0].sha.clone();
        let patch = show_commit(&sub, &sha, Some("f")).unwrap();
        assert!(patch.contains("sub one") && !patch.contains("root one"));
        assert_eq!(blame_line(&sub, "f", 1, None).unwrap().summary, "seed");
        // T657: with unsaved edits, the edited text is blamed, so a new first
        // line is uncommitted and the old first line is still `seed` on line 2.
        let edited = format!(
            "new line\n{}",
            std::fs::read_to_string(sub.join("f")).unwrap()
        );
        assert!(
            blame_line(&sub, "f", 1, Some(&edited))
                .unwrap()
                .is_uncommitted()
        );
        assert_eq!(
            blame_line(&sub, "f", 2, Some(&edited)).unwrap().summary,
            "seed"
        );
        // Discard restores the file from the index.
        std::fs::write(sub.join("f"), "scribble\n").unwrap();
        discard(&sub, "f").unwrap();
        assert_ne!(
            std::fs::read_to_string(sub.join("f")).unwrap(),
            "scribble\n"
        );
        let _ = std::fs::remove_dir_all(&top);
    }

    #[cfg(feature = "process")]
    #[test]
    #[ignore = "needs git; creates a throwaway repo"]
    fn file_paths_are_literal_not_globs() {
        // T651: `app/[id]/p` (a Next.js-style route folder) used to match
        // `app/i/p` as a glob character class.
        let top = scratch_repo("literal");
        std::fs::create_dir_all(top.join("app/[id]")).unwrap();
        std::fs::create_dir_all(top.join("app/i")).unwrap();
        std::fs::write(top.join("app/[id]/p"), "a\n").unwrap();
        std::fs::write(top.join("app/i/p"), "b\n").unwrap();
        stage(&top, "app/[id]/p").unwrap();
        let staged: Vec<String> = status(&top)
            .into_iter()
            .filter(FileStatus::is_staged)
            .map(|r| r.path)
            .collect();
        assert_eq!(staged, ["app/[id]/p"]);
        stage(&top, "app/i/p").unwrap();
        unstage(&top, "app/[id]/p").unwrap();
        let staged: Vec<String> = status(&top)
            .into_iter()
            .filter(FileStatus::is_staged)
            .map(|r| r.path)
            .collect();
        assert_eq!(staged, ["app/i/p"]);
        let _ = std::fs::remove_dir_all(&top);
    }

    // Runner smoke test against this very repository. Ignored by default because
    // it assumes `git` is installed and the tests run inside the work tree.
    #[cfg(feature = "process")]
    #[test]
    #[ignore = "needs git and an in-tree checkout"]
    fn runners_report_branch_in_this_repo() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert!(is_repo(dir));
        assert!(branch(dir).is_some());
    }
}
