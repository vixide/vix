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
#[ignore = "needs git and an in-tree checkout"]
fn git_gutter_refresh_skips_recompute_until_the_buffer_revision_changes() {
    // T510: `refresh_git_gutter` is called every redraw, so it must not
    // recompute the diff (and thus repopulate the gutter) when neither the
    // active path nor the buffer's edit revision changed since the last call.
    let mut app = app_at(Path::new("."));
    app.refresh_git();
    app.open_initial(&PathBuf::from("Cargo.toml"));
    app.on_key(key('x')); // modify the first line
    app.refresh_git_gutter();
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .editor
            .gutter_marks()
            .is_some_and(|m| !m.is_empty()),
        "the edit is marked in the gutter"
    );

    // Clear the marks by hand, then refresh again with no intervening edit:
    // a cache hit must leave them cleared rather than recomputing and
    // repopulating them.
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .clear_gutter_marks();
    app.refresh_git_gutter();
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .editor
            .gutter_marks()
            .is_none(),
        "no new edit since the last refresh, so the cache hit must not repopulate the marks"
    );

    // A further edit bumps the revision, so the next refresh must recompute.
    app.on_key(key('y'));
    app.refresh_git_gutter();
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .editor
            .gutter_marks()
            .is_some_and(|m| !m.is_empty()),
        "a new edit invalidates the cache, so the gutter is recomputed"
    );
}

#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn git_panel_stages_and_commits() {
    let dir = unique_dir("gitpanel");
    fs::create_dir_all(&dir).unwrap();
    init_git_repo(&dir, "Test");
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

/// T125: the Git panel's `g` key generates a commit message from the staged
/// diff and opens the commit prompt pre-filled with it -- it only fills the
/// message box, it never commits on its own.
#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn git_panel_generates_a_commit_message_from_the_staged_diff() {
    let dir = unique_dir("gitpanel-ai-commit");
    fs::create_dir_all(&dir).unwrap();
    init_git_repo(&dir, "Test");
    fs::write(dir.join("a.txt"), "hello\n").unwrap();

    // Deterministic `ai_command`: ignores its input, always prints the same
    // message -- no real assistant needed to exercise the wiring.
    let settings = Settings {
        ai_command: "printf '%s' 'Add a.txt with a greeting'".to_string(),
        ..Settings::default()
    };
    let mut app = app_at_with(&dir, settings);

    app.run_action("git.changes");
    assert!(app.git_panel.is_some(), "panel opens in a repo");
    app.on_key(keycode(KeyCode::Char(' '))); // stage the file
    assert!(app.git_status[0].is_staged(), "file is staged");

    app.on_key(keycode(KeyCode::Char('g'))); // generate a commit message
    wait_for_ai_replace(&mut app, |app| app.prompt.is_some());
    let prompt = app
        .prompt
        .as_ref()
        .expect("commit prompt opened with the generated message");
    assert!(matches!(prompt.kind, vix::app::PromptKind::GitCommit));
    assert_eq!(prompt.input, "Add a.txt with a greeting");

    // Generating a message never commits on its own -- the user still has to
    // confirm.
    app.refresh_git();
    assert!(
        !app.git_status.is_empty(),
        "nothing was committed yet, only the prompt was filled"
    );

    app.on_key(keycode(KeyCode::Enter)); // accept the generated message
    app.refresh_git();
    assert!(
        app.git_status.is_empty(),
        "after confirming the generated message, the tree is clean"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn git_stash_and_pop_round_trip() {
    let dir = unique_dir("gitstash");
    fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    init_git_repo(&dir, "Test");
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(&dir)
            .args(args)
            .output()
            .unwrap();
    };
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
    init_git_repo(&dir, "Ada Lovelace");
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(&dir)
            .args(args)
            .output()
            .unwrap();
    };
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
    init_git_repo(&dir, "Test");
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(&dir)
            .args(args)
            .output()
            .unwrap();
    };
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
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn git_browse_log_lists_commits_and_enter_opens_the_diff_tab() {
    let dir = unique_dir("gitlog");
    fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    init_git_repo(&dir, "Test");
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(&dir)
            .args(args)
            .output()
            .unwrap();
    };
    fs::write(dir.join("a.txt"), "one\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "first commit"]);
    fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
    run(&["commit", "-qa", "-m", "second commit"]);

    let mut app = app_at(&dir);
    let tabs_before = app.editor.tabs.len();
    app.run_action("git.browse_log");
    let log = app.git_log.as_ref().expect("log panel opens");
    assert_eq!(log.entries.len(), 2, "both commits are listed");
    assert_eq!(log.entries[0].subject, "second commit", "most recent first");

    app.on_key(keycode(KeyCode::Enter));

    assert!(app.git_log.is_none(), "Enter closes the log panel");
    assert_eq!(
        app.editor.tabs.len(),
        tabs_before + 1,
        "the diff opens in a new tab"
    );
    let tab = app.editor.active_tab().unwrap();
    assert!(tab.read_only, "the diff tab is read-only");
    assert!(
        tab.title().contains("log"),
        "titled with the whole-repo scope: {}",
        tab.title()
    );
    assert!(
        tab.text().contains("+two"),
        "the patch shows the added line: {:?}",
        tab.text()
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn git_file_history_lists_only_commits_touching_the_active_file() {
    let dir = unique_dir("gitfilehistory");
    fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    init_git_repo(&dir, "Test");
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(&dir)
            .args(args)
            .output()
            .unwrap();
    };
    fs::write(dir.join("a.txt"), "a1\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "add a"]);
    fs::write(dir.join("b.txt"), "b1\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "add b, unrelated to a"]);
    fs::write(dir.join("a.txt"), "a1\na2\n").unwrap();
    run(&["commit", "-qa", "-m", "edit a"]);

    let mut app = app_at(&dir);
    app.open_initial(&dir.join("a.txt"));
    app.run_action("git.file_history");

    let log = app.git_log.as_ref().expect("file history opens");
    assert_eq!(
        log.entries.len(),
        2,
        "only a.txt's own two commits, not the unrelated b.txt one: {:?}",
        log.entries.iter().map(|e| &e.subject).collect::<Vec<_>>()
    );
    assert!(matches!(&log.scope, vix::git::LogScope::File(p) if p == "a.txt"));

    app.on_key(keycode(KeyCode::Enter));
    let tab = app.editor.active_tab().unwrap();
    assert!(
        tab.title().contains("a.txt"),
        "titled with the file scope: {}",
        tab.title()
    );
    assert!(tab.text().contains("+a2"), "{:?}", tab.text());

    fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn git_open_at_revision_shows_the_files_old_content_read_only() {
    let dir = unique_dir("gitrevision");
    fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    init_git_repo(&dir, "Test");
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(&dir)
            .args(args)
            .output()
            .unwrap();
    };
    let file = dir.join("a.txt");
    fs::write(&file, "old content\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "v1"]);
    fs::write(&file, "new content\n").unwrap();
    run(&["commit", "-qa", "-m", "v2"]);

    let mut app = app_at(&dir);
    app.open_initial(&file);
    assert_eq!(app.editor.active_tab().unwrap().text(), "new content\n");

    app.run_action("git.open_at_revision");
    assert!(app.prompt.is_some(), "prompts for a revision");
    for c in "HEAD~1".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));

    let tab = app.editor.active_tab().unwrap();
    assert_eq!(tab.text(), "old content\n", "the file's content at HEAD~1");
    assert!(tab.read_only);
    assert!(
        tab.title().contains("a.txt") && tab.title().contains('@'),
        "titled `a.txt @ <abbrev>`: {}",
        tab.title()
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn git_open_at_revision_with_an_unknown_revision_reports_an_error() {
    let dir = unique_dir("gitrevisionbad");
    fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    init_git_repo(&dir, "Test");
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(&dir)
            .args(args)
            .output()
            .unwrap();
    };
    let file = dir.join("a.txt");
    fs::write(&file, "content\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "v1"]);

    let mut app = app_at(&dir);
    app.open_initial(&file);
    let before = app.messages.items.len();
    let tabs_before = app.editor.tabs.len();

    app.run_action("git.open_at_revision");
    for c in "not-a-real-revision".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));

    assert!(app.messages.items.len() > before, "an error was reported");
    assert_eq!(app.editor.tabs.len(), tabs_before, "no tab was opened");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn refresh_git_populates_branch_when_in_a_repo() {
    let mut app = app_at(Path::new("."));
    app.refresh_git();
    if app.flags.contains(AppFlags::GIT_REPO) {
        assert!(app.git_branch.is_some(), "a repo reports a branch");
    }
    // The dirty flag is always consistent with the cached status list.
    assert_eq!(app.git_dirty(), !app.git_status.is_empty());
}

const TWO_CONFLICTS: &str = "a\n<<<<<<< HEAD\nours1\n=======\ntheirs1\n>>>>>>> branch\nmid\n\
    <<<<<<< HEAD\nours2\n=======\ntheirs2\n>>>>>>> branch\nz\n";

#[test]
fn conflict_list_opens_with_every_conflict_in_source_order() {
    let mut app = app_at(Path::new("."));
    buffer_with(&mut app, TWO_CONFLICTS, 0);
    app.run_action("git.conflict_list");
    let list = app.conflict_list.as_ref().expect("the overlay opened");
    assert_eq!(list.len(), 2);
    assert_eq!(list.entries[0].ours, "ours1\n");
    assert_eq!(list.entries[1].ours, "ours2\n");
}

#[test]
fn conflict_list_reports_no_conflict_on_a_clean_buffer() {
    let mut app = app_at(Path::new("."));
    buffer_with(&mut app, "hello world\n", 0);
    app.run_action("git.conflict_list");
    assert!(app.conflict_list.is_none(), "nothing to list");
}

#[test]
fn conflict_list_enter_jumps_to_the_selected_conflict_and_closes() {
    let mut app = app_at(Path::new("."));
    buffer_with(&mut app, TWO_CONFLICTS, 0);
    app.run_action("git.conflict_list");
    app.on_key(keycode(KeyCode::Down)); // select the second conflict
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.conflict_list.is_none(), "the overlay closed");
    // The cursor landed on the second conflict's `<<<<<<<` line (1-based
    // line 8 in TWO_CONFLICTS).
    assert_eq!(app.editor.cursor_1based().0, 8);
}

#[test]
fn conflict_list_resolve_key_resolves_without_closing_the_overlay() {
    let mut app = app_at(Path::new("."));
    buffer_with(&mut app, TWO_CONFLICTS, 0);
    app.run_action("git.conflict_list");
    app.on_key(key('o')); // keep ours for the first (currently selected) conflict
    let list = app
        .conflict_list
        .as_ref()
        .expect("one conflict remains, so the overlay stays open");
    assert_eq!(list.len(), 1);
    assert_eq!(list.entries[0].ours, "ours2\n");
    let content = app.editor.active_tab().unwrap().editor.get_content();
    assert!(
        content.contains("ours1") && !content.contains("theirs1"),
        "the first conflict resolved to ours: {content}"
    );
}

#[test]
fn conflict_list_resolve_key_closes_the_overlay_once_none_remain() {
    let mut app = app_at(Path::new("."));
    buffer_with(&mut app, TWO_CONFLICTS, 0);
    app.run_action("git.conflict_list");
    app.on_key(key('o'));
    app.on_key(key('t')); // resolve the (now-first) remaining conflict too
    assert!(
        app.conflict_list.is_none(),
        "no conflicts left, overlay closed"
    );
}

#[test]
fn conflict_list_esc_closes_without_changing_the_buffer() {
    let mut app = app_at(Path::new("."));
    buffer_with(&mut app, TWO_CONFLICTS, 0);
    let before = app.editor.active_tab().unwrap().editor.get_content();
    app.run_action("git.conflict_list");
    app.on_key(esc());
    assert!(app.conflict_list.is_none());
    assert_eq!(
        app.editor.active_tab().unwrap().editor.get_content(),
        before
    );
}

#[test]
fn conflict_list_renders_without_panicking() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut app = app_at(Path::new("."));
    buffer_with(&mut app, TWO_CONFLICTS, 0);
    app.run_action("git.conflict_list");
    let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    let screen: String = term
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect();
    assert!(screen.contains("ours1"), "a conflict preview is shown");
}
