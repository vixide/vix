//! Resolve the provider's directory for one isolated conversation. Git is
//! optional, including when reading a transcript created by a fuller build.
use std::io;
use std::path::{Path, PathBuf};

#[cfg(feature = "git")]
fn unavailable(path: &Path) -> io::Error {
    io::Error::other(t!("status.agent_worktree_unavailable", path = path.display()).to_string())
}

#[cfg(feature = "git")]
fn git(dir: &Path, args: &[&str]) -> io::Result<String> {
    crate::git::run(dir, args).map_err(|e| io::Error::other(super::super::git::git_error_text(&e)))
}

#[cfg(feature = "git")]
fn git_path(dir: &Path, option: &str) -> io::Result<PathBuf> {
    Path::new(&git(dir, &["rev-parse", "--path-format=absolute", option])?).canonicalize()
}

/// Only a newly chosen conversation can create a checkout. A reopened
/// transcript always validates its binding, even on the very first turn.
#[cfg(feature = "git")]
pub(super) fn directory(root: &Path, checkout: &Path, create: &mut bool) -> io::Result<PathBuf> {
    let fail = || unavailable(checkout);
    // Git hooks can launch an editor with a different index or working tree
    // in its environment. A child using those variables could edit that tree.
    if [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
    ]
    .iter()
    .any(|key| std::env::var_os(key).is_some())
    {
        return Err(fail());
    }
    let repo = git_path(root, "--show-toplevel")?;
    let root = root.canonicalize()?;
    let prefix = root.strip_prefix(&repo).map_err(|_| fail())?;
    let branch = checkout
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(fail)?;
    let parent = checkout.parent().ok_or_else(fail)?.canonicalize()?;
    if parent.starts_with(&repo) {
        return Err(fail());
    }
    let path = parent.join(branch);
    if *create {
        // Explicit -b refuses an existing branch (Git's implicit naming can
        // otherwise reuse one). No reset, forced checkout or branch deletion.
        let destination = path.to_str().ok_or_else(fail)?;
        // Claim the empty folder exclusively. Git itself accepts an existing
        // empty destination, including one reached through a symlink.
        std::fs::create_dir(&path)?;
        if let Err(error) = git(
            &root,
            &["worktree", "add", "-b", branch, "--", destination, "HEAD"],
        ) {
            // Git may already have removed it. Never discard partial files.
            let _ = std::fs::remove_dir(&path);
            return Err(error);
        }
        *create = false;
    }
    verify(&root, &path, branch)?;
    let cwd = path.join(prefix);
    if cwd.is_dir() && cwd.canonicalize()? == cwd {
        Ok(cwd)
    } else {
        Err(fail())
    }
}

/// Check the working tree and its registration in the source repository.
/// The NUL-delimited listing preserves whitespace and non-ASCII names.
#[cfg(feature = "git")]
fn verify(root: &Path, path: &Path, branch: &str) -> io::Result<()> {
    if !path.join(".git").is_file()
        || git_path(path, "--show-toplevel")? != path
        || git_path(path, "--git-common-dir")? != git_path(root, "--git-common-dir")?
        || git(path, &["symbolic-ref", "HEAD"])? != format!("refs/heads/{branch}")
    {
        return Err(unavailable(path));
    }
    let listing = git(root, &["worktree", "list", "--porcelain", "-z"])?;
    let registered = listing.split("\0\0").any(|record| {
        let mut fields = record.split('\0');
        let Some(location) = fields.next().and_then(|s| s.strip_prefix("worktree ")) else {
            return false;
        };
        fields.any(|s| s.strip_prefix("branch refs/heads/") == Some(branch))
            && Path::new(location).canonicalize().is_ok_and(|p| p == path)
    });
    if registered {
        Ok(())
    } else {
        Err(unavailable(path))
    }
}

#[cfg(not(feature = "git"))]
pub(super) fn directory(_root: &Path, _checkout: &Path, _create: &mut bool) -> io::Result<PathBuf> {
    Err(io::Error::other(
        t!("msg.capability_missing", name = "git").to_string(),
    ))
}
