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

/// Variables that point Git (and so a provider's own Git calls) at another
/// repository, index, object store or ref namespace. Git hooks export some
/// of them to an editor they launch; a child inheriting them could edit
/// that tree instead of the verified checkout.
#[cfg(feature = "git")]
const REDIRECTING_ENV: [&str; 7] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
];

/// Whether `root` can start a worktree conversation: it is a folder of a
/// Git working tree that its committed `HEAD` holds (so the new checkout
/// has the same folder), and no variable redirects Git elsewhere.
#[cfg(feature = "git")]
pub(super) fn available(root: &Path) -> bool {
    !redirected() && committed_folder(root)
}

#[cfg(feature = "git")]
fn redirected() -> bool {
    REDIRECTING_ENV
        .iter()
        .any(|key| std::env::var_os(key).is_some())
}

/// `HEAD:./` names the tree of the folder Git runs in; an unborn `HEAD`
/// or a folder no commit holds yet (untracked, ignored) has none.
#[cfg(feature = "git")]
fn committed_folder(root: &Path) -> bool {
    git(root, &["cat-file", "-t", "HEAD:./"]).is_ok_and(|kind| kind == "tree")
}

#[cfg(feature = "git")]
fn git_path(dir: &Path, option: &str) -> io::Result<PathBuf> {
    Path::new(&git(dir, &["rev-parse", "--path-format=absolute", option])?).canonicalize()
}

/// Only a conversation that has never run a turn can create a checkout, and
/// only while none exists at its path. Otherwise the binding is validated.
#[cfg(feature = "git")]
pub(super) fn directory(root: &Path, checkout: &Path, create: &mut bool) -> io::Result<PathBuf> {
    let fail = || unavailable(checkout);
    if redirected() {
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
    // A conversation whose first attempt failed after Git created the
    // checkout (the provider is not installed, say) validates it instead.
    if *create && std::fs::symlink_metadata(&path).is_ok() {
        *create = false;
    }
    if *create {
        if !committed_folder(&root) {
            return Err(fail());
        }
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
    verify(&root, &path, branch).map_err(|_| fail())?;
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

#[cfg(all(test, feature = "git"))]
mod tests {
    use super::*;
    use crate::app::git::tests::{git, repo, temp};
    use std::fs;

    /// `path` (below the canonical temp folder) spelled through the temp
    /// folder as the OS names it: on macOS `/var/…`, a symlink to
    /// `/private/var/…`.
    fn alias(path: &Path) -> PathBuf {
        let tmp = std::env::temp_dir();
        tmp.canonicalize()
            .ok()
            .and_then(|canonical| path.strip_prefix(canonical).ok().map(|rest| tmp.join(rest)))
            .unwrap_or_else(|| path.to_owned())
    }

    fn branches(repo: &Path) -> String {
        git(repo, &["branch", "--list", "vix-agent-*"])
    }

    #[test]
    fn creates_then_reuses_the_checkout_of_the_open_subfolder_through_symlinks() {
        let repo = repo("wt-create", &[("sub/a.txt", "a\n")]);
        let store = temp("wt-create-store");
        let root = alias(&repo.join("sub"));
        let checkout = alias(&store).join("vix-agent-test");
        assert!(available(&root));
        let mut create = true;
        let cwd = directory(&root, &checkout, &mut create).unwrap();
        assert!(!create);
        assert_eq!(cwd, store.join("vix-agent-test/sub"));
        assert_eq!(fs::read_to_string(cwd.join("a.txt")).unwrap(), "a\n");
        assert_eq!(git(&cwd, &["branch", "--show-current"]), "vix-agent-test");
        // Reopened, and retried after a first attempt that Git completed.
        for mut create in [false, true] {
            assert_eq!(directory(&root, &checkout, &mut create).unwrap(), cwd);
            assert!(!create);
        }
        // A detached or switched checkout is refused, then accepted again.
        git(&cwd, &["switch", "-q", "--detach"]);
        assert!(directory(&root, &checkout, &mut false).is_err());
        git(&cwd, &["switch", "-q", "vix-agent-test"]);
        assert!(directory(&root, &checkout, &mut false).is_ok());
        // A removed checkout is never recreated by a reopened conversation.
        let path = store.join("vix-agent-test");
        git(&repo, &["worktree", "remove", path.to_str().unwrap()]);
        assert!(directory(&root, &checkout, &mut false).is_err());
        assert!(!path.exists());
        fs::remove_dir_all(store).unwrap();
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn nothing_is_created_for_a_folder_no_commit_holds() {
        let store = temp("wt-none-store");
        let checkout = store.join("vix-agent-none");
        // An unborn HEAD: a repository without a commit.
        let unborn = temp("wt-unborn");
        git(&unborn, &["init", "-q"]);
        assert!(!available(&unborn));
        assert!(directory(&unborn, &checkout, &mut true).is_err());
        assert!(!checkout.exists());
        assert_eq!(branches(&unborn), "");
        // A folder that is not committed yet.
        let repo = repo("wt-untracked", &[("a", "a")]);
        fs::create_dir(repo.join("new")).unwrap();
        assert!(!available(&repo.join("new")));
        assert!(directory(&repo.join("new"), &checkout, &mut true).is_err());
        assert!(!checkout.exists());
        assert_eq!(branches(&repo), "");
        // Not a repository at all.
        assert!(!available(&store));
        for dir in [store, unborn, repo] {
            fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn a_checkout_inside_the_repository_a_refused_branch_and_a_planted_folder_fail() {
        let repo = repo("wt-refuse", &[("a", "a")]);
        let store = temp("wt-refuse-store");
        fs::create_dir(repo.join("inside")).unwrap();
        let inside = repo.join("inside/vix-agent-inside");
        assert!(directory(&repo, &inside, &mut true).is_err());
        assert!(!inside.exists());
        // An existing branch: Git refuses `-b`, and the claimed folder goes.
        git(&repo, &["branch", "vix-agent-dup"]);
        let dup = store.join("vix-agent-dup");
        assert!(directory(&repo, &dup, &mut true).is_err());
        assert!(!dup.exists());
        // A name Git rejects as a branch.
        let bad = store.join("vix-agent-a..b");
        assert!(directory(&repo, &bad, &mut true).is_err());
        assert!(!bad.exists());
        assert_eq!(branches(&repo).trim(), "vix-agent-dup");
        // A folder already at the path is validated, never adopted or emptied.
        let planted = store.join("vix-agent-planted");
        fs::create_dir(&planted).unwrap();
        fs::write(planted.join("keep"), "mine").unwrap();
        assert!(directory(&repo, &planted, &mut true).is_err());
        assert_eq!(fs::read_to_string(planted.join("keep")).unwrap(), "mine");
        assert_eq!(branches(&repo).trim(), "vix-agent-dup");
        fs::remove_dir_all(store).unwrap();
        fs::remove_dir_all(repo).unwrap();
    }
}
