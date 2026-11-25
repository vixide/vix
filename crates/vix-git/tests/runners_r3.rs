//! Round-3 tests: the plain runners (`stage`, `commit`, `checkout`, logs,
//! blobs, blame, patches) against a throwaway local repository, and their
//! failure paths in a directory that is not a repository. No network;
//! fixture commands isolate themselves from the developer's git config.
#![cfg(feature = "process")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use vix_git::{ApplyTarget, Change, GitError, LogPanel, LogScope};

fn fixture_git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
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

fn repo(name: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("vix-r3-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let dir = base.join("repo");
    fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    fixture_git(&dir, &["init", "-q", "-b", "main"]);
    for (k, v) in [
        ("user.name", "Vix Test"),
        ("user.email", "test@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", "/dev/null"),
        ("core.autocrlf", "false"),
    ] {
        fixture_git(&dir, &["config", k, v]);
    }
    fs::write(dir.join("a.txt"), "one\ntwo\nthree\n").unwrap();
    fixture_git(&dir, &["add", "a.txt"]);
    fixture_git(&dir, &["commit", "-q", "-m", "first commit"]);
    dir
}

fn plain_dir(name: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("vix-r3-plain-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    base.canonicalize().unwrap()
}

fn cleanup(dir: &Path) {
    if let Some(base) = dir.parent() {
        let _ = fs::remove_dir_all(base);
    }
}

#[test]
fn git_error_text_source_and_conversion() {
    assert_eq!(
        GitError::NotInstalled.to_string(),
        "git is not installed (or not on PATH)"
    );
    assert_eq!(GitError::InvalidArgument("nope".into()).to_string(), "nope");
    let failed = GitError::Failed {
        code: Some(128),
        stderr: "fatal: x".into(),
    };
    assert_eq!(failed.to_string(), "fatal: x");
    let no_text = GitError::Failed {
        code: Some(3),
        stderr: String::new(),
    };
    assert_eq!(no_text.to_string(), "git exited with status 3");
    let killed = GitError::Failed {
        code: None,
        stderr: String::new(),
    };
    assert_eq!(killed.to_string(), "git was terminated");
    let io = GitError::Io(std::io::Error::other("pipe"));
    assert_eq!(io.to_string(), "pipe");
    assert!(std::error::Error::source(&io).is_some());
    assert!(std::error::Error::source(&failed).is_none());
    assert!(matches!(
        GitError::from(std::io::Error::from(std::io::ErrorKind::NotFound)),
        GitError::NotInstalled
    ));
    assert!(matches!(
        GitError::from(std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
        GitError::Io(_)
    ));
}

#[test]
fn run_returns_trimmed_stdout_or_the_failure() {
    let dir = repo("run");
    assert_eq!(
        vix_git::run(&dir, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap(),
        "main"
    );
    match vix_git::run(&dir, &["no-such-subcommand"]) {
        Err(GitError::Failed { code, stderr }) => {
            assert!(code.is_some_and(|c| c != 0), "git exits non-zero");
            assert_ne!(stderr, String::new());
        }
        other => panic!("expected Failed, got {other:?}"),
    }
    assert!(matches!(
        vix_git::run(&dir.join("does-not-exist"), &["status"]),
        Err(GitError::Io(_) | GitError::NotInstalled)
    ));
    cleanup(&dir);
}

#[test]
fn repo_facts_status_branches_and_commits() {
    let dir = repo("facts");
    assert!(vix_git::is_repo(&dir));
    assert_eq!(vix_git::branch(&dir).as_deref(), Some("main"));
    assert!(!vix_git::is_dirty(&dir));
    assert_eq!(vix_git::commit_count(&dir), Some(1));
    assert_eq!(vix_git::local_branches(&dir), vec!["main".to_string()]);

    fs::write(dir.join("a.txt"), "one\nTWO\nthree\n").unwrap();
    fs::write(dir.join("new.txt"), "n\n").unwrap();
    assert!(vix_git::is_dirty(&dir));
    let status = vix_git::status(&dir);
    let a = status.iter().find(|s| s.path == "a.txt").unwrap();
    assert_eq!(a.primary(), Some(Change::Modified));
    assert!(!a.is_staged());
    let n = status.iter().find(|s| s.path == "new.txt").unwrap();
    assert_eq!(n.primary(), Some(Change::Untracked));

    // Detached HEAD yields the short hash.
    let sha = vix_git::run(&dir, &["rev-parse", "HEAD"]).unwrap();
    fixture_git(&dir, &["checkout", "-q", "--detach"]);
    assert_eq!(
        vix_git::branch(&dir).as_deref(),
        vix_git::resolve_short_sha(&dir, &sha).as_deref()
    );
    cleanup(&dir);
}

#[test]
fn staging_cycle_blobs_and_staged_diff() {
    let dir = repo("stage");
    assert_eq!(vix_git::staged_diff(&dir), None, "nothing staged");
    assert_eq!(
        vix_git::head_blob(&dir, "a.txt").as_deref(),
        Some("one\ntwo\nthree\n")
    );
    assert_eq!(vix_git::head_blob(&dir, "missing.txt"), None);

    fs::write(dir.join("a.txt"), "one\nTWO\nthree\n").unwrap();
    vix_git::stage(&dir, "a.txt").unwrap();
    assert_eq!(
        vix_git::index_blob(&dir, "a.txt").as_deref(),
        Some("one\nTWO\nthree\n")
    );
    assert!(vix_git::staged_diff(&dir).unwrap().contains("+TWO"));
    vix_git::unstage(&dir, "a.txt").unwrap();
    assert_eq!(
        vix_git::index_blob(&dir, "a.txt").as_deref(),
        Some("one\ntwo\nthree\n")
    );

    // stage_content stages exactly the given text, working tree untouched.
    vix_git::stage_content(&dir, "a.txt", "one\ntwo\nthree\nfour\n").unwrap();
    assert_eq!(
        vix_git::index_blob(&dir, "a.txt").as_deref(),
        Some("one\ntwo\nthree\nfour\n")
    );
    assert_eq!(
        fs::read_to_string(dir.join("a.txt")).unwrap(),
        "one\nTWO\nthree\n"
    );
    // ... also for a brand-new path (default mode).
    vix_git::stage_content(&dir, "fresh.txt", "x\n").unwrap();
    assert_eq!(
        vix_git::index_blob(&dir, "fresh.txt").as_deref(),
        Some("x\n")
    );

    // discard restores the working tree from the index.
    vix_git::discard(&dir, "a.txt").unwrap();
    assert_eq!(
        fs::read_to_string(dir.join("a.txt")).unwrap(),
        "one\ntwo\nthree\nfour\n"
    );
    assert!(vix_git::discard(&dir, "untracked-never-added.txt").is_err());

    fs::write(dir.join("b.txt"), "b\n").unwrap();
    vix_git::stage_all(&dir).unwrap();
    assert!(
        vix_git::status(&dir)
            .iter()
            .all(vix_git::FileStatus::is_staged)
    );
    cleanup(&dir);
}

#[test]
fn apply_patch_stages_or_reverts_and_reports_misfits() {
    let dir = repo("patch");
    let patch = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,3 +1,3 @@\n one\n-two\n+TWO\n three\n";
    vix_git::apply_patch(&dir, patch, ApplyTarget::Index).unwrap();
    assert!(vix_git::staged_diff(&dir).unwrap().contains("+TWO"));
    assert_eq!(
        fs::read_to_string(dir.join("a.txt")).unwrap(),
        "one\ntwo\nthree\n"
    );
    // A patch that no longer fits is refused with git's own message.
    assert!(matches!(
        vix_git::apply_patch(&dir, patch, ApplyTarget::Index),
        Err(GitError::Failed { .. })
    ));
    // Reverting a patch against the working tree.
    fs::write(dir.join("a.txt"), "one\nTWO\nthree\n").unwrap();
    vix_git::apply_patch(&dir, patch, ApplyTarget::RevertWorktree).unwrap();
    assert_eq!(
        fs::read_to_string(dir.join("a.txt")).unwrap(),
        "one\ntwo\nthree\n"
    );
    cleanup(&dir);
}

#[test]
fn branches_checkout_commit_amend_and_log() {
    let dir = repo("branches");
    assert!(matches!(
        vix_git::checkout(&dir, "-x"),
        Err(GitError::InvalidArgument(_))
    ));
    assert!(matches!(
        vix_git::checkout(&dir, ""),
        Err(GitError::InvalidArgument(_))
    ));
    assert!(matches!(
        vix_git::checkout(&dir, "a b"),
        Err(GitError::InvalidArgument(_))
    ));
    assert!(matches!(
        vix_git::create_branch(&dir, "--x"),
        Err(GitError::InvalidArgument(_))
    ));
    assert!(matches!(
        vix_git::checkout(&dir, "ghost"),
        Err(GitError::Failed { .. })
    ));
    vix_git::create_branch(&dir, "feature").unwrap();
    assert_eq!(vix_git::branch(&dir).as_deref(), Some("feature"));
    assert!(matches!(
        vix_git::create_branch(&dir, "feature"),
        Err(GitError::Failed { .. })
    ));
    vix_git::checkout(&dir, "main").unwrap();
    // Current branch comes first.
    assert_eq!(
        vix_git::local_branches(&dir),
        vec!["main".to_string(), "feature".to_string()]
    );
    vix_git::checkout(&dir, "feature").unwrap();
    assert_eq!(vix_git::local_branches(&dir)[0], "feature");

    // commit: nothing staged fails; staged succeeds; amend folds more in.
    assert!(vix_git::commit(&dir, "empty").is_err());
    fs::write(dir.join("c.txt"), "c\n").unwrap();
    vix_git::stage(&dir, "c.txt").unwrap();
    vix_git::commit(&dir, "add c").unwrap();
    assert_eq!(vix_git::commit_count(&dir), Some(2));
    fs::write(dir.join("d.txt"), "d\n").unwrap();
    vix_git::stage(&dir, "d.txt").unwrap();
    vix_git::commit_amend(&dir).unwrap();
    assert_eq!(vix_git::commit_count(&dir), Some(2));

    // log / file_log / show_commit / review_diff / show_file_at / rev_blob
    let log = vix_git::log(&dir, 10);
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].subject, "add c");
    assert_eq!(log[1].subject, "first commit");
    assert_eq!(log[0].author, "Vix Test");
    assert_eq!(log[0].date.len(), 10);
    assert!(log[0].sha.starts_with(&log[0].abbrev));
    assert_eq!(vix_git::log(&dir, 1).len(), 1);
    let file_log = vix_git::file_log(&dir, "a.txt", 10);
    assert_eq!(file_log.len(), 1);
    assert_eq!(file_log[0].subject, "first commit");

    let whole = vix_git::show_commit(&dir, &log[0].sha, None).unwrap();
    assert!(whole.contains("c.txt") && whole.contains("d.txt"));
    let only = vix_git::show_commit(&dir, &log[0].sha, Some("c.txt")).unwrap();
    assert!(only.contains("c.txt") && !only.contains("d.txt"));
    assert_eq!(vix_git::show_commit(&dir, "-bad", None), None);
    assert_eq!(vix_git::show_commit(&dir, "deadbeefdeadbeef", None), None);

    let range = format!("{}..{}", log[1].sha, log[0].sha);
    let diff = vix_git::review_diff(&dir, &range).unwrap();
    assert!(diff.contains("+++ b/c.txt"));
    let single = vix_git::review_diff(&dir, &log[0].sha).unwrap();
    assert!(single.contains("+++ b/c.txt"));
    assert_eq!(vix_git::review_diff(&dir, "-x"), None);

    assert_eq!(
        vix_git::show_file_at(&dir, &log[1].sha, "a.txt").as_deref(),
        Some("one\ntwo\nthree\n")
    );
    assert_eq!(vix_git::show_file_at(&dir, &log[1].sha, "c.txt"), None);
    assert_eq!(vix_git::show_file_at(&dir, "-x", "a.txt"), None);
    assert_eq!(
        vix_git::rev_blob(&dir, &log[1].sha, "a.txt").as_deref(),
        Some("one\ntwo\nthree\n")
    );
    assert_eq!(vix_git::rev_blob(&dir, "-x", "a.txt"), None);
    assert_eq!(vix_git::resolve_short_sha(&dir, "-x"), None);
    assert_eq!(vix_git::resolve_short_sha(&dir, "nonesuch"), None);
    assert_eq!(
        vix_git::resolve_short_sha(&dir, &log[0].sha).as_deref(),
        Some(log[0].abbrev.as_str())
    );
    cleanup(&dir);
}

#[test]
fn blame_names_the_author_and_flags_unsaved_edits() {
    let dir = repo("blame");
    let b = vix_git::blame_line(&dir, "a.txt", 2, None).unwrap();
    assert_eq!(b.author, "Vix Test");
    assert!(!b.is_uncommitted());
    // With unsaved buffer contents the edited line is uncommitted.
    let edited = vix_git::blame_line(&dir, "a.txt", 2, Some("one\nEDITED\nthree\n")).unwrap();
    assert!(edited.is_uncommitted());
    let same = vix_git::blame_line(&dir, "a.txt", 1, Some("one\ntwo\nthree\n")).unwrap();
    assert!(!same.is_uncommitted());
    assert!(vix_git::blame_line(&dir, "nope.txt", 1, None).is_none());
    assert!(vix_git::blame_line(&dir, "nope.txt", 1, Some("x")).is_none());
    cleanup(&dir);
}

#[test]
fn paths_resolve_from_a_subdirectory_of_the_workspace() {
    let dir = repo("subdir");
    fs::create_dir_all(dir.join("sub")).unwrap();
    fs::write(dir.join("sub/s.txt"), "s\n").unwrap();
    fixture_git(&dir, &["add", "sub/s.txt"]);
    fixture_git(&dir, &["commit", "-q", "-m", "sub"]);
    let sub = dir.join("sub");
    assert_eq!(vix_git::head_blob(&sub, "s.txt").as_deref(), Some("s\n"));
    fs::write(sub.join("s.txt"), "s2\n").unwrap();
    let status = vix_git::status(&sub);
    assert_eq!(status.len(), 1);
    assert_eq!(
        status[0].path, "s.txt",
        "paths are relative to the workspace dir"
    );
    vix_git::stage(&sub, "s.txt").unwrap();
    assert_eq!(vix_git::index_blob(&sub, "s.txt").as_deref(), Some("s2\n"));
    // A sibling reached with `..` maps through the prefix.
    assert_eq!(
        vix_git::head_blob(&sub, "../a.txt").as_deref(),
        Some("one\ntwo\nthree\n")
    );
    // Escaping the repository entirely cannot be mapped: git reports it.
    assert_eq!(vix_git::head_blob(&sub, "../../../x"), None);
    cleanup(&dir);
}

#[test]
fn every_runner_fails_quietly_outside_a_repository() {
    let dir = plain_dir("norepo");
    assert!(!vix_git::is_repo(&dir));
    assert_eq!(vix_git::branch(&dir), None);
    assert!(!vix_git::is_dirty(&dir));
    assert_eq!(vix_git::status(&dir), Vec::<vix_git::FileStatus>::new());
    assert_eq!(vix_git::head_blob(&dir, "a"), None);
    assert_eq!(vix_git::index_blob(&dir, "a"), None);
    assert_eq!(vix_git::staged_diff(&dir), None);
    assert_eq!(vix_git::commit_count(&dir), None);
    assert_eq!(vix_git::local_branches(&dir), Vec::<String>::new());
    assert_eq!(vix_git::log(&dir, 5), Vec::<vix_git::LogEntry>::new());
    assert_eq!(
        vix_git::file_log(&dir, "a", 5),
        Vec::<vix_git::LogEntry>::new()
    );
    assert!(!vix_git::commit_signing_needs_tty(&dir));
    for r in [
        vix_git::stage_all(&dir),
        vix_git::stage(&dir, "a"),
        vix_git::unstage(&dir, "a"),
        vix_git::discard(&dir, "a"),
        vix_git::stage_content(&dir, "a", "x"),
        vix_git::checkout(&dir, "main"),
        vix_git::create_branch(&dir, "x"),
        vix_git::commit(&dir, "m"),
        vix_git::commit_amend(&dir),
        vix_git::stash_push(&dir),
        vix_git::stash_pop(&dir),
        vix_git::apply_patch(&dir, "junk", ApplyTarget::Index),
    ] {
        assert!(matches!(r, Err(GitError::Failed { .. })), "{r:?}");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn stash_push_and_pop_round_trip() {
    let dir = repo("stashpop");
    fs::write(dir.join("a.txt"), "changed\n").unwrap();
    vix_git::stash_push(&dir).unwrap();
    assert_eq!(
        fs::read_to_string(dir.join("a.txt")).unwrap(),
        "one\ntwo\nthree\n"
    );
    vix_git::stash_pop(&dir).unwrap();
    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "changed\n");
    assert!(vix_git::stash_pop(&dir).is_err(), "no stash left");
    cleanup(&dir);
}

#[test]
fn signing_needs_tty_depends_on_config() {
    let dir = repo("sign");
    assert!(!vix_git::commit_signing_needs_tty(&dir), "gpgsign is off");
    fixture_git(&dir, &["config", "commit.gpgsign", "true"]);
    fixture_git(&dir, &["config", "gpg.format", "ssh"]);
    assert!(
        !vix_git::commit_signing_needs_tty(&dir),
        "ssh signing never needs a tty pinentry"
    );
    fixture_git(&dir, &["config", "--unset", "gpg.format"]);
    // GPG signing: the answer depends on the machine's gpg-agent.conf, which
    // the pure parser below covers; here we only require it not to panic.
    let _ = vix_git::commit_signing_needs_tty(&dir);
    cleanup(&dir);
}

#[test]
fn pinentry_parser_takes_the_last_uncommented_program() {
    assert!(vix_git::pinentry_wants_tty(
        "pinentry-program /usr/bin/pinentry-curses"
    ));
    assert!(vix_git::pinentry_wants_tty(
        "# c\npinentry-program C:\\gpg\\pinentry-tty.exe"
    ));
    assert!(!vix_git::pinentry_wants_tty(
        "pinentry-program /usr/bin/pinentry-mac"
    ));
    assert!(!vix_git::pinentry_wants_tty(
        "pinentry-program /usr/bin/pinentry-curses\npinentry-program /usr/bin/pinentry-gtk"
    ));
    assert!(!vix_git::pinentry_wants_tty(
        "# pinentry-program /x/pinentry-curses"
    ));
    assert!(!vix_git::pinentry_wants_tty(
        "pinentry-programx /x/pinentry-curses"
    ));
    assert!(!vix_git::pinentry_wants_tty(""));
}

#[test]
fn log_panel_navigation_and_selection() {
    let entry = |n: usize| vix_git::LogEntry {
        sha: format!("sha{n}"),
        abbrev: format!("s{n}"),
        author: "a".into(),
        date: "2026-01-01".into(),
        subject: format!("subject {n}"),
    };
    let mut panel = LogPanel::new((0..5).map(entry).collect(), LogScope::File("a.txt".into()));
    assert_eq!(panel.selected_sha(), Some("sha0"));
    panel.up();
    assert_eq!(panel.selected, 0, "stops at the top");
    panel.down();
    panel.down();
    assert_eq!(panel.selected_abbrev(), Some("s2"));
    panel.page_down(10);
    assert_eq!(panel.selected, 4, "stops at the bottom");
    panel.page_up(2);
    assert_eq!(panel.selected, 2);
    panel.selected = 4;
    panel.ensure_visible(2);
    assert_eq!(panel.scroll, 3);
    assert_eq!(panel.scope, LogScope::File("a.txt".into()));
    let empty = LogPanel::new(Vec::new(), LogScope::Repo);
    assert_eq!(empty.selected_sha(), None);
    assert_eq!(empty.selected_abbrev(), None);
}

#[test]
fn change_letters_and_status_parsing_edges() {
    use vix_git::parse_status;
    assert_eq!(
        [
            Change::Added,
            Change::Modified,
            Change::Deleted,
            Change::Renamed,
            Change::Untracked,
            Change::Conflicted
        ]
        .map(Change::letter),
        ['A', 'M', 'D', 'R', '?', 'U']
    );
    let rows = parse_status(
        "A  new.rs\nC  copy.rs\n D gone.rs\nR  old.rs -> moved.rs\nUU both.rs\n?? loose.rs\nx\nZZ weird.rs\n M \n",
    );
    let by = |p: &str| rows.iter().find(|r| r.path == p).unwrap();
    assert_eq!(by("new.rs").staged, Some(Change::Added));
    assert_eq!(
        by("copy.rs").staged,
        Some(Change::Modified),
        "copied counts as modified"
    );
    assert_eq!(by("gone.rs").unstaged, Some(Change::Deleted));
    assert_eq!(
        by("moved.rs").staged,
        Some(Change::Renamed),
        "the destination is recorded"
    );
    assert_eq!(by("both.rs").primary(), Some(Change::Conflicted));
    assert_eq!(by("loose.rs").unstaged, Some(Change::Untracked));
    assert_eq!(by("weird.rs").primary(), None);
    assert!(!rows.iter().any(|r| r.path.is_empty()));
}

#[test]
fn rebase_todo_overlay_reorders_and_rewords() {
    use vix_git::{RebaseAction, RebaseTodo, TodoCommit, TodoItem, serialize_todo};
    let commits: Vec<TodoCommit> = ["aaaaaaaaaa", "bbbbbbbbbb", "cc"]
        .iter()
        .map(|s| TodoCommit::pick(*s, format!("subject {s}")))
        .collect();
    assert_eq!(commits[0].abbrev(), "aaaaaaa");
    assert_eq!(commits[2].abbrev(), "cc", "a short hash is shown whole");
    let mut todo = RebaseTodo::new("base".into(), "HEAD~3".into(), commits);
    assert_eq!(todo.selected_commit().unwrap().sha, "aaaaaaaaaa");
    todo.up();
    assert_eq!(todo.selected, 0);
    todo.move_up();
    assert_eq!(
        todo.commits[0].sha, "aaaaaaaaaa",
        "the top row cannot move up"
    );
    todo.move_down();
    assert_eq!(
        (todo.selected, todo.commits[1].sha.as_str()),
        (1, "aaaaaaaaaa")
    );
    todo.down();
    todo.down();
    assert_eq!(todo.selected, 2, "stops at the last row");
    todo.move_down();
    assert_eq!(todo.selected, 2, "the bottom row cannot move down");
    todo.set_message("new words".into());
    let c = todo.selected_commit().unwrap();
    assert_eq!(
        (c.action, c.message.as_deref()),
        (RebaseAction::Reword, Some("new words"))
    );
    todo.set_action(RebaseAction::Squash);
    let c = todo.selected_commit().unwrap();
    assert_eq!(
        (c.action, c.message.as_deref()),
        (RebaseAction::Squash, None)
    );
    todo.set_action(RebaseAction::Reword);
    todo.ensure_visible(1);
    assert_eq!(todo.scroll, 2);
    let mut none = RebaseTodo::new("b".into(), "b".into(), Vec::new());
    none.set_action(RebaseAction::Drop);
    none.set_message("x".into());
    none.move_up();
    none.move_down();
    assert!(none.selected_commit().is_none());
    // A commit without a subject serializes as just action and hash.
    let line = serialize_todo(&[
        TodoItem::Commit(TodoCommit::pick("abc", "")),
        TodoItem::Other("# note".into()),
    ]);
    assert!(line.starts_with("pick abc"), "{line:?}");
    assert!(
        !line.starts_with("pick abc "),
        "no trailing space: {line:?}"
    );
}

#[test]
fn worktree_branch_accessor_follows_the_checkout_kind() {
    use vix_git::parse_worktree_list;
    let list = parse_worktree_list(
        "worktree /a\nHEAD abc\nbranch refs/heads/main\n\nworktree /b\nHEAD def\ndetached\n\nworktree /c\nbare\n",
    );
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].branch(), Some("main"));
    assert_eq!(list[1].branch(), None);
    assert_eq!(list[2].branch(), None);
}
