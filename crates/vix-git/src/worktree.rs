//! Worktrees (T634): parse `git worktree list --porcelain`, and add or
//! remove a linked worktree.

#[cfg(feature = "process")]
use std::path::Path;
use std::path::PathBuf;

#[cfg(feature = "process")]
use crate::GitError;

/// One worktree from `git worktree list --porcelain`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Worktree {
    /// The worktree's directory, as git spells it (absolute, on the machine
    /// that runs git — the remote host under a remote workspace).
    pub path: PathBuf,
    /// The checked-out commit's full hash (absent for a bare repository).
    pub head: Option<String>,
    /// What it has checked out.
    pub checkout: Checkout,
    /// Locked against `git worktree remove`/`prune`.
    pub locked: bool,
    /// Its directory is gone; `git worktree prune` would drop it.
    pub prunable: bool,
}

/// What a worktree has checked out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Checkout {
    /// A branch, named without `refs/heads/`.
    Branch(String),
    /// A detached `HEAD` (also the state before a `branch` line is read).
    #[default]
    Detached,
    /// The bare repository entry, which has no working tree.
    Bare,
}

impl Worktree {
    /// The checked-out branch, if any.
    #[must_use]
    pub fn branch(&self) -> Option<&str> {
        match &self.checkout {
            Checkout::Branch(b) => Some(b),
            Checkout::Detached | Checkout::Bare => None,
        }
    }
}

/// Parse `git worktree list --porcelain`: blank-line-separated records,
/// each starting with `worktree <path>` and followed by `HEAD <sha>`,
/// `branch <ref>`, `bare`, `detached`, `locked [reason]` and `prunable
/// [reason]` lines. Unknown lines are ignored (git may add attributes), and
/// a line before any `worktree` line is dropped. The first record is always
/// the main worktree.
#[must_use]
pub fn parse_worktree_list(text: &str) -> Vec<Worktree> {
    let mut out: Vec<Worktree> = Vec::new();
    let mut current: Option<Worktree> = None;
    for line in text.lines() {
        if line.is_empty() {
            out.extend(current.take());
            continue;
        }
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        if key == "worktree" {
            out.extend(current.take());
            current = Some(Worktree {
                path: PathBuf::from(value),
                ..Worktree::default()
            });
            continue;
        }
        let Some(w) = current.as_mut() else {
            continue;
        };
        match key {
            "HEAD" => w.head = Some(value.to_string()),
            "branch" => {
                w.checkout = Checkout::Branch(
                    value
                        .strip_prefix("refs/heads/")
                        .unwrap_or(value)
                        .to_string(),
                );
            }
            "bare" => w.checkout = Checkout::Bare,
            "detached" => w.checkout = Checkout::Detached,
            "locked" => w.locked = true,
            "prunable" => w.prunable = true,
            _ => {}
        }
    }
    out.extend(current);
    out
}

#[cfg(feature = "process")]
/// Every worktree of `dir`'s repository, the main one first
/// (**Git → Worktree → List…**). Empty outside a repository.
#[must_use]
pub fn worktree_list(dir: &Path) -> Vec<Worktree> {
    crate::run(dir, &["worktree", "list", "--porcelain"])
        .map(|s| parse_worktree_list(&s))
        .unwrap_or_default()
}

#[cfg(feature = "process")]
/// Refuse a path git would read as an option.
fn check_path(path: &str) -> Result<(), GitError> {
    if path.is_empty() || path.starts_with('-') || path.chars().any(char::is_control) {
        return Err(GitError::InvalidArgument(path.to_string()));
    }
    Ok(())
}

#[cfg(feature = "process")]
/// Add a linked worktree at `path` (relative to `dir`, or absolute) on a
/// new branch named after its last component, from `HEAD` — git's own
/// default for `git worktree add <path>`.
///
/// # Errors
/// [`GitError::InvalidArgument`] for an empty or option-shaped `path`;
/// git's stderr on failure (e.g. the path exists, or the branch does).
pub fn worktree_add(dir: &Path, path: &str) -> Result<(), GitError> {
    check_path(path)?;
    crate::run(dir, &["worktree", "add", path]).map(drop)
}

#[cfg(feature = "process")]
/// Remove the linked worktree at `path` (`git worktree remove`). git refuses
/// one with uncommitted changes or untracked files, and the main worktree;
/// nothing here forces it. The host confirms first.
///
/// # Errors
/// [`GitError::InvalidArgument`] for an empty or option-shaped `path`;
/// git's stderr on failure.
pub fn worktree_remove(dir: &Path, path: &str) -> Result<(), GitError> {
    check_path(path)?;
    crate::run(dir, &["worktree", "remove", path]).map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_records() {
        let text = "worktree /src/repo\n\
                    HEAD 1111111111111111111111111111111111111111\n\
                    branch refs/heads/main\n\
                    \n\
                    worktree /src/repo-feature\n\
                    HEAD 2222222222222222222222222222222222222222\n\
                    branch refs/heads/feature/x\n\
                    locked moving disks\n\
                    \n\
                    worktree /src/repo-detached\n\
                    HEAD 3333333333333333333333333333333333333333\n\
                    detached\n\
                    prunable gitdir file points to non-existent location\n\
                    \n";
        let got = parse_worktree_list(text);
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].path, PathBuf::from("/src/repo"));
        assert_eq!(got[0].branch(), Some("main"));
        assert_eq!(got[1].branch(), Some("feature/x"));
        assert!(got[1].locked && !got[1].prunable);
        assert!(got[2].prunable && !got[2].locked);
        assert_eq!(got[2].checkout, Checkout::Detached);
        assert_eq!(
            got[2].head.as_deref(),
            Some("3333333333333333333333333333333333333333")
        );
    }

    #[test]
    fn parses_bare_entries_paths_with_spaces_and_no_trailing_blank() {
        let text = "worktree /srv/repo.git\nbare\n\nworktree /home/a b/wt\nHEAD abc\nbranch refs/heads/wt\nfuture-attr x";
        let got = parse_worktree_list(text);
        assert_eq!(got.len(), 2);
        assert!(got[0].checkout == Checkout::Bare && got[0].head.is_none());
        assert_eq!(got[1].path, PathBuf::from("/home/a b/wt"));
        assert_eq!(got[1].branch(), Some("wt"));
    }

    #[test]
    fn ignores_lines_before_the_first_record() {
        assert_eq!(parse_worktree_list("HEAD abc\nbare\n"), Vec::new());
        assert_eq!(parse_worktree_list(""), Vec::new());
    }
}
