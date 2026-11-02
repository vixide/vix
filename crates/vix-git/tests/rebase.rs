//! T633: interactive rebase and cherry-pick against a real temporary
//! repository (no network). Fixture commands isolate themselves from the
//! developer's git configuration on the command alone (`GIT_CONFIG_GLOBAL`,
//! `GIT_CONFIG_NOSYSTEM`); the process environment is never touched. The
//! repository's own config carries the identity and turns signing off.
#![cfg(feature = "process")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use vix_git::{RebaseAction, RebaseState};

fn fixture_git(dir: &Path, args: &[&str]) -> String {
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
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

fn commit(dir: &Path, file: &str, text: &str, message: &str) {
    fs::write(dir.join(file), text).unwrap();
    fixture_git(dir, &["add", file]);
    fixture_git(dir, &["commit", "-q", "-m", message]);
}

/// A fresh repository with an `init` commit and then `one`, `two`, `three`,
/// each adding a file, under a per-test, per-process temp directory.
fn repo(name: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("vix-t633-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let dir = base.join("repo");
    fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    fixture_git(&dir, &["init", "-q", "-b", "main"]);
    for (k, v) in [
        ("user.name", "Vix Test"),
        ("user.email", "test@example.invalid"),
        ("commit.gpgsign", "false"),
        ("tag.gpgsign", "false"),
        ("core.hooksPath", "/dev/null"),
    ] {
        fixture_git(&dir, &["config", k, v]);
    }
    commit(&dir, "init.txt", "init\n", "init");
    commit(&dir, "one.txt", "1\n", "one");
    commit(&dir, "two.txt", "2\n", "two");
    commit(&dir, "three.txt", "3\n", "three");
    dir
}

fn subjects(dir: &Path) -> Vec<String> {
    fixture_git(dir, &["log", "--format=%s"])
        .lines()
        .map(str::to_string)
        .collect()
}

fn cleanup(dir: &Path) {
    if let Some(base) = dir.parent() {
        let _ = fs::remove_dir_all(base);
    }
}

#[test]
fn lists_the_commits_after_the_base_oldest_first() {
    let dir = repo("list");
    let (base, commits) = vix_git::rebase_commits(&dir, "HEAD~3").unwrap();
    assert_eq!(base.len(), 40);
    let subjects: Vec<&str> = commits.iter().map(|c| c.subject.as_str()).collect();
    assert_eq!(subjects, ["one", "two", "three"]);
    assert!(commits.iter().all(|c| c.action == RebaseAction::Pick));

    assert!(matches!(
        vix_git::rebase_commits(&dir, "--exec=x"),
        Err(vix_git::GitError::InvalidArgument(_))
    ));
    assert!(vix_git::rebase_commits(&dir, "no-such-branch").is_err());
    cleanup(&dir);
}

#[test]
fn squashing_two_commits_melds_them_into_one() {
    let dir = repo("squash");
    let (base, mut commits) = vix_git::rebase_commits(&dir, "HEAD~3").unwrap();
    commits[1].action = RebaseAction::Squash;
    let state = vix_git::rebase_interactive(&dir, &base, &commits).unwrap();
    assert_eq!(state, RebaseState::Finished);
    assert!(!vix_git::rebase_in_progress(&dir));

    assert_eq!(subjects(&dir), ["three", "one", "init"]);
    // The squash kept both messages and both files.
    let body = fixture_git(&dir, &["log", "-1", "--format=%B", "HEAD~1"]);
    assert!(body.contains("one") && body.contains("two"), "{body}");
    let files = fixture_git(&dir, &["show", "--name-only", "--format=", "HEAD~1"]);
    assert!(
        files.contains("one.txt") && files.contains("two.txt"),
        "{files}"
    );
    cleanup(&dir);
}

#[test]
fn reorder_fixup_drop_and_reword() {
    let dir = repo("reorder");
    let (base, commits) = vix_git::rebase_commits(&dir, "HEAD~3").unwrap();
    let mut todo = vix_git::RebaseTodo::new(base.clone(), "HEAD~3".into(), commits);
    // three, one, two: move `three` to the top, fix `two` into `one`.
    todo.selected = 2;
    todo.move_up();
    todo.move_up();
    todo.set_message("third, reworded\n\nwith a \"body\" and $HOME".into());
    todo.down();
    todo.down();
    todo.set_action(RebaseAction::Fixup);
    assert_eq!(vix_git::check_todo(&todo.commits), Ok(()));
    let state = vix_git::rebase_interactive(&dir, &base, &todo.commits).unwrap();
    assert_eq!(state, RebaseState::Finished);
    assert_eq!(subjects(&dir), ["one", "third, reworded", "init"]);
    let body = fixture_git(&dir, &["log", "-1", "--format=%B", "HEAD~1"]);
    assert_eq!(body, "third, reworded\n\nwith a \"body\" and $HOME");

    // Drop the top commit.
    let (base, mut commits) = vix_git::rebase_commits(&dir, "HEAD~1").unwrap();
    commits[0].action = RebaseAction::Drop;
    vix_git::rebase_interactive(&dir, &base, &commits).unwrap();
    assert_eq!(subjects(&dir), ["third, reworded", "init"]);
    cleanup(&dir);
}

#[test]
fn edit_stops_then_continue_and_abort() {
    let dir = repo("edit");
    let (base, mut commits) = vix_git::rebase_commits(&dir, "HEAD~2").unwrap();
    commits[0].action = RebaseAction::Edit;
    let state = vix_git::rebase_interactive(&dir, &base, &commits).unwrap();
    assert_eq!(state, RebaseState::Stopped);
    assert!(vix_git::rebase_in_progress(&dir));
    // Amend the stopped commit, then continue.
    fs::write(dir.join("two.txt"), "2 amended\n").unwrap();
    fixture_git(&dir, &["add", "two.txt"]);
    let state = vix_git::rebase_continue(&dir).unwrap();
    assert_eq!(state, RebaseState::Finished);
    assert_eq!(subjects(&dir), ["three", "two", "one", "init"]);
    assert_eq!(
        fs::read_to_string(dir.join("two.txt")).unwrap(),
        "2 amended\n"
    );

    // Abort puts everything back.
    let head = fixture_git(&dir, &["rev-parse", "HEAD"]);
    let (base, mut commits) = vix_git::rebase_commits(&dir, "HEAD~2").unwrap();
    commits[0].action = RebaseAction::Edit;
    commits[1].action = RebaseAction::Squash;
    assert_eq!(
        vix_git::rebase_interactive(&dir, &base, &commits).unwrap(),
        RebaseState::Stopped
    );
    vix_git::rebase_abort(&dir).unwrap();
    assert!(!vix_git::rebase_in_progress(&dir));
    assert_eq!(fixture_git(&dir, &["rev-parse", "HEAD"]), head);
    assert!(
        vix_git::rebase_abort(&dir).is_err(),
        "nothing left to abort"
    );
    cleanup(&dir);
}

#[test]
fn cherry_pick_applies_a_commit_from_another_branch() {
    let dir = repo("pick");
    fixture_git(&dir, &["switch", "-q", "-c", "side", "HEAD~2"]);
    commit(&dir, "side.txt", "s\n", "side work");
    let side = fixture_git(&dir, &["rev-parse", "HEAD"]);
    fixture_git(&dir, &["switch", "-q", "main"]);
    let offered = vix_git::cherry_pick_candidates(&dir, "side", 50).unwrap();
    assert_eq!(offered.len(), 1, "only the commit main lacks");
    assert_eq!(offered[0].sha, side);
    assert_eq!(
        vix_git::cherry_pick_candidates(&dir, "main", 50).unwrap(),
        Vec::new()
    );
    assert!(vix_git::cherry_pick_candidates(&dir, "-x", 50).is_err());
    vix_git::cherry_pick(&dir, &side).unwrap();
    assert_eq!(subjects(&dir)[0], "side work");
    assert!(dir.join("side.txt").is_file());
    assert!(matches!(
        vix_git::cherry_pick(&dir, "-n"),
        Err(vix_git::GitError::InvalidArgument(_))
    ));
    cleanup(&dir);
}
