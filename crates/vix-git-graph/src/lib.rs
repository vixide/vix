//! The interactive Git graph: a commit list with branch lanes drawn from the
//! parent links, ref decorations, a branch/tag filter, a search box, paged
//! loading, and per-commit actions.
//!
//! The crate is three layers, each testable alone:
//!
//! 1. [`parse_log`] turns `git log` output in this crate's own format
//!    (see [`LOG_FORMAT`]) into [`Commit`]s.
//! 2. [`assign_lanes`] is a pure function from commits (newest first, parents
//!    after children) to one [`Row`] of lane geometry per commit. It never
//!    touches git, so it is property-tested over random commit DAGs.
//! 3. [`Panel`] is the overlay's state: the loaded commits, their rows, the
//!    search query, the ref filter, the selection and a pending confirmation.
//!
//! With the `process` feature (on by default) the crate also runs git:
//! [`fetch_page`] loads one page of commits, [`list_refs`] lists the branches
//! and tags for the filter, and [`checkout`], [`create_branch_at`] and
//! [`cherry_pick`] are the mutating per-commit actions. Every failure is a
//! [`GraphError`].

#[cfg(feature = "process")]
use std::path::Path;

/// How many commits one page of the graph loads.
pub const PAGE_SIZE: usize = 200;

/// Separates the fields of one record in [`LOG_FORMAT`] (ASCII unit separator).
const FIELD_SEP: char = '\u{1f}';
/// Ends one record in [`LOG_FORMAT`] (ASCII record separator).
const RECORD_SEP: char = '\u{1e}';

/// The `git log --format=` argument this crate parses: full hash, abbreviated
/// hash, parent hashes, author, short date, ref names, subject.
pub const LOG_FORMAT: &str = "--format=%H\u{1f}%h\u{1f}%P\u{1f}%an\u{1f}%ad\u{1f}%D\u{1f}%s\u{1e}";

/// Why a graph operation failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum GraphError {
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
    /// malformed revision or branch name).
    InvalidArgument(String),
    /// `git log` produced a record this crate could not parse.
    Parse(String),
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInstalled => f.write_str("git is not installed (or not on PATH)"),
            Self::Io(e) => write!(f, "{e}"),
            Self::Failed { stderr, .. } if !stderr.is_empty() => f.write_str(stderr),
            Self::Failed { code: Some(c), .. } => write!(f, "git exited with status {c}"),
            Self::Failed { code: None, .. } => f.write_str("git was terminated"),
            Self::InvalidArgument(why) | Self::Parse(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for GraphError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for GraphError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::NotFound {
            Self::NotInstalled
        } else {
            Self::Io(e)
        }
    }
}

// ----- Commits and decorations ------------------------------------------

/// What kind of ref a [`RefLabel`] names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefKind {
    /// `HEAD`, when detached or pointing at a commit not on a named branch.
    Head,
    /// A local branch.
    Branch,
    /// A remote-tracking branch.
    Remote,
    /// A tag.
    Tag,
}

/// One ref pointing at a commit (a decoration).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefLabel {
    /// The short name, e.g. `main`, `origin/main`, `v1.0`.
    pub name: String,
    /// What the ref is.
    pub kind: RefKind,
    /// Whether `HEAD` points at this ref (`HEAD -> main`).
    pub is_head: bool,
}

/// One commit of the graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    /// Full commit hash.
    pub sha: String,
    /// Abbreviated commit hash.
    pub abbrev: String,
    /// Parent hashes, first parent first.
    pub parents: Vec<String>,
    /// Author name.
    pub author: String,
    /// Author date (`YYYY-MM-DD`).
    pub date: String,
    /// The first line of the message.
    pub subject: String,
    /// The refs pointing at this commit.
    pub refs: Vec<RefLabel>,
}

/// Parse the `%D` field produced with `--decorate=full`: a comma-separated
/// list such as `HEAD -> refs/heads/main, tag: refs/tags/v1, refs/remotes/o/x`.
#[must_use]
pub fn parse_decorations(field: &str) -> Vec<RefLabel> {
    let mut out = Vec::new();
    for raw in field.split(", ") {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        if raw == "HEAD" {
            out.push(RefLabel {
                name: "HEAD".to_string(),
                kind: RefKind::Head,
                is_head: true,
            });
            continue;
        }
        let (is_head, rest) = match raw.strip_prefix("HEAD -> ") {
            Some(r) => (true, r),
            None => (false, raw),
        };
        let rest = rest.strip_prefix("tag: ").unwrap_or(rest);
        let (kind, name) = if let Some(n) = rest.strip_prefix("refs/heads/") {
            (RefKind::Branch, n)
        } else if let Some(n) = rest.strip_prefix("refs/tags/") {
            (RefKind::Tag, n)
        } else if let Some(n) = rest.strip_prefix("refs/remotes/") {
            (RefKind::Remote, n)
        } else {
            (RefKind::Branch, rest)
        };
        // `origin/HEAD` is a pointer, not a place anyone wants to filter on.
        if kind == RefKind::Remote && name.ends_with("/HEAD") {
            continue;
        }
        out.push(RefLabel {
            name: name.to_string(),
            kind,
            is_head,
        });
    }
    out
}

/// Parse `git log` output produced with [`LOG_FORMAT`] and `--decorate=full`.
///
/// # Errors
/// [`GraphError::Parse`] when a record has fewer than seven fields.
pub fn parse_log(output: &str) -> Result<Vec<Commit>, GraphError> {
    let mut commits = Vec::new();
    for record in output.split(RECORD_SEP) {
        let record = record.trim_start_matches('\n');
        if record.trim().is_empty() {
            continue;
        }
        let f: Vec<&str> = record.splitn(7, FIELD_SEP).collect();
        let [sha, abbrev, parents, author, date, refs, subject] = f[..] else {
            return Err(GraphError::Parse(format!(
                "malformed git log record: {record:?}"
            )));
        };
        commits.push(Commit {
            sha: sha.to_string(),
            abbrev: abbrev.to_string(),
            parents: parents.split_whitespace().map(str::to_string).collect(),
            author: author.to_string(),
            date: date.to_string(),
            subject: subject.to_string(),
            refs: parse_decorations(refs),
        });
    }
    Ok(commits)
}

// ----- Lane assignment ----------------------------------------------------

/// The lane geometry of one commit row.
///
/// Lanes are columns, numbered from 0 on the left. The commit's node sits in
/// [`lane`](Self::lane). Above the node, every lane in
/// [`incoming`](Self::incoming) was waiting for this commit (so a line from a
/// child arrives there). Lanes in [`through`](Self::through) just pass the row
/// by. Below the node, each lane in [`outgoing`](Self::outgoing) leads to one
/// parent, first parent first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The column holding this commit's node.
    pub lane: usize,
    /// Lanes whose line ends at this commit; includes `lane` unless the
    /// commit is a branch tip nothing was waiting for.
    pub incoming: Vec<usize>,
    /// Lanes that continue straight past this row.
    pub through: Vec<usize>,
    /// Lanes that continue from this commit toward each of its parents.
    pub outgoing: Vec<usize>,
    /// How many columns the row needs (one more than its highest lane).
    pub width: usize,
}

impl Row {
    /// Draw this row as text, two characters per column, padded to `width`
    /// columns (at least the row's own [`width`](Self::width)).
    ///
    /// `●` is the commit, `│` a lane passing by, `╯` a lane merging into the
    /// commit from the right, and `╮`/`╭` a lane branching off to the right
    /// or left; `─` joins a bend to the node.
    #[must_use]
    pub fn glyphs(&self, width: usize) -> String {
        let width = width.max(self.width);
        let mut bend: Vec<Option<char>> = vec![None; width];
        for &i in &self.incoming {
            if i != self.lane {
                bend[i] = Some('╯');
            }
        }
        for &o in &self.outgoing {
            if o != self.lane {
                bend[o] = Some(match bend[o] {
                    Some(_) => '┼',
                    None if o > self.lane => '╮',
                    None => '╭',
                });
            }
        }
        let lo = bend
            .iter()
            .position(Option::is_some)
            .map_or(self.lane, |p| p.min(self.lane));
        let hi = bend
            .iter()
            .rposition(Option::is_some)
            .map_or(self.lane, |p| p.max(self.lane));
        let mut s = String::with_capacity(width * 2);
        for (col, mark) in bend.iter().enumerate() {
            let glyph = if col == self.lane {
                '●'
            } else if let Some(b) = *mark {
                b
            } else if self.through.contains(&col) {
                '│'
            } else if col > lo && col < hi {
                '─'
            } else {
                ' '
            };
            s.push(glyph);
            s.push(if col >= lo && col < hi { '─' } else { ' ' });
        }
        s
    }
}

/// The first free slot in `lanes`, appending one when every slot is taken.
fn free_slot(lanes: &mut Vec<Option<&str>>) -> usize {
    if let Some(i) = lanes.iter().position(Option::is_none) {
        i
    } else {
        lanes.push(None);
        lanes.len() - 1
    }
}

/// Assign every commit a lane.
///
/// `commits` must be newest first with every parent after its children (what
/// `git log --topo-order` gives); parents that are not in the slice (history
/// cut off by paging) simply keep their lane open to the bottom. The result
/// has one [`Row`] per commit, in the same order. Appending commits to the
/// slice never changes the rows already computed for its prefix, so a page
/// load only needs to recompute, not redraw differently.
#[must_use]
pub fn assign_lanes(commits: &[Commit]) -> Vec<Row> {
    // `lanes[i]` is the hash that lane `i` is waiting to reach.
    let mut lanes: Vec<Option<&str>> = Vec::new();
    let mut rows = Vec::with_capacity(commits.len());
    for c in commits {
        let incoming: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter(|(_, l)| **l == Some(c.sha.as_str()))
            .map(|(i, _)| i)
            .collect();
        let lane = match incoming.first() {
            Some(&i) => i,
            None => free_slot(&mut lanes),
        };
        for &i in &incoming {
            lanes[i] = None;
        }
        let through: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter(|(_, l)| l.is_some())
            .map(|(i, _)| i)
            .collect();
        let mut outgoing = Vec::with_capacity(c.parents.len());
        for (k, parent) in c.parents.iter().enumerate() {
            let waiting = lanes.iter().position(|l| *l == Some(parent.as_str()));
            match (waiting, k) {
                (Some(i), _) => outgoing.push(i),
                (None, 0) => {
                    lanes[lane] = Some(parent.as_str());
                    outgoing.push(lane);
                }
                (None, _) => {
                    let slot = free_slot(&mut lanes);
                    lanes[slot] = Some(parent.as_str());
                    outgoing.push(slot);
                }
            }
        }
        let width = incoming
            .iter()
            .chain(&through)
            .chain(&outgoing)
            .copied()
            .chain(std::iter::once(lane))
            .max()
            .map_or(1, |m| m + 1);
        rows.push(Row {
            lane,
            incoming,
            through,
            outgoing,
            width,
        });
        while lanes.last().is_some_and(Option::is_none) {
            lanes.pop();
        }
    }
    rows
}

// ----- Panel state --------------------------------------------------------

/// A mutating per-commit action that needs a yes/no first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Check the commit (or its local branch) out.
    Checkout,
    /// Cherry-pick the commit onto the current branch.
    CherryPick,
}

/// What the panel's keys currently mean.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Typing searches; arrows move; action keys act.
    Browse,
    /// Waiting for `y`/`n` before running `action` on the commit `sha`.
    Confirm {
        /// What to run on a yes.
        action: Action,
        /// The full hash of the commit it applies to.
        sha: String,
    },
    /// Typing the name of a new branch to create at the commit `sha`.
    NewBranch {
        /// The full hash the branch will start at.
        sha: String,
        /// The name typed so far.
        name: String,
    },
}

/// What [`checkout`] will do for a commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckoutTarget {
    /// Switch to this local branch, which points at the commit.
    Branch(String),
    /// Detach `HEAD` at this full hash.
    Detached(String),
}

impl Commit {
    /// Whether `query` (already lowercased) matches the hash, author, subject
    /// or any ref name.
    #[must_use]
    pub fn matches(&self, query: &str) -> bool {
        query.is_empty()
            || self.sha.starts_with(query)
            || self.abbrev.contains(query)
            || self.author.to_lowercase().contains(query)
            || self.subject.to_lowercase().contains(query)
            || self
                .refs
                .iter()
                .any(|r| r.name.to_lowercase().contains(query))
    }

    /// What checking this commit out means: its local branch when it has one
    /// that `HEAD` isn't already on, otherwise a detached `HEAD`.
    #[must_use]
    pub fn checkout_target(&self) -> CheckoutTarget {
        self.refs
            .iter()
            .find(|r| r.kind == RefKind::Branch && !r.is_head)
            .map_or_else(
                || CheckoutTarget::Detached(self.sha.clone()),
                |r| CheckoutTarget::Branch(r.name.clone()),
            )
    }
}

/// The Git graph overlay's state.
///
/// `selected` and `scroll` index the *visible* rows: the loaded commits that
/// match [`query`](Self::query). Lanes are always computed over every loaded
/// commit, so searching hides rows without breaking the lines between them.
pub struct Panel {
    /// Every commit loaded so far, newest first.
    pub commits: Vec<Commit>,
    /// Lane geometry for [`commits`](Self::commits), same order.
    pub rows: Vec<Row>,
    /// The search text (matched case-insensitively).
    pub query: String,
    /// The branch or tag the graph is limited to; `None` shows every ref.
    pub reference: Option<String>,
    /// Branch and tag names the filter cycles through.
    pub refs: Vec<String>,
    /// Index of the highlighted visible row.
    pub selected: usize,
    /// First visible row.
    pub scroll: usize,
    /// What the keys currently mean.
    pub mode: Mode,
    /// Whether git has no more commits to load.
    pub exhausted: bool,
    /// Indices into [`commits`](Self::commits) of the rows matching the query.
    visible: Vec<usize>,
}

impl Panel {
    /// A panel over the first page of `commits`. `refs` feeds the filter;
    /// `exhausted` says the page was the whole history.
    #[must_use]
    pub fn new(commits: Vec<Commit>, refs: Vec<String>, exhausted: bool) -> Self {
        let mut p = Panel {
            commits: Vec::new(),
            rows: Vec::new(),
            query: String::new(),
            reference: None,
            refs,
            selected: 0,
            scroll: 0,
            mode: Mode::Browse,
            exhausted,
            visible: Vec::new(),
        };
        p.set_commits(commits);
        p
    }

    fn set_commits(&mut self, commits: Vec<Commit>) {
        self.commits = commits;
        self.rows = assign_lanes(&self.commits);
        self.refilter();
    }

    fn refilter(&mut self) {
        let q = self.query.to_lowercase();
        self.visible = self
            .commits
            .iter()
            .enumerate()
            .filter(|(_, c)| c.matches(&q))
            .map(|(i, _)| i)
            .collect();
        self.selected = self.selected.min(self.visible.len().saturating_sub(1));
    }

    /// Append the next page of commits (a short page marks the history
    /// exhausted) and recompute the lanes.
    pub fn append_page(&mut self, page: Vec<Commit>, page_size: usize) {
        if page.len() < page_size {
            self.exhausted = true;
        }
        let mut all = std::mem::take(&mut self.commits);
        all.extend(page);
        self.set_commits(all);
    }

    /// Replace everything with a freshly loaded first page (after the ref
    /// filter changed or an action rewrote history).
    pub fn replace(&mut self, commits: Vec<Commit>, page_size: usize) {
        self.exhausted = commits.len() < page_size;
        self.selected = 0;
        self.scroll = 0;
        self.set_commits(commits);
    }

    /// The visible rows, as indices into [`commits`](Self::commits).
    #[must_use]
    pub fn visible(&self) -> &[usize] {
        &self.visible
    }

    /// Number of visible rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.visible.len()
    }

    /// Whether no loaded commit matches the query.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.visible.is_empty()
    }

    /// The widest lane geometry among the loaded rows, so every row's graph
    /// column lines up.
    #[must_use]
    pub fn graph_width(&self) -> usize {
        self.rows.iter().map(|r| r.width).max().unwrap_or(1)
    }

    /// Whether the highlight is close enough to the end that the next page
    /// should be loaded.
    #[must_use]
    pub fn wants_more(&self) -> bool {
        !self.exhausted && self.selected + 20 >= self.visible.len()
    }

    /// Append a character to the search and reset the highlight.
    pub fn push(&mut self, c: char) {
        self.query.push(c);
        self.selected = 0;
        self.scroll = 0;
        self.refilter();
    }

    /// Remove the last character of the search.
    pub fn backspace(&mut self) {
        self.query.pop();
        self.selected = 0;
        self.scroll = 0;
        self.refilter();
    }

    /// Move the highlight up one row.
    pub fn up(&mut self) {
        self.selected = vix_list_state::up(self.selected);
    }

    /// Move the highlight down one row.
    pub fn down(&mut self) {
        self.selected = vix_list_state::down(self.selected, self.len());
    }

    /// Move the highlight up one page.
    pub fn page_up(&mut self, page: usize) {
        self.selected = vix_list_state::page_up(self.selected, page);
    }

    /// Move the highlight down one page.
    pub fn page_down(&mut self, page: usize) {
        self.selected = vix_list_state::page_down(self.selected, page, self.len());
    }

    /// Select a visible row directly (a click); whether it was a real row.
    pub fn select_index(&mut self, idx: usize) -> bool {
        match vix_list_state::select_index(idx, self.len()) {
            Some(i) => {
                self.selected = i;
                true
            }
            None => false,
        }
    }

    /// Keep the highlight inside `height` visible rows.
    pub fn ensure_visible(&mut self, height: usize) {
        self.scroll =
            vix_list_state::ensure_visible(self.selected, self.scroll, height, self.len());
    }

    /// The highlighted commit.
    #[must_use]
    pub fn selected_commit(&self) -> Option<&Commit> {
        self.commits.get(*self.visible.get(self.selected)?)
    }

    /// Step the branch/tag filter: all refs, then each of
    /// [`refs`](Self::refs) in turn, then all again. The caller reloads the
    /// first page with [`replace`](Self::replace) afterwards.
    pub fn cycle_reference(&mut self, forward: bool) {
        let n = self.refs.len() + 1;
        let cur = self
            .reference
            .as_ref()
            .and_then(|r| self.refs.iter().position(|x| x == r))
            .map_or(0, |i| i + 1);
        let next = if forward {
            (cur + 1) % n
        } else {
            (cur + n - 1) % n
        };
        self.reference = next.checked_sub(1).and_then(|i| self.refs.get(i)).cloned();
    }
}

// ----- Running git --------------------------------------------------------

/// Whether `s` is a plausible full or abbreviated commit hash.
#[must_use]
pub fn valid_sha(s: &str) -> bool {
    (4..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Whether `s` is safe to hand git as a branch, tag or revision name.
#[must_use]
pub fn valid_name(s: &str) -> bool {
    !s.is_empty() && !s.starts_with('-') && !s.chars().any(|c| c.is_whitespace() || c.is_control())
}

#[cfg(feature = "process")]
fn run(dir: &Path, args: &[&str]) -> Result<String, GraphError> {
    // Built by the workspace authority (T710): a remote workspace runs git
    // where its files are.
    let out = vix_authority::current()
        .command(
            &vix_authority::ProcessSpec::new("git")
                .arg("-C")
                .arg(dir)
                .args(args),
        )?
        .output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(GraphError::Failed {
            code: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        })
    }
}

/// Load one page of the graph: up to `count` commits after skipping `skip`,
/// limited to `reference` (a branch or tag) or, with `None`, across every ref.
/// An empty repository yields an empty page.
///
/// # Errors
/// [`GraphError::InvalidArgument`] for an option-shaped `reference`; otherwise
/// whatever running or parsing `git log` produced.
#[cfg(feature = "process")]
pub fn fetch_page(
    dir: &Path,
    skip: usize,
    count: usize,
    reference: Option<&str>,
) -> Result<Vec<Commit>, GraphError> {
    let skip = skip.to_string();
    let count = count.to_string();
    let mut args = vec![
        "log",
        "--topo-order",
        "--decorate=full",
        "--date=short",
        LOG_FORMAT,
        "--skip",
        &skip,
        "-n",
        &count,
    ];
    match reference {
        Some(r) if valid_name(r) => {
            args.push("--end-of-options");
            args.push(r);
        }
        Some(r) => {
            return Err(GraphError::InvalidArgument(format!(
                "invalid ref name: {r:?}"
            )));
        }
        None => args.push("--all"),
    }
    match run(dir, &args) {
        Ok(out) => parse_log(&out),
        Err(GraphError::Failed { stderr, .. })
            if stderr.contains("does not have any commits yet") =>
        {
            Ok(Vec::new())
        }
        Err(e) => Err(e),
    }
}

/// The branch and tag names the filter can pick, branches first.
///
/// # Errors
/// Whatever running `git for-each-ref` produced.
#[cfg(feature = "process")]
pub fn list_refs(dir: &Path) -> Result<Vec<String>, GraphError> {
    let out = run(
        dir,
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ],
    )?;
    Ok(out
        .lines()
        .filter(|l| !l.is_empty() && !l.ends_with("/HEAD"))
        .map(str::to_string)
        .collect())
}

/// Check `commit` out: its local branch if it has one, else a detached `HEAD`.
///
/// # Errors
/// [`GraphError::Failed`] when git refuses (for example a dirty tree).
#[cfg(feature = "process")]
pub fn checkout(dir: &Path, commit: &Commit) -> Result<CheckoutTarget, GraphError> {
    let target = commit.checkout_target();
    match &target {
        CheckoutTarget::Branch(b) if valid_name(b) => {
            run(dir, &["switch", "--end-of-options", b])?;
        }
        CheckoutTarget::Detached(sha) if valid_sha(sha) => {
            run(dir, &["switch", "--detach", "--end-of-options", sha])?;
        }
        _ => {
            return Err(GraphError::InvalidArgument(format!(
                "cannot check out {:?}",
                commit.abbrev
            )));
        }
    }
    Ok(target)
}

/// Create the branch `name` at `sha` and switch to it.
///
/// # Errors
/// [`GraphError::InvalidArgument`] for a bad name or hash; otherwise git's own
/// failure (for example the branch already exists).
#[cfg(feature = "process")]
pub fn create_branch_at(dir: &Path, name: &str, sha: &str) -> Result<(), GraphError> {
    if !valid_name(name) {
        return Err(GraphError::InvalidArgument(format!(
            "invalid branch name: {name:?}"
        )));
    }
    if !valid_sha(sha) {
        return Err(GraphError::InvalidArgument(format!(
            "invalid commit: {sha:?}"
        )));
    }
    run(dir, &["switch", "-c", name, "--end-of-options", sha]).map(|_| ())
}

/// Cherry-pick `sha` onto the current branch. When git stops (a conflict, an
/// empty pick) the pick is aborted so the repository is left as it was.
///
/// # Errors
/// [`GraphError::InvalidArgument`] for a bad hash; otherwise git's own failure.
#[cfg(feature = "process")]
pub fn cherry_pick(dir: &Path, sha: &str) -> Result<(), GraphError> {
    if !valid_sha(sha) {
        return Err(GraphError::InvalidArgument(format!(
            "invalid commit: {sha:?}"
        )));
    }
    match run(dir, &["cherry-pick", "--end-of-options", sha]) {
        Ok(_) => Ok(()),
        Err(e) => {
            // Best effort: nothing to abort is not a second failure to report.
            run(dir, &["cherry-pick", "--abort"]).ok();
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(sha: &str, parents: &[&str]) -> Commit {
        Commit {
            sha: sha.to_string(),
            abbrev: sha.to_string(),
            parents: parents.iter().map(|p| (*p).to_string()).collect(),
            author: "Ann".to_string(),
            date: "2026-01-01".to_string(),
            subject: format!("commit {sha}"),
            refs: Vec::new(),
        }
    }

    #[test]
    fn linear_history_is_one_lane() {
        let rows = assign_lanes(&[c("c", &["b"]), c("b", &["a"]), c("a", &[])]);
        assert!(rows.iter().all(|r| r.lane == 0 && r.width == 1));
        assert_eq!(rows[1].incoming, vec![0]);
        assert_eq!(rows[2].outgoing.len(), 0);
    }

    #[test]
    fn merge_opens_and_closes_a_second_lane() {
        // m merges b (first parent) and f (second); both descend from a.
        let commits = [
            c("m", &["b", "f"]),
            c("f", &["a"]),
            c("b", &["a"]),
            c("a", &[]),
        ];
        let rows = assign_lanes(&commits);
        assert_eq!(rows[0].outgoing, vec![0, 1]);
        assert_eq!(rows[1].lane, 1);
        assert_eq!(rows[1].through, vec![0]);
        assert_eq!(rows[2].lane, 0);
        // f already waits for a in lane 1, so b bends over to join it.
        assert_eq!(rows[2].outgoing, vec![1]);
        assert_eq!(rows[3].lane, 1);
        assert_eq!(rows[3].incoming, vec![1]);
    }

    #[test]
    fn glyphs_draw_through_lanes() {
        let rows = assign_lanes(&[c("m", &["b", "f"]), c("f", &["a"]), c("b", &["a"])]);
        assert_eq!(rows[0].glyphs(2), "●─╮ ");
        assert_eq!(rows[1].glyphs(2), "│ ● ");
    }

    #[test]
    fn parents_missing_from_the_page_keep_their_lane() {
        let rows = assign_lanes(&[c("x", &["gone"]), c("y", &["gone"])]);
        assert_eq!(rows[1].lane, 1);
        assert_eq!(rows[1].through, vec![0]);
        let rows = assign_lanes(&[c("x", &["gx"]), c("y", &["gy"])]);
        assert_eq!(rows[1].lane, 1);
        assert_eq!(rows[1].through, vec![0]);
    }

    #[test]
    fn decorations_parse_all_kinds() {
        let refs = parse_decorations(
            "HEAD -> refs/heads/main, tag: refs/tags/v1, refs/remotes/origin/main, refs/remotes/origin/HEAD, refs/heads/feature/x",
        );
        let names: Vec<_> = refs.iter().map(|r| (r.name.as_str(), r.kind)).collect();
        assert_eq!(
            names,
            vec![
                ("main", RefKind::Branch),
                ("v1", RefKind::Tag),
                ("origin/main", RefKind::Remote),
                ("feature/x", RefKind::Branch),
            ]
        );
        assert!(refs[0].is_head);
        assert_eq!(parse_decorations("HEAD")[0].kind, RefKind::Head);
        assert_eq!(parse_decorations("").len(), 0);
    }

    #[test]
    fn parse_log_reads_records_and_rejects_garbage() {
        let (f, r) = (FIELD_SEP, RECORD_SEP);
        let out = format!(
            "aaa{f}aa{f}bbb ccc{f}Ann{f}2026-01-02{f}HEAD -> refs/heads/main{f}fix: a thing{r}\nbbb{f}bb{f}{f}Bob{f}2026-01-01{f}{f}root{r}\n"
        );
        let commits = parse_log(&out).unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].parents, vec!["bbb", "ccc"]);
        assert_eq!(commits[0].refs[0].name, "main");
        assert_eq!(commits[1].parents.len(), 0);
        assert_eq!(commits[1].subject, "root");
        assert!(matches!(
            parse_log("a\u{1f}b\u{1e}"),
            Err(GraphError::Parse(_))
        ));
    }

    #[test]
    fn panel_searches_filters_and_pages() {
        let mut p = Panel::new(
            vec![c("c3", &["c2"]), c("c2", &["c1"]), c("c1", &[])],
            vec!["main".to_string(), "v1".to_string()],
            false,
        );
        assert_eq!(p.len(), 3);
        p.push('2');
        assert_eq!(p.selected_commit().unwrap().sha, "c2");
        p.backspace();
        p.down();
        assert_eq!(p.selected, 1);
        p.append_page(vec![c("c0", &[])], 200);
        assert!(p.exhausted);
        assert_eq!(p.len(), 4);
        p.cycle_reference(true);
        assert_eq!(p.reference.as_deref(), Some("main"));
        p.cycle_reference(true);
        p.cycle_reference(true);
        assert_eq!(p.reference, None);
        p.cycle_reference(false);
        assert_eq!(p.reference.as_deref(), Some("v1"));
    }

    #[test]
    fn checkout_target_prefers_a_local_branch() {
        let mut commit = c("abcd", &[]);
        assert_eq!(
            commit.checkout_target(),
            CheckoutTarget::Detached("abcd".to_string())
        );
        commit.refs = parse_decorations("HEAD -> refs/heads/main, refs/heads/topic");
        assert_eq!(
            commit.checkout_target(),
            CheckoutTarget::Branch("topic".to_string())
        );
    }

    #[test]
    fn validation_refuses_option_shaped_input() {
        assert!(valid_sha("deadbeef"));
        assert!(!valid_sha("--all"));
        assert!(!valid_sha("abc"));
        assert!(valid_name("feature/x"));
        assert!(!valid_name("-x"));
        assert!(!valid_name("a b"));
    }
}
