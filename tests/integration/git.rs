#![warn(clippy::pedantic)]
#![allow(clippy::cast_possible_truncation, clippy::format_collect)]
// Shared fixtures/helpers live in `common.rs`; every test here needs a
// handful of them, so a glob import earns its keep over a long explicit list.
#![allow(clippy::wildcard_imports)]

use crate::common::*;

#[test]
#[ignore = "needs git and an in-tree checkout"]
fn git_gutter_marks_a_modified_line() {
    let mut app = app_at(Path::new("."));
    app.refresh_git();
    app.open_initial(&PathBuf::from("Cargo.toml"));
    app.on_key(key('x')); // modify the first line
    app.refresh_git_gutter();
    let marks = app
        .editor
        .active_tab()
        .unwrap()
        .editor
        .gutter_marks()
        .cloned()
        .unwrap_or_default();
    assert!(!marks.is_empty(), "a modified line is marked in the gutter");
}

#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn git_panel_stages_and_commits() {
    let dir = unique_dir("gitpanel");
    fs::create_dir_all(&dir).unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(&dir)
            .args(args)
            .output()
            .unwrap();
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "t@example.com"]);
    run(&["config", "user.name", "Test"]);
    fs::write(dir.join("a.txt"), "hello\n").unwrap();

    let mut app = app_at(&dir);
    app.run_action("git.changes");
    assert!(app.git_panel.is_some(), "panel opens in a repo");
    assert_eq!(app.git_status.len(), 1, "one changed (untracked) file");

    // Space stages the selected file.
    app.on_key(keycode(KeyCode::Char(' ')));
    assert!(app.git_status[0].is_staged(), "file is staged");

    // 'c' begins the commit message prompt.
    app.on_key(keycode(KeyCode::Char('c')));
    assert!(app.prompt.is_some(), "commit message prompt opens");
    for ch in "initial".chars() {
        app.on_key(key(ch));
    }
    app.on_key(keycode(KeyCode::Enter));

    app.refresh_git();
    assert!(app.git_status.is_empty(), "after commit the tree is clean");
    fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn git_stash_and_pop_round_trip() {
    let dir = unique_dir("gitstash");
    fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(&dir)
            .args(args)
            .output()
            .unwrap();
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "t@example.com"]);
    run(&["config", "user.name", "Test"]);
    let file = dir.join("a.txt");
    fs::write(&file, "one\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "init"]);
    fs::write(&file, "one\ntwo\n").unwrap(); // uncommitted change

    let mut app = app_at(&dir);
    app.refresh_git();
    assert!(app.git_dirty(), "working tree dirty before stash");
    app.run_action("git.stash");
    app.refresh_git();
    assert!(!app.git_dirty(), "clean after stash");
    app.run_action("git.stash_pop");
    app.refresh_git();
    assert!(app.git_dirty(), "change restored after pop");
    fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn git_blame_annotates_the_current_line() {
    let dir = unique_dir("gitblame");
    fs::create_dir_all(&dir).unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(&dir)
            .args(args)
            .output()
            .unwrap();
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "t@example.com"]);
    run(&["config", "user.name", "Ada Lovelace"]);
    let file = dir.join("a.txt");
    fs::write(&file, "one\ntwo\nthree\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "seed the file"]);

    let mut app = app_at(&dir);
    app.open_initial(&file);
    // Cursor starts on line 1; blame should attribute it to the seed commit.
    app.run_action("git.blame");
    assert!(
        app.status.contains("Ada Lovelace"),
        "blame names the author: {}",
        app.status
    );
    assert!(
        app.status.contains("seed the file"),
        "blame shows the summary: {}",
        app.status
    );
    assert!(
        app.status.starts_with("L1:"),
        "blame labels the line: {}",
        app.status
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn git_blame_flags_an_uncommitted_line() {
    let dir = unique_dir("gitblameunc");
    fs::create_dir_all(&dir).unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(&dir)
            .args(args)
            .output()
            .unwrap();
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "t@example.com"]);
    run(&["config", "user.name", "Test"]);
    let file = dir.join("a.txt");
    fs::write(&file, "committed\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "init"]);
    // A second, uncommitted line on disk.
    fs::write(&file, "committed\nbrand new\n").unwrap();

    let mut app = app_at(&dir);
    app.open_initial(&file);
    app.run_action("cursor_down"); // move to line 2 (the new line)
    app.run_action("git.blame");
    assert!(
        app.status.contains("L2") && app.status.to_lowercase().contains("commit"),
        "uncommitted line is flagged: {}",
        app.status
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn refresh_git_populates_branch_when_in_a_repo() {
    let mut app = app_at(Path::new("."));
    app.refresh_git();
    if app.git_repo {
        assert!(app.git_branch.is_some(), "a repo reports a branch");
    }
    // The dirty flag is always consistent with the cached status list.
    assert_eq!(app.git_dirty(), !app.git_status.is_empty());
}
