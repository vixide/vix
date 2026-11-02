//! Interactive rebase and cherry-pick (T633).
//!
//! The pure half reads and writes git's rebase todo list ([`parse_todo`],
//! [`serialize_todo`]), checks it ([`check_todo`]), turns an edited list into
//! what git is handed ([`plan`]), and holds the overlay's state
//! ([`RebaseTodo`]). The runners start a rebase with that list, continue or
//! abort one, and cherry-pick a commit.
//!
//! No interactive editor ever opens. `git rebase -i` is run with
//! `GIT_SEQUENCE_EDITOR` set to a one-line shell command that writes the
//! prepared list over git's own (the list itself travels in an environment
//! variable, so no temporary file is needed and a remote host gets it too),
//! and `GIT_EDITOR=true`, which accepts the message git proposes for a squash.
//! Both are set on the spawned process only, never on Vix's own environment.

#[cfg(feature = "process")]
use std::path::Path;

#[cfg(feature = "process")]
use crate::GitError;

/// What a rebase does with one commit: the todo list's command word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RebaseAction {
    /// Keep the commit as it is (`pick`).
    Pick,
    /// Keep the commit with a new message (`reword`).
    Reword,
    /// Stop after applying the commit, to amend it (`edit`).
    Edit,
    /// Meld into the previous commit, keeping both messages (`squash`).
    Squash,
    /// Meld into the previous commit, discarding this message (`fixup`).
    Fixup,
    /// Leave the commit out (`drop`).
    Drop,
}

impl RebaseAction {
    /// Every action, in the order the overlay's keys list them.
    pub const ALL: [RebaseAction; 6] = [
        RebaseAction::Pick,
        RebaseAction::Reword,
        RebaseAction::Edit,
        RebaseAction::Squash,
        RebaseAction::Fixup,
        RebaseAction::Drop,
    ];

    /// The todo list's long command word (`pick`, `reword`, …).
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            RebaseAction::Pick => "pick",
            RebaseAction::Reword => "reword",
            RebaseAction::Edit => "edit",
            RebaseAction::Squash => "squash",
            RebaseAction::Fixup => "fixup",
            RebaseAction::Drop => "drop",
        }
    }

    /// The one-letter abbreviation git accepts, which is also the overlay's
    /// key for the action (`p`, `r`, `e`, `s`, `f`, `d`).
    #[must_use]
    pub fn key(self) -> char {
        match self {
            RebaseAction::Pick => 'p',
            RebaseAction::Reword => 'r',
            RebaseAction::Edit => 'e',
            RebaseAction::Squash => 's',
            RebaseAction::Fixup => 'f',
            RebaseAction::Drop => 'd',
        }
    }

    /// The action for a todo command word, long or abbreviated.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|a| a.word() == word || word.len() == 1 && word.starts_with(a.key()))
    }

    /// The action an overlay key selects.
    #[must_use]
    pub fn from_key(key: char) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.key() == key)
    }

    /// Whether the commit melds into the one before it (`squash`, `fixup`).
    #[must_use]
    pub fn melds(self) -> bool {
        matches!(self, RebaseAction::Squash | RebaseAction::Fixup)
    }
}

/// One commit line of a todo list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TodoCommit {
    /// What to do with the commit.
    pub action: RebaseAction,
    /// The commit's hash, as written in the list (Vix writes the full hash).
    pub sha: String,
    /// The commit's subject line (informational: git ignores it).
    pub subject: String,
    /// The replacement message for a `reword`, when one was typed. `None`
    /// keeps the commit's own message.
    pub message: Option<String>,
}

impl TodoCommit {
    /// A `pick` of `sha` with `subject`.
    #[must_use]
    pub fn pick(sha: impl Into<String>, subject: impl Into<String>) -> Self {
        TodoCommit {
            action: RebaseAction::Pick,
            sha: sha.into(),
            subject: subject.into(),
            message: None,
        }
    }

    /// The hash's first seven characters, for display.
    #[must_use]
    pub fn abbrev(&self) -> &str {
        self.sha.get(..7).unwrap_or(&self.sha)
    }
}

/// One line of a todo list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TodoItem {
    /// A commit line Vix understands (`pick`, `reword`, `edit`, `squash`,
    /// `fixup`, `drop`).
    Commit(TodoCommit),
    /// Any other command (`exec`, `break`, `label`, `reset`, `merge`,
    /// `update-ref`, `fixup -C`, …), kept verbatim.
    Other(String),
}

/// Parse a rebase todo list. Blank lines and `#` comments are skipped (git
/// ignores them too); a commit line with an option (`fixup -C <sha>`) or a
/// command Vix doesn't edit is kept as [`TodoItem::Other`], so writing the
/// list back with [`serialize_todo`] loses nothing git would act on.
#[must_use]
pub fn parse_todo(text: &str) -> Vec<TodoItem> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|line| {
            let mut parts = line.splitn(3, char::is_whitespace);
            let word = parts.next().unwrap_or_default();
            let sha = parts.next().unwrap_or_default();
            let subject = parts.next().unwrap_or_default().trim();
            match RebaseAction::from_word(word) {
                Some(action) if !sha.is_empty() && !sha.starts_with('-') => {
                    TodoItem::Commit(TodoCommit {
                        action,
                        sha: sha.to_string(),
                        subject: subject.to_string(),
                        message: None,
                    })
                }
                _ => TodoItem::Other(line.to_string()),
            }
        })
        .collect()
}

/// Write a todo list in git's format: one `<word> <sha> <subject>` line per
/// commit, other lines verbatim, each ending in a newline. A reword's typed
/// message isn't part of the format; [`plan`] carries it.
#[must_use]
pub fn serialize_todo(items: &[TodoItem]) -> String {
    let mut out = String::new();
    for item in items {
        match item {
            TodoItem::Commit(c) => {
                out.push_str(c.action.word());
                out.push(' ');
                out.push_str(&c.sha);
                if !c.subject.is_empty() {
                    out.push(' ');
                    out.push_str(&c.subject);
                }
            }
            TodoItem::Other(line) => out.push_str(line),
        }
        out.push('\n');
    }
    out
}

/// Why a todo list can't be run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TodoProblem {
    /// The list has no commits at all.
    Empty,
    /// The first kept commit is a `squash` or `fixup`, which has nothing
    /// before it to meld into.
    MeldsFirst,
}

/// Check an edited list before handing it to git, which would otherwise
/// stop half-way with "cannot 'squash' without a previous commit".
///
/// # Errors
/// The first [`TodoProblem`] found.
pub fn check_todo(commits: &[TodoCommit]) -> Result<(), TodoProblem> {
    if commits.is_empty() {
        return Err(TodoProblem::Empty);
    }
    match commits.iter().find(|c| c.action != RebaseAction::Drop) {
        Some(c) if c.action.melds() => Err(TodoProblem::MeldsFirst),
        _ => Ok(()),
    }
}

/// What [`plan`] hands git: the todo text and the environment variables it
/// refers to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RebasePlan {
    /// The todo list to write over git's own.
    pub todo: String,
    /// Environment variables the list's `exec` lines read (`VIX_REWORD_<n>`:
    /// a reword's typed message).
    pub env: Vec<(String, String)>,
}

/// Turn an edited list into what git runs. A `reword` with a typed message
/// becomes a `pick` followed by
/// `exec git commit --amend --allow-empty -m "$VIX_REWORD_<n>"`, the message
/// travelling in that environment variable, so no message editor opens and
/// no message text is ever parsed by a shell. A `reword` without one stays a
/// `reword`, and `GIT_EDITOR=true` keeps its message.
#[must_use]
pub fn plan(commits: &[TodoCommit]) -> RebasePlan {
    let mut items = Vec::with_capacity(commits.len());
    let mut env = Vec::new();
    for c in commits {
        match (&c.message, c.action) {
            (Some(message), RebaseAction::Reword) => {
                let var = format!("VIX_REWORD_{}", env.len());
                items.push(TodoItem::Commit(TodoCommit {
                    action: RebaseAction::Pick,
                    ..c.clone()
                }));
                items.push(TodoItem::Other(format!(
                    "exec git commit --amend --allow-empty -m \"${var}\""
                )));
                env.push((var, message.clone()));
            }
            _ => items.push(TodoItem::Commit(c.clone())),
        }
    }
    RebasePlan {
        todo: serialize_todo(&items),
        env,
    }
}

/// The interactive rebase overlay's state: the commits after the base, oldest
/// first (the order git applies them), a highlighted row, and a scroll offset.
#[derive(Clone, Debug)]
pub struct RebaseTodo {
    /// The base commit's full hash: the rebase replays everything after it.
    pub base: String,
    /// The base as typed (a branch, tag, or `HEAD~3`), for the title.
    pub base_label: String,
    /// The commit lines, oldest first.
    pub commits: Vec<TodoCommit>,
    /// Index of the highlighted row.
    pub selected: usize,
    /// First visible row.
    pub scroll: usize,
}

impl RebaseTodo {
    /// A fresh overlay over `commits`, every one a `pick`, highlighting the
    /// first.
    #[must_use]
    pub fn new(base: String, base_label: String, commits: Vec<TodoCommit>) -> Self {
        RebaseTodo {
            base,
            base_label,
            commits,
            selected: 0,
            scroll: 0,
        }
    }

    /// Move the highlight up one row.
    pub fn up(&mut self) {
        self.selected = vix_list_state::up(self.selected);
    }

    /// Move the highlight down one row.
    pub fn down(&mut self) {
        self.selected = vix_list_state::down(self.selected, self.commits.len());
    }

    /// Move the highlighted commit one row earlier (`Alt+↑`); the highlight
    /// follows it.
    pub fn move_up(&mut self) {
        if self.selected > 0 && self.selected < self.commits.len() {
            self.commits.swap(self.selected, self.selected - 1);
            self.selected -= 1;
        }
    }

    /// Move the highlighted commit one row later (`Alt+↓`); the highlight
    /// follows it.
    pub fn move_down(&mut self) {
        if self.selected + 1 < self.commits.len() {
            self.commits.swap(self.selected, self.selected + 1);
            self.selected += 1;
        }
    }

    /// Set the highlighted commit's action. Choosing anything but `reword`
    /// forgets a typed message.
    pub fn set_action(&mut self, action: RebaseAction) {
        if let Some(c) = self.commits.get_mut(self.selected) {
            c.action = action;
            if action != RebaseAction::Reword {
                c.message = None;
            }
        }
    }

    /// Give the highlighted commit a `reword` with `message`.
    pub fn set_message(&mut self, message: String) {
        if let Some(c) = self.commits.get_mut(self.selected) {
            c.action = RebaseAction::Reword;
            c.message = Some(message);
        }
    }

    /// The highlighted commit.
    #[must_use]
    pub fn selected_commit(&self) -> Option<&TodoCommit> {
        self.commits.get(self.selected)
    }

    /// Keep the highlighted row within a window of `height` visible rows.
    pub fn ensure_visible(&mut self, height: usize) {
        self.scroll =
            vix_list_state::ensure_visible(self.selected, self.scroll, height, self.commits.len());
    }
}

#[cfg(feature = "process")]
/// Where a rebase stands after a runner returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RebaseState {
    /// No rebase is in progress: it finished (or was aborted).
    Finished,
    /// The rebase is stopped, at an `edit` or on a conflict; finish with
    /// [`rebase_continue`] or give up with [`rebase_abort`].
    Stopped,
}

#[cfg(feature = "process")]
/// Resolve `base` to a commit and list the commits a rebase onto it would
/// replay: `base..HEAD`, oldest first, without merges (which an interactive
/// rebase flattens away), each as a `pick`. Returns the base's full hash and
/// the commits.
///
/// # Errors
/// [`GitError::InvalidArgument`] for an empty or option-shaped base; git's
/// error when it doesn't name a commit.
pub fn rebase_commits(dir: &Path, base: &str) -> Result<(String, Vec<TodoCommit>), GitError> {
    if !crate::valid_ref_name(base) {
        return Err(GitError::InvalidArgument(format!(
            "invalid revision: {base:?}"
        )));
    }
    let spec = format!("{base}^{{commit}}");
    let sha = crate::run(dir, &["rev-parse", "--verify", "--end-of-options", &spec])?;
    let range = format!("{sha}..HEAD");
    let out = crate::run(
        dir,
        &[
            "log",
            "--reverse",
            "--no-merges",
            "--format=%H%x1f%s",
            &range,
        ],
    )?;
    let commits = out
        .lines()
        .filter_map(|l| {
            let (sha, subject) = l.split_once('\u{1f}')?;
            Some(TodoCommit::pick(sha, subject))
        })
        .collect();
    Ok((sha, commits))
}

#[cfg(feature = "process")]
/// `git rebase -i <base>` with `commits` as the todo list (see [`plan`]):
/// `GIT_SEQUENCE_EDITOR` writes the list over git's, and `GIT_EDITOR=true`
/// accepts a squash's combined message. Returns whether the rebase finished
/// or stopped (at an `edit`).
///
/// # Errors
/// [`GitError::InvalidArgument`] for an option-shaped base; git's error
/// otherwise — a dirty tree, or a conflict (after which the rebase is
/// stopped: see [`rebase_in_progress`]).
pub fn rebase_interactive(
    dir: &Path,
    base: &str,
    commits: &[TodoCommit],
) -> Result<RebaseState, GitError> {
    if !crate::valid_ref_name(base) {
        return Err(GitError::InvalidArgument(format!(
            "invalid revision: {base:?}"
        )));
    }
    let plan = plan(commits);
    let mut env: Vec<(&str, &str)> = vec![
        ("GIT_SEQUENCE_EDITOR", "printf '%s' \"$VIX_REBASE_TODO\" >"),
        ("VIX_REBASE_TODO", &plan.todo),
        ("GIT_EDITOR", "true"),
    ];
    env.extend(plan.env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    run_with_env(dir, &["rebase", "-i", base], &env)?;
    Ok(state(dir))
}

#[cfg(feature = "process")]
/// `git rebase --continue`, accepting any proposed message (`GIT_EDITOR=true`).
///
/// # Errors
/// git's error, such as unresolved conflicts.
pub fn rebase_continue(dir: &Path) -> Result<RebaseState, GitError> {
    run_with_env(dir, &["rebase", "--continue"], &[("GIT_EDITOR", "true")])?;
    Ok(state(dir))
}

#[cfg(feature = "process")]
/// `git rebase --abort`: put the branch back where it was.
///
/// # Errors
/// git's error, such as no rebase in progress.
pub fn rebase_abort(dir: &Path) -> Result<(), GitError> {
    crate::run(dir, &["rebase", "--abort"]).map(drop)
}

#[cfg(feature = "process")]
/// Whether a rebase is stopped in `dir`'s repository (git's `rebase-merge`
/// or `rebase-apply` directory exists).
#[must_use]
pub fn rebase_in_progress(dir: &Path) -> bool {
    let auth = vix_authority::current();
    ["rebase-merge", "rebase-apply"].into_iter().any(|name| {
        crate::git_stdout(dir, &["rev-parse", "--git-path", name])
            .is_some_and(|p| auth.exists(&dir.join(p)))
    })
}

#[cfg(feature = "process")]
/// The commits on `rev` that `HEAD` doesn't have (`git log HEAD..<rev>`),
/// newest first, at most `max`: what **Cherry-Pick…** offers.
///
/// # Errors
/// [`GitError::InvalidArgument`] for an empty or option-shaped revision;
/// git's error when it doesn't resolve.
pub fn cherry_pick_candidates(
    dir: &Path,
    rev: &str,
    max: usize,
) -> Result<Vec<crate::LogEntry>, GitError> {
    if !crate::valid_ref_name(rev) {
        return Err(GitError::InvalidArgument(format!(
            "invalid revision: {rev:?}"
        )));
    }
    let spec = format!("{rev}^{{commit}}");
    let sha = crate::run(dir, &["rev-parse", "--verify", "--end-of-options", &spec])?;
    let n = max.to_string();
    let fmt = crate::log_format_arg();
    let range = format!("HEAD..{sha}");
    let out = crate::run(dir, &["log", &fmt, "--date=short", "-n", &n, &range])?;
    Ok(crate::parse_log(&out))
}

#[cfg(feature = "process")]
/// `git cherry-pick <sha>`: apply one commit on top of `HEAD`.
///
/// # Errors
/// [`GitError::InvalidArgument`] for an option-shaped hash; git's error
/// otherwise, such as a conflict.
pub fn cherry_pick(dir: &Path, sha: &str) -> Result<(), GitError> {
    if !crate::valid_ref_name(sha) {
        return Err(GitError::InvalidArgument(format!(
            "invalid commit: {sha:?}"
        )));
    }
    run_with_env(
        dir,
        &["cherry-pick", "--end-of-options", sha],
        &[("GIT_EDITOR", "true")],
    )
}

#[cfg(feature = "process")]
fn state(dir: &Path) -> RebaseState {
    if rebase_in_progress(dir) {
        RebaseState::Stopped
    } else {
        RebaseState::Finished
    }
}

#[cfg(feature = "process")]
fn run_with_env(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<(), GitError> {
    let out = crate::git_command_env(dir, args, env)?.output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(GitError::from_output(&out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TODO: &str = "pick 1111111 first\nsquash 2222222 second\nexec make test\nfixup 3333333 third\ndrop 4444444 fourth\nreword 5555555 fifth\nedit 6666666 sixth\n";

    #[test]
    fn todo_round_trips() {
        let items = parse_todo(TODO);
        assert_eq!(items.len(), 7);
        assert_eq!(serialize_todo(&items), TODO);
        assert_eq!(parse_todo(&serialize_todo(&items)), items);
    }

    #[test]
    fn parse_reads_abbreviations_and_skips_comments() {
        let items = parse_todo("# Rebase 1..2\n\np abc one\ns def two words\nf 0a1 \nbreak\n");
        assert_eq!(
            items,
            vec![
                TodoItem::Commit(TodoCommit::pick("abc", "one")),
                TodoItem::Commit(TodoCommit {
                    action: RebaseAction::Squash,
                    ..TodoCommit::pick("def", "two words")
                }),
                TodoItem::Commit(TodoCommit {
                    action: RebaseAction::Fixup,
                    ..TodoCommit::pick("0a1", "")
                }),
                TodoItem::Other("break".into()),
            ]
        );
    }

    #[test]
    fn options_and_unknown_commands_stay_verbatim() {
        let items = parse_todo("fixup -C 123 msg\nupdate-ref refs/heads/x\n");
        assert_eq!(
            items,
            vec![
                TodoItem::Other("fixup -C 123 msg".into()),
                TodoItem::Other("update-ref refs/heads/x".into()),
            ]
        );
    }

    #[test]
    fn keys_and_words_agree() {
        for a in RebaseAction::ALL {
            assert_eq!(RebaseAction::from_key(a.key()), Some(a));
            assert_eq!(RebaseAction::from_word(a.word()), Some(a));
            assert_eq!(RebaseAction::from_word(&a.key().to_string()), Some(a));
        }
        assert_eq!(RebaseAction::from_word("exec"), None);
        assert_eq!(RebaseAction::from_key('x'), None);
    }

    #[test]
    fn check_rejects_a_leading_meld() {
        let mut c = vec![TodoCommit::pick("a", "1"), TodoCommit::pick("b", "2")];
        assert_eq!(check_todo(&c), Ok(()));
        c[0].action = RebaseAction::Squash;
        assert_eq!(check_todo(&c), Err(TodoProblem::MeldsFirst));
        c[0].action = RebaseAction::Drop;
        c[1].action = RebaseAction::Fixup;
        assert_eq!(check_todo(&c), Err(TodoProblem::MeldsFirst));
        assert_eq!(check_todo(&[]), Err(TodoProblem::Empty));
    }

    #[test]
    fn plan_turns_a_typed_reword_into_pick_and_exec() {
        let mut t = RebaseTodo::new(
            "base".into(),
            "HEAD~2".into(),
            vec![
                TodoCommit::pick("aaa", "one"),
                TodoCommit::pick("bbb", "two"),
            ],
        );
        t.down();
        t.set_message("new \"message\"\n\nbody".into());
        let p = plan(&t.commits);
        assert_eq!(
            p.todo,
            "pick aaa one\npick bbb two\nexec git commit --amend --allow-empty -m \"$VIX_REWORD_0\"\n"
        );
        assert_eq!(
            p.env,
            vec![("VIX_REWORD_0".into(), "new \"message\"\n\nbody".into())]
        );
        // A reword with no typed message stays a reword (`p` forgot it).
        t.set_action(RebaseAction::Pick);
        t.set_action(RebaseAction::Reword);
        assert_eq!(plan(&t.commits).todo, "pick aaa one\nreword bbb two\n");
        assert_eq!(plan(&t.commits).env, Vec::new());
    }

    #[test]
    fn reordering_moves_the_highlight_with_the_commit() {
        let mut t = RebaseTodo::new(
            "b".into(),
            "b".into(),
            vec![
                TodoCommit::pick("a", "1"),
                TodoCommit::pick("b", "2"),
                TodoCommit::pick("c", "3"),
            ],
        );
        t.move_up();
        assert_eq!(t.selected, 0);
        t.move_down();
        assert_eq!(t.selected, 1);
        assert_eq!(t.commits[1].sha, "a");
        t.move_down();
        t.move_down();
        assert_eq!(t.selected, 2);
        let order: Vec<&str> = t.commits.iter().map(|c| c.sha.as_str()).collect();
        assert_eq!(order, ["b", "c", "a"]);
        t.set_action(RebaseAction::Fixup);
        assert_eq!(
            t.selected_commit().map(|c| c.action),
            Some(RebaseAction::Fixup)
        );
    }
}
