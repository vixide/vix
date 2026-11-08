//! T634: the stash and worktree runners against a real temporary repository
//! (no network). Fixture commands isolate themselves from the developer's
//! git configuration on the command alone (`GIT_CONFIG_GLOBAL`,
//! `GIT_CONFIG_NOSYSTEM`); the process environment is never touched. The
//! repository's own config carries the identity and turns signing off, so
//! the runners under test (which read the normal configuration) need no
//! global setup either.
#![cfg(feature = "process")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture_git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A fresh repository with one commit of `a.txt`, under a per-test,
/// per-process temp directory.
fn repo(name: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("vix-t634-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let dir = base.join("repo");
    fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    fixture_git(&dir, &["init", "-q"]);
    for (k, v) in [
        ("user.name", "Vix Test"),
        ("user.email", "test@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", "/dev/null"),
        // Windows runners set core.autocrlf=true globally; keep LF (T772).
        ("core.autocrlf", "false"),
    ] {
        fixture_git(&dir, &["config", k, v]);
    }
    fs::write(dir.join("a.txt"), "one\n").unwrap();
    fixture_git(&dir, &["add", "a.txt"]);
    fixture_git(&dir, &["commit", "-q", "-m", "init"]);
    dir
}

fn cleanup(dir: &Path) {
    if let Some(base) = dir.parent() {
        let _ = fs::remove_dir_all(base);
    }
}

#[test]
fn stash_push_list_show_apply_pop_and_drop() {
    let dir = repo("stash");
    let file = dir.join("a.txt");

    fs::write(&file, "one\ntwo\n").unwrap();
    vix_git::stash_push(&dir).unwrap();
    fs::write(&file, "one\nthree\n").unwrap();
    vix_git::stash_push(&dir).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), "one\n");

    let list = vix_git::stash_list(&dir);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].index, 0);
    assert_eq!(list[1].index, 1);

    // Show the older stash: its diff adds "two".
    let patch = vix_git::stash_show(&dir, 1).expect("stash show");
    assert!(patch.contains("+two"), "{patch}");

    // Apply keeps the entry.
    vix_git::stash_apply(&dir, 1).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), "one\ntwo\n");
    assert_eq!(vix_git::stash_list(&dir).len(), 2);
    fixture_git(&dir, &["checkout", "--", "a.txt"]);

    // Drop removes the newest without touching the tree.
    vix_git::stash_drop(&dir, 0).unwrap();
    let list = vix_git::stash_list(&dir);
    assert_eq!(list.len(), 1);
    assert_eq!(fs::read_to_string(&file).unwrap(), "one\n");

    // Pop applies and removes the last one.
    vix_git::stash_pop_at(&dir, 0).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), "one\ntwo\n");
    assert_eq!(vix_git::stash_list(&dir), Vec::new());

    // A missing stash is git's error, not a panic.
    assert!(vix_git::stash_drop(&dir, 5).is_err());
    assert!(vix_git::stash_show(&dir, 5).is_none());
    cleanup(&dir);
}

#[test]
fn worktree_add_list_and_remove() {
    let dir = repo("worktree");
    let list = vix_git::worktree_list(&dir);
    assert_eq!(list.len(), 1, "only the main worktree");
    assert_eq!(list[0].path.canonicalize().unwrap(), dir);

    // A path relative to the repository, on a new branch named after it.
    vix_git::worktree_add(&dir, "../wt-feature").unwrap();
    let wt = dir.parent().unwrap().join("wt-feature");
    assert!(wt.join("a.txt").is_file());
    let list = vix_git::worktree_list(&dir);
    assert_eq!(list.len(), 2);
    assert_eq!(
        list[1].path.canonicalize().unwrap(),
        wt.canonicalize().unwrap()
    );
    assert_eq!(list[1].branch(), Some("wt-feature"));

    // Option-shaped paths never reach git.
    assert!(matches!(
        vix_git::worktree_add(&dir, "--force"),
        Err(vix_git::GitError::InvalidArgument(_))
    ));
    assert!(vix_git::worktree_remove(&dir, "-f").is_err());

    // A dirty worktree is refused (nothing here forces it).
    fs::write(wt.join("a.txt"), "dirty\n").unwrap();
    let wt_path = list[1].path.to_string_lossy().into_owned();
    assert!(vix_git::worktree_remove(&dir, &wt_path).is_err());
    assert!(wt.exists());

    fixture_git(&wt, &["checkout", "--", "a.txt"]);
    vix_git::worktree_remove(&dir, &wt_path).unwrap();
    assert!(!wt.exists());
    assert_eq!(vix_git::worktree_list(&dir).len(), 1);
    cleanup(&dir);
}
