#![allow(
    clippy::cast_possible_truncation,
    clippy::format_collect,
    reason = "tests count and index small fixtures, and build expected text with format! over iterators"
)]
// Shared fixtures/helpers live in `common.rs`; every test here needs a
// handful of them, so a glob import earns its keep over a long explicit list.
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use crate::integration::common::*;

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

/// T577: with nothing staged but a dirty tree, Git → Commit asks (default),
/// stages everything without asking ("always"), or refuses ("never").
#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn smart_commit_ask_always_and_never() {
    let dir = unique_dir("smartcommit");
    fs::create_dir_all(&dir).unwrap();
    init_git_repo(&dir, "Test");
    fs::write(dir.join("a.txt"), "hello\n").unwrap();
    let mut app = app_at(&dir);
    let press_c = |app: &mut App| {
        app.run_action("git.changes");
        app.on_key(keycode(KeyCode::Char('c')));
    };

    app.settings.smart_commit = "never".to_string();
    press_c(&mut app);
    assert!(
        app.confirm.is_none() && app.prompt.is_none(),
        "never refuses"
    );

    app.settings.smart_commit = "ask".to_string();
    press_c(&mut app);
    assert!(app.confirm.is_some(), "ask shows a confirm");
    assert!(
        app.git_status.iter().all(|f| !f.is_staged()),
        "nothing staged yet"
    );
    app.on_key(keycode(KeyCode::Esc));
    assert!(
        app.confirm.is_none() && app.prompt.is_none(),
        "no declines cleanly"
    );
    assert!(app.git_status.iter().all(|f| !f.is_staged()));

    press_c(&mut app);
    app.on_key(key('y'));
    assert!(
        app.confirm.is_none() && app.prompt.is_some(),
        "yes opens the commit prompt"
    );
    assert!(app.git_status.iter().all(vix::git::FileStatus::is_staged));

    fs::remove_dir_all(&dir).ok();

    // "always" skips the confirm.
    let dir = unique_dir("smartcommit-always");
    fs::create_dir_all(&dir).unwrap();
    init_git_repo(&dir, "Test");
    fs::write(dir.join("a.txt"), "hello\n").unwrap();
    let mut app = app_at(&dir);
    app.settings.smart_commit = "always".to_string();
    press_c(&mut app);
    assert!(app.confirm.is_none() && app.prompt.is_some());
    fs::remove_dir_all(&dir).ok();
}

/// T125: the Git panel's `g` key generates a commit message from the staged
/// diff and opens the commit prompt pre-filled with it -- it only fills the
/// message box, it never commits on its own.
#[cfg(feature = "ai")]
#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn git_panel_generates_a_commit_message_from_the_staged_diff() {
    let dir = unique_dir("gitpanel-ai-commit");
    fs::create_dir_all(&dir).unwrap();
    init_git_repo(&dir, "Test");
    fs::write(dir.join("a.txt"), "hello\n").unwrap();

    // Deterministic `ai_command`: ignores its input, always prints the same
    // message -- no real assistant needed to exercise the wiring.
    let settings = {
        let mut s = Settings::default();
        s.ai_command = "printf '%s' 'Add a.txt with a greeting'".to_string();
        s
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

/// T730: with `ai = "off"` the Git panel's `g` key generates nothing -- the AI
/// command never runs and no commit prompt opens.
#[cfg(feature = "ai")]
#[test]
#[ignore = "needs git; creates a throwaway repo"]
fn git_panel_generates_no_commit_message_with_ai_off() {
    let dir = unique_dir("gitpanel-ai-off");
    fs::create_dir_all(&dir).unwrap();
    init_git_repo(&dir, "Test");
    fs::write(dir.join("a.txt"), "hello\n").unwrap();
    let marker = dir.join("ai-ran");
    let settings = {
        let mut s = Settings::default();
        s.ai = "off".to_string();
        s.ai_command = format!("touch {}", marker.display());
        s
    };
    let mut app = app_at_with(&dir, settings);

    app.run_action("git.changes");
    app.on_key(keycode(KeyCode::Char(' '))); // stage the file
    app.on_key(keycode(KeyCode::Char('g'))); // would generate a message
    assert!(!app.ai_replace_running(), "no AI request started");
    std::thread::sleep(Duration::from_millis(200));
    app.poll_ai_replace();
    assert!(app.prompt.is_none(), "no commit prompt opened");
    assert!(!marker.exists(), "the AI command never ran");
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
#[cfg(feature = "git")]
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
#[cfg(feature = "git")]
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
#[cfg(feature = "git")]
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

/// T650: a throwaway repo with distinct `f` and `sub/f`, committed, and an
/// App rooted at `sub/` (Vix started in a repository subdirectory).
fn app_in_repo_subdir(tag: &str) -> (PathBuf, App) {
    let dir = unique_dir(tag);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("sub")).unwrap();
    let dir = dir.canonicalize().unwrap();
    init_git_repo(&dir, "Ada Lovelace");
    fs::write(dir.join("f"), "root one\nroot two\n").unwrap();
    fs::write(dir.join("sub/f"), "sub one\nsub two\n").unwrap();
    for args in [
        &["add", "."][..],
        &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "seed"],
    ] {
        std::process::Command::new("git")
            .current_dir(&dir)
            .args(args)
            .output()
            .unwrap();
    }
    let mut app = app_at(&dir.join("sub"));
    app.refresh_git();
    (dir, app)
}

/// The staged (index) content of `path` in the repo at `dir`.
fn index_content(dir: &Path, path: &str) -> String {
    let out = std::process::Command::new("git")
        .current_dir(dir)
        .args(["show", &format!(":{path}")])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn git_works_when_vix_is_rooted_in_a_repo_subdirectory() {
    let (dir, mut app) = app_in_repo_subdir("gitsubdir");
    assert!(app.flags.contains(AppFlags::GIT_REPO));
    app.open_initial(&dir.join("sub/f"));

    // Gutter: an untouched file has no marks (it used to diff against the
    // root `f`'s HEAD blob); one edit marks exactly one line.
    app.refresh_git_gutter();
    let marks = |app: &App| {
        app.editor
            .active_tab()
            .unwrap()
            .editor
            .gutter_marks()
            .cloned()
            .unwrap_or_default()
    };
    assert!(marks(&app).is_empty(), "unchanged file: {:?}", marks(&app));
    app.on_key(key('x'));
    app.refresh_git_gutter();
    assert_eq!(marks(&app).len(), 1, "one edited line: {:?}", marks(&app));

    // Stage Hunk writes `sub/f`'s index entry and leaves the root `f`'s alone.
    app.run_action("git.stage_hunk");
    assert_eq!(index_content(&dir, "sub/f"), "xsub one\nsub two\n");
    assert_eq!(index_content(&dir, "f"), "root one\nroot two\n");

    // Blame names the subdirectory file's commit.
    app.run_action("git.blame");
    assert!(app.status.contains("Ada Lovelace"), "{}", app.status);

    // The status rows are relative to the root, so the explorer badge and
    // the panel agree on the file.
    assert!(app.git_change_for(&dir.join("sub/f")).is_some());
    fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn git_panel_stage_selected_works_from_a_repo_subdirectory() {
    let (dir, mut app) = app_in_repo_subdir("gitsubpanel");
    fs::write(dir.join("sub/f"), "sub changed\n").unwrap();
    fs::write(dir.join("f"), "root changed\n").unwrap();
    app.run_action("git.changes");
    let paths: Vec<&str> = app.git_status.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(paths, ["../f", "f"], "paths relative to the Vix root");
    // Stage both rows ('s'), moving down between them.
    app.on_key(keycode(KeyCode::Char('s')));
    app.on_key(keycode(KeyCode::Down));
    app.on_key(keycode(KeyCode::Char('s')));
    assert!(
        app.git_status.iter().all(vix::git::FileStatus::is_staged),
        "both staged: {:?} (messages: {:?})",
        app.git_status,
        app.status
    );
    assert_eq!(index_content(&dir, "sub/f"), "sub changed\n");
    assert_eq!(index_content(&dir, "f"), "root changed\n");
    fs::remove_dir_all(&dir).ok();
}

/// T657: the Git panel opens a file's changes as a diff on Enter, and `d`
/// discards them after a confirmation; `git.diff_index` diffs against the
/// staged copy; an untracked file is never deleted by Discard.
#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn git_panel_opens_and_discards_changes() {
    let dir = unique_dir("gitpanel-open-changes");
    fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    init_git_repo(&dir, "Test");
    fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
    for args in [
        &["add", "."][..],
        &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "seed"],
    ] {
        std::process::Command::new("git")
            .current_dir(&dir)
            .args(args)
            .output()
            .unwrap();
    }
    fs::write(dir.join("a.txt"), "one\nTWO\n").unwrap();
    let mut app = app_at(&dir);
    app.run_action("git.changes");
    app.on_key(keycode(KeyCode::Enter));
    let view = app.diff_view.take().expect("Enter opens the diff");
    assert!(view.title.contains("HEAD") && view.title.contains("a.txt"));

    // Nothing is staged, so the active file also differs from the index.
    app.open_initial(&dir.join("a.txt"));
    app.run_action("git.diff_index");
    let view = app.diff_view.take().expect("git.diff_index opens the diff");
    assert!(view.title.contains("Staged"));
    app.git_panel = Some(vix::app::GitPanel { selected: 0 });
    app.on_key(key('d'));
    assert_eq!(
        app.confirm.as_ref().map(|c| c.kind),
        Some(vix::app::ConfirmKind::GitDiscard)
    );
    app.on_key(key('y'));
    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "one\ntwo\n");

    // Untracked: refused, file kept.
    fs::write(dir.join("new.txt"), "x\n").unwrap();
    app.run_action("git.changes");
    let idx = app
        .git_status
        .iter()
        .position(|s| s.path == "new.txt")
        .expect("listed");
    app.git_panel = Some(vix::app::GitPanel { selected: idx });
    app.on_key(key('d'));
    assert!(app.confirm.is_none(), "no confirm for an untracked file");
    assert!(dir.join("new.txt").exists());
    fs::remove_dir_all(&dir).ok();
}

/// T657: inline/line blame follows unsaved edits rather than the copy on disk.
#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn blame_follows_unsaved_edits() {
    let dir = unique_dir("blame-unsaved");
    fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    init_git_repo(&dir, "Test");
    fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
    for args in [
        &["add", "."][..],
        &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "seed"],
    ] {
        std::process::Command::new("git")
            .current_dir(&dir)
            .args(args)
            .output()
            .unwrap();
    }
    let mut app = app_at(&dir);
    app.open_initial(&dir.join("a.txt"));
    // Insert a new first line without saving; the cursor stays on it.
    type_str(&mut app, "inserted\n");
    app.on_key(keycode(KeyCode::Up));
    app.run_action("git.blame");
    assert!(
        !app.status.contains("seed"),
        "line 1 is the unsaved line, not the committed one: {}",
        app.status
    );
    app.on_key(keycode(KeyCode::Down));
    app.run_action("git.blame");
    assert!(
        app.status.contains("seed"),
        "line 2 is the committed first line: {}",
        app.status
    );
    fs::remove_dir_all(&dir).ok();
}

/// T681: a throwaway repo with a merge, for the Git graph tests. `main` ends
/// at the merge commit, `topic` at "side".
fn graph_repo(tag: &str) -> PathBuf {
    let dir = unique_dir(tag);
    fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    init_git_repo(&dir, "Test");
    let run = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .current_dir(&dir)
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}");
    };
    fs::write(dir.join("a.txt"), "a\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "base"]);
    run(&["switch", "-q", "-c", "topic"]);
    fs::write(dir.join("side.txt"), "side\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "side"]);
    run(&["switch", "-q", "-"]);
    fs::write(dir.join("a.txt"), "a\nmain\n").unwrap();
    run(&["commit", "-qa", "-m", "mainline"]);
    run(&["merge", "-q", "--no-ff", "-m", "merge topic", "topic"]);
    dir
}

#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn git_graph_lists_lanes_searches_filters_and_opens_the_diff_tab() {
    let dir = graph_repo("gitgraph");
    let mut app = app_at(&dir);
    app.run_action("git.graph");
    let g = app.git_graph.as_ref().expect("graph panel opens");
    assert_eq!(g.commits.len(), 4);
    assert_eq!(g.commits[0].subject, "merge topic");
    assert_eq!(g.commits[0].parents.len(), 2, "the merge has two parents");
    assert!(g.graph_width() >= 2, "the merge needs a second lane");

    // Typing searches (hides rows; Backspace restores them).
    type_str(&mut app, "side");
    assert_eq!(app.git_graph.as_ref().unwrap().len(), 1);
    for _ in 0..4 {
        app.on_key(keycode(KeyCode::Backspace));
    }
    assert_eq!(app.git_graph.as_ref().unwrap().len(), 4);

    // Tab limits the graph to one ref; cycling all the way round restores it.
    app.on_key(keycode(KeyCode::Tab));
    let g = app.git_graph.as_ref().unwrap();
    assert!(g.reference.is_some(), "a ref filter is active");
    let refs = g.refs.len();
    for _ in 0..refs {
        app.on_key(keycode(KeyCode::Tab));
    }
    let g = app.git_graph.as_ref().unwrap();
    assert!(g.reference.is_none() && g.commits.len() == 4);

    // Enter opens the highlighted commit as a read-only `git show` tab.
    let tabs_before = app.editor.tabs.len();
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.git_graph.is_none(), "Enter closes the graph");
    assert_eq!(app.editor.tabs.len(), tabs_before + 1);
    let tab = app.editor.active_tab().unwrap();
    assert!(tab.read_only && tab.title().contains("graph"));
    assert!(tab.text().contains("merge topic"));

    fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn git_graph_checkout_branch_and_cherry_pick_confirm_before_acting() {
    let dir = graph_repo("gitgraphact");
    let head = |dir: &Path| {
        let out = std::process::Command::new("git")
            .current_dir(dir)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    let mut app = app_at(&dir);
    app.run_action("git.graph");

    // Search for the topic tip, then Ctrl+O: nothing happens until `y`.
    type_str(&mut app, "side");
    app.on_key(ctrl('o'));
    assert_eq!(head(&dir), "main", "checkout waits for confirmation");
    app.on_key(key('n'));
    assert_eq!(head(&dir), "main", "n cancels");
    app.on_key(ctrl('o'));
    app.on_key(key('y'));
    assert_eq!(
        head(&dir),
        "topic",
        "y checks out the commit's local branch"
    );

    // Ctrl+B asks for a name, then branches at the highlighted commit.
    app.on_key(ctrl('b'));
    type_str(&mut app, "from-side");
    app.on_key(keycode(KeyCode::Enter));
    assert_eq!(head(&dir), "from-side");

    // Cherry-pick the mainline commit (confirmed), then Ctrl+Y copies a hash.
    for _ in 0..4 {
        app.on_key(keycode(KeyCode::Backspace));
    }
    type_str(&mut app, "mainline");
    app.on_key(ctrl('p'));
    app.on_key(key('y'));
    let a = fs::read_to_string(dir.join("a.txt")).unwrap();
    assert!(a.contains("main"), "the picked change landed: {a:?}");
    app.on_key(ctrl('y'));
    assert!(app.status.contains("Copied"), "{}", app.status);

    app.on_key(keycode(KeyCode::Esc));
    assert!(app.git_graph.is_none());
    fs::remove_dir_all(&dir).ok();
}

/// A fixture `git` for the T634 tests: isolated from the developer's global
/// and system config on this command alone, never through the process env.
fn t634_git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .current_dir(dir)
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A one-commit repository under `<unique>/repo`; its local config carries
/// the identity and turns signing off, so the app's own git calls need no
/// global setup.
fn t634_repo(name: &str) -> PathBuf {
    let base = unique_dir(name);
    let dir = base.join("repo");
    fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    t634_git(&dir, &["init", "-q"]);
    for (k, v) in [
        ("user.name", "Vix Test"),
        ("user.email", "test@example.invalid"),
        ("commit.gpgsign", "false"),
    ] {
        t634_git(&dir, &["config", k, v]);
    }
    fs::write(dir.join("a.txt"), "one\n").unwrap();
    t634_git(&dir, &["add", "a.txt"]);
    t634_git(&dir, &["commit", "-q", "-m", "init"]);
    dir
}

/// T634: Git → Stash List… shows, applies, pops and drops stashes, and a
/// drop asks first.
#[test]
fn git_stash_list_chooser_shows_pops_and_drops_after_confirming() {
    let dir = t634_repo("t634stash");
    let file = dir.join("a.txt");
    let mut app = app_at(&dir);
    app.refresh_git();
    fs::write(&file, "one\ntwo\n").unwrap();
    app.run_action("git.stash");
    fs::write(&file, "one\nthree\n").unwrap();
    app.run_action("git.stash");

    app.run_action("git.stash_list");
    assert_eq!(app.chooser.stash().unwrap().entries.len(), 2);

    // `d` asks; `n` keeps the stash and brings the chooser back.
    app.on_key(key('d'));
    assert!(app.chooser.stash().is_none());
    assert_eq!(
        app.confirm.as_ref().map(|c| c.kind),
        Some(vix::app::ConfirmKind::GitStashDrop)
    );
    app.on_key(key('n'));
    assert!(app.confirm.is_none());
    assert_eq!(app.chooser.stash().unwrap().entries.len(), 2);

    // `d` then `y` drops the newest ("three").
    app.on_key(key('d'));
    app.on_key(key('y'));
    assert_eq!(app.chooser.stash().unwrap().entries.len(), 1);

    // `a` applies the remaining one and keeps it.
    app.on_key(key('a'));
    assert!(app.chooser.stash().is_none());
    assert_eq!(fs::read_to_string(&file).unwrap(), "one\ntwo\n");
    t634_git(&dir, &["checkout", "--", "a.txt"]);

    // Enter shows its diff in a read-only tab.
    app.run_action("git.stash_list");
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.chooser.stash().is_none());
    let tab = app.editor.active_tab().unwrap();
    assert_eq!(tab.path.as_deref(), Some(Path::new("stash@{0}")));
    assert!(tab.text().contains("+two"));

    // `p` pops it: the tree has "two" again and the list is empty, so the
    // chooser closes.
    app.run_action("git.stash_list");
    app.on_key(key('p'));
    assert!(app.chooser.stash().is_none());
    assert_eq!(fs::read_to_string(&file).unwrap(), "one\ntwo\n");

    // No stashes: the chooser doesn't open.
    app.run_action("git.stash_list");
    assert!(app.chooser.stash().is_none());
    fs::remove_dir_all(dir.parent().unwrap()).ok();
}

/// T634: Git → Worktree adds, lists, switches to, and (after confirming)
/// removes a worktree.
#[test]
fn git_worktree_add_list_switch_and_remove() {
    let dir = t634_repo("t634wt");
    let mut app = app_at(&dir);
    app.refresh_git();

    app.run_action("git.worktree_add");
    assert!(app.prompt.is_some());
    for ch in "../wt-one".chars() {
        app.on_key(key(ch));
    }
    app.on_key(keycode(KeyCode::Enter));
    let wt = dir.parent().unwrap().join("wt-one");
    assert!(wt.join("a.txt").is_file(), "{}", app.status);

    app.run_action("git.worktree_list");
    let c = app.chooser.worktree().unwrap();
    assert_eq!(c.worktrees.len(), 2);
    assert_eq!(c.selected, 0, "the current worktree is highlighted");
    assert_eq!(c.worktrees[1].branch(), Some("wt-one"));

    // The main worktree can't be removed.
    app.on_key(key('d'));
    assert!(app.confirm.is_none());

    // Remove mode: Enter asks, `y` removes, and the list comes back.
    app.chooser = None;
    app.run_action("git.worktree_remove");
    app.on_key(keycode(KeyCode::Down));
    app.on_key(keycode(KeyCode::Enter));
    assert_eq!(
        app.confirm.as_ref().map(|c| c.kind),
        Some(vix::app::ConfirmKind::GitWorktreeRemove)
    );
    app.on_key(key('y'));
    assert!(!wt.exists());
    assert_eq!(app.chooser.worktree().unwrap().worktrees.len(), 1);
    app.chooser = None;

    // Add another and switch to it through the project-switch path.
    app.run_action("git.worktree_add");
    for ch in "../wt-two".chars() {
        app.on_key(key(ch));
    }
    app.on_key(keycode(KeyCode::Enter));
    app.run_action("git.worktree_list");
    app.on_key(keycode(KeyCode::Down));
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.chooser.worktree().is_none());
    let two = dir.parent().unwrap().join("wt-two").canonicalize().unwrap();
    assert_eq!(app.root.canonicalize().unwrap(), two);
    fs::remove_dir_all(dir.parent().unwrap()).ok();
}

/// T664: the change gutter's base is a setting; unknown values normalize to
/// `head`, and the default is `head`.
#[test]
fn gutter_base_setting_defaults_to_head_and_normalizes() {
    let mut app = app_at(Path::new("."));
    assert_eq!(app.settings.git_gutter_base, "head");
    app.set_gutter_base("index");
    assert_eq!(app.settings.git_gutter_base, "index");
    app.set_gutter_base("branch:dev");
    assert_eq!(app.settings.git_gutter_base, "branch:dev");
    app.set_gutter_base("nonsense");
    assert_eq!(app.settings.git_gutter_base, "head");
}

/// T664 (resolving T657's staged-hunk marking): after Stage Hunk the gutter
/// still marks the hunk against `head` but not against `index`; against
/// `disk` it marks only unsaved edits; against a branch it marks the whole
/// difference from that branch. The chooser action lists every base.
#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn gutter_base_changes_what_the_gutter_marks() {
    let (dir, mut app) = app_in_repo_subdir("gutterbase");
    app.open_initial(&dir.join("sub/f"));
    let marks = |app: &App| {
        app.editor
            .active_tab()
            .unwrap()
            .editor
            .gutter_marks()
            .map_or(0, Vec::len)
    };
    app.on_key(key('x'));
    app.refresh_git_gutter();
    assert_eq!(marks(&app), 1, "edit is marked against head");

    app.run_action("git.stage_hunk");
    app.refresh_git_gutter();
    assert_eq!(marks(&app), 1, "head still marks the staged hunk");
    app.set_gutter_base("index");
    app.refresh_git_gutter();
    assert_eq!(marks(&app), 0, "index no longer marks the staged hunk");

    // Disk: the edit is unsaved, so it is marked; after saving it is not.
    app.set_gutter_base("disk");
    app.refresh_git_gutter();
    assert_eq!(marks(&app), 1, "disk marks the unsaved edit");
    app.run_action("file.save");
    app.refresh_git_gutter();
    assert_eq!(marks(&app), 0, "saved edit is no longer a difference");

    // Branch: a branch cut before the edit differs by that edit.
    std::process::Command::new("git")
        .current_dir(&dir)
        .args(["branch", "base-branch", "HEAD"])
        .output()
        .unwrap();
    app.set_gutter_base("branch:base-branch");
    app.refresh_git_gutter();
    assert_eq!(marks(&app), 1, "branch base marks the uncommitted edit");

    // The chooser lists head, index, disk and the branches, preselecting the
    // current base; choosing a row applies it.
    app.run_action("git.gutter_base");
    let c = app.chooser.branch().expect("chooser opens");
    assert!(c.gutter);
    assert_eq!(&c.branches[..3], ["head", "index", "disk"]);
    assert_eq!(c.branches[c.selected], "branch:base-branch");
    app.on_key(keycode(KeyCode::Up));
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.chooser.branch().is_none());
    assert_ne!(app.settings.git_gutter_base, "branch:base-branch");
    fs::remove_dir_all(&dir).ok();
}

const REVIEW_DIFF: &str = "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1,3 +1,3 @@\n keep\n-old\n+new\n tail\n@@ -10,1 +10,2 @@\n ctx\n+more\n";

fn type_text(app: &mut App, text: &str) {
    for ch in text.chars() {
        app.on_key(key(ch));
    }
}

/// T665: a range opens as one read-only multi-file buffer; a comment on a
/// diff line is saved under `.vix/review/`, shows under the line, exports as
/// Markdown, and comes back when the same range is reviewed again.
#[test]
fn review_comments_save_export_and_reload() {
    let dir = unique_dir("review-comments");
    let _ = fs::remove_dir_all(dir.join(".vix"));
    let mut app = app_at(&dir);
    app.open_review_from_diff("main..feat", REVIEW_DIFF);
    let tab = app.editor.active_tab().unwrap();
    assert_eq!(tab.path.as_deref(), Some(Path::new("review main..feat")));
    assert!(tab.read_only);
    assert!(tab.text().contains("=== src/a.rs"));

    // Not on a diff line: refused with a status, no prompt.
    app.editor.goto(1, None, app.layout.editor);
    app.run_action("git.review_comment");
    assert!(app.prompt.is_none());

    // Row 7 (1-based) is the `+new` line.
    app.editor.goto(7, None, app.layout.editor);
    app.run_action("git.review_comment");
    assert!(matches!(
        app.prompt.as_ref().map(|p| p.kind),
        Some(PromptKind::GitReviewComment)
    ));
    type_text(&mut app, "why rename?");
    app.on_key(keycode(KeyCode::Enter));
    let state = app.review.as_ref().unwrap();
    assert_eq!(state.review.comments.len(), 1);
    assert_eq!(state.review.comments[0].line, 2);
    assert_eq!(state.review.comments[0].code, "new");
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .contains("    >> why rename?")
    );
    let json = fs::read_to_string(dir.join(".vix/review/main..feat.json")).unwrap();
    assert!(json.contains("why rename?"));

    app.run_action("git.review_export");
    let md = fs::read_to_string(dir.join(".vix/review/main..feat.md")).unwrap();
    assert!(md.contains("## `src/a.rs`") && md.contains("- **Line 2**: why rename?"));

    // Reviewing the same range in a fresh session reloads the comment.
    let mut again = app_at(&dir);
    again.open_review_from_diff("main..feat", REVIEW_DIFF);
    assert_eq!(again.review.as_ref().unwrap().review.comments.len(), 1);
    assert!(
        again
            .editor
            .active_tab()
            .unwrap()
            .text()
            .contains(">> why rename?")
    );

    // Commenting again with empty text removes it from the buffer and file.
    again.editor.goto(7, None, again.layout.editor);
    again.run_action("git.review_comment");
    assert_eq!(again.prompt.as_ref().unwrap().input, "why rename?");
    for _ in 0.."why rename?".len() {
        again.on_key(keycode(KeyCode::Backspace));
    }
    again.on_key(keycode(KeyCode::Enter));
    assert_eq!(again.review.as_ref().unwrap().review.comments.len(), 0);
    assert!(!again.editor.active_tab().unwrap().text().contains(">>"));
    fs::remove_dir_all(&dir).ok();
}

/// T665: next/prev hunk move between hunk headers and wrap; outside a review
/// tab the actions only report that.
#[test]
fn review_hunk_navigation_wraps_and_needs_a_review_tab() {
    let dir = unique_dir("review-nav");
    let mut app = app_at(&dir);
    app.run_action("git.review_next");
    assert!(app.review.is_none());
    app.open_review_from_diff("a..b", REVIEW_DIFF);
    app.editor.goto(1, None, app.layout.editor);
    app.run_action("git.review_next");
    assert_eq!(app.editor.cursor_line(), 3);
    app.run_action("git.review_next");
    assert_eq!(app.editor.cursor_line(), 8);
    app.run_action("git.review_next");
    assert_eq!(app.editor.cursor_line(), 3, "wraps to the first hunk");
    app.run_action("git.review_prev");
    assert_eq!(app.editor.cursor_line(), 8, "wraps to the last hunk");
    // An empty diff opens nothing.
    let before = app.editor.tabs.len();
    app.open_review_from_diff("x..y", "");
    assert_eq!(app.editor.tabs.len(), before);
    fs::remove_dir_all(&dir).ok();
}

/// T665: `git.review` asks for a range and reviews what git reports for it,
/// both for an `a..b` range and for a single commit.
#[test]
#[ignore = "needs git; creates a throwaway repo and commits in it"]
fn review_range_and_single_commit_use_real_git_diffs() {
    let dir = unique_dir("review-git").canonicalize().unwrap();
    init_git_repo(&dir, "Ada Lovelace");
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(&dir)
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .unwrap();
    };
    fs::write(dir.join("f"), "one\ntwo\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "seed"]);
    fs::write(dir.join("f"), "one\nTWO\n").unwrap();
    git(&["commit", "-q", "-am", "change"]);
    let mut app = app_at(&dir);
    app.refresh_git();
    for range in ["HEAD~1..HEAD", "HEAD"] {
        app.run_action("git.review");
        assert!(matches!(
            app.prompt.as_ref().map(|p| p.kind),
            Some(PromptKind::GitReview)
        ));
        type_text(&mut app, range);
        app.on_key(keycode(KeyCode::Enter));
        let state = app.review.as_ref().unwrap();
        assert_eq!(state.review.range, range);
        assert_eq!(state.review.files[0].path, "f");
        assert!(app.editor.active_tab().unwrap().text().contains("+TWO"));
    }
    fs::remove_dir_all(&dir).ok();
}

/// The subjects of `dir`'s commits, newest first (T633).
fn t633_subjects(dir: &Path) -> Vec<String> {
    let out = std::process::Command::new("git")
        .current_dir(dir)
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(["log", "--format=%s"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

/// T633: Git → Rebase → Interactive Rebase… lists the commits after the
/// base; `s` squashes, Alt+↑ reorders, `r` rewords through a prompt, and
/// Enter runs the rebase. A leading squash is refused before git runs.
#[test]
fn git_interactive_rebase_squashes_reorders_and_rewords() {
    let dir = t634_repo("t633rebase");
    for (file, subject) in [
        ("one.txt", "one"),
        ("two.txt", "two"),
        ("three.txt", "three"),
    ] {
        fs::write(dir.join(file), subject).unwrap();
        t634_git(&dir, &["add", file]);
        t634_git(&dir, &["commit", "-q", "-m", subject]);
    }
    let mut app = app_at(&dir);
    app.refresh_git();

    app.run_action("git.rebase_interactive");
    type_str(&mut app, "HEAD~3");
    app.on_key(keycode(KeyCode::Enter));
    let todo = app.git_rebase.as_ref().expect("the todo overlay opens");
    let subjects: Vec<&str> = todo.commits.iter().map(|c| c.subject.as_str()).collect();
    assert_eq!(subjects, ["one", "two", "three"]);

    // A squash on the first row has nothing to meld into: refused.
    app.on_key(key('s'));
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.git_rebase.is_some(), "the overlay stays open");
    assert_eq!(t633_subjects(&dir).len(), 4, "git never ran");
    app.on_key(key('p'));

    // three, one, two: move `three` up twice, reword it, squash `two`.
    app.on_key(keycode(KeyCode::Down));
    app.on_key(keycode(KeyCode::Down));
    app.on_key(alt(KeyCode::Up));
    app.on_key(alt(KeyCode::Up));
    app.on_key(key('r'));
    assert!(app.prompt.is_some(), "r prompts for the new message");
    for _ in 0.."three".len() {
        app.on_key(keycode(KeyCode::Backspace));
    }
    type_str(&mut app, "third");
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.prompt.is_none());
    let todo = app.git_rebase.as_ref().unwrap();
    assert_eq!(todo.commits[0].message.as_deref(), Some("third"));
    app.on_key(keycode(KeyCode::Down));
    app.on_key(keycode(KeyCode::Down));
    app.on_key(key('s'));
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.git_rebase.is_none());
    assert_eq!(t633_subjects(&dir), ["one", "third", "init"]);
    assert!(
        dir.join("two.txt").is_file(),
        "the squash kept two's change"
    );

    // Nothing stopped, so Continue and Abort just say so.
    app.run_action("git.rebase_continue");
    app.run_action("git.rebase_abort");
    assert_eq!(t633_subjects(&dir), ["one", "third", "init"]);
    fs::remove_dir_all(dir.parent().unwrap()).ok();
}

/// T633: `edit` stops the rebase; Continue finishes it, and Abort undoes
/// another; Git → Rebase → Cherry-Pick… opens Browse Log, where `c` picks.
#[test]
fn git_rebase_edit_continue_abort_and_cherry_pick() {
    let dir = t634_repo("t633edit");
    for (file, subject) in [("one.txt", "one"), ("two.txt", "two")] {
        fs::write(dir.join(file), subject).unwrap();
        t634_git(&dir, &["add", file]);
        t634_git(&dir, &["commit", "-q", "-m", subject]);
    }
    let mut app = app_at(&dir);
    app.refresh_git();

    app.run_action("git.rebase_interactive");
    type_str(&mut app, "HEAD~2");
    app.on_key(keycode(KeyCode::Enter));
    app.on_key(key('e'));
    app.on_key(keycode(KeyCode::Enter));
    assert!(vix::git::rebase_in_progress(&dir), "edit stops the rebase");
    // A second rebase can't start while one is stopped.
    app.run_action("git.rebase_interactive");
    assert!(app.prompt.is_none());
    app.run_action("git.rebase_continue");
    assert!(!vix::git::rebase_in_progress(&dir));
    assert_eq!(t633_subjects(&dir), ["two", "one", "init"]);

    app.run_action("git.rebase_interactive");
    type_str(&mut app, "HEAD~1");
    app.on_key(keycode(KeyCode::Enter));
    app.on_key(key('e'));
    app.on_key(keycode(KeyCode::Enter));
    assert!(vix::git::rebase_in_progress(&dir));
    app.run_action("git.rebase_abort");
    assert!(!vix::git::rebase_in_progress(&dir));

    // Cherry-pick `one` onto a side branch made before it.
    let main = vix::git::branch(&dir).unwrap();
    t634_git(&dir, &["switch", "-q", "-c", "side", "HEAD~2"]);
    app.refresh_git();
    app.run_action("git.cherry_pick");
    type_str(&mut app, &main);
    app.on_key(keycode(KeyCode::Enter));
    let log = app.git_log.as_ref().expect("the commit list opens");
    let offered: Vec<&str> = log.entries.iter().map(|e| e.subject.as_str()).collect();
    assert_eq!(offered, ["two", "one"], "only what `side` lacks");
    app.on_key(keycode(KeyCode::Down));
    app.on_key(key('c'));
    assert!(app.git_log.is_none());
    assert_eq!(t633_subjects(&dir), ["one", "init"]);
    assert!(dir.join("one.txt").is_file());

    // Nothing left to offer from `side` itself.
    app.run_action("git.cherry_pick");
    type_str(&mut app, "side");
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.git_log.is_none());
    // In a plain Browse Log, `c` does nothing.
    app.run_action("git.browse_log");
    app.on_key(key('c'));
    assert!(app.git_log.is_some());
    assert_eq!(t633_subjects(&dir), ["one", "init"]);
    fs::remove_dir_all(dir.parent().unwrap()).ok();
}

/// `git diff` of `dir` with the prefixes Review expects.
fn t665_diff(dir: &Path) -> String {
    t665_run(dir, &["diff", "--src-prefix=a/", "--dst-prefix=b/"])
}

/// `git diff --cached` of `dir`.
fn t665_cached(dir: &Path) -> String {
    t665_run(dir, &["diff", "--cached"])
}

/// Stdout of `git <args>` in `dir`, with the user's git config ignored.
fn t665_run(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .current_dir(dir)
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(args)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// T665 follow-up: with the cursor in a hunk of the review, Stage Review Hunk
/// puts exactly that hunk in the index, and Discard Review Hunk asks first and
/// then undoes it in the working tree. Outside a hunk both only say so.
#[test]
fn review_stages_and_discards_the_hunk_under_the_cursor() {
    let dir = t634_repo("t665hunk");
    let file = dir.join("a.txt");
    let mut app = app_at(&dir);
    app.refresh_git();
    fs::write(&file, "one\ntwo\n").unwrap();
    app.open_review_from_diff("work", &t665_diff(&dir));
    let hunk_row = app.review.as_ref().unwrap().rendered.hunk_rows()[0];

    // On the title row: refused, nothing is asked or staged.
    app.editor.goto(1, None, app.layout.editor);
    app.run_action("git.review_stage_hunk");
    app.run_action("git.review_discard_hunk");
    assert!(app.confirm.is_none());
    assert!(!t665_cached(&dir).contains("+two"));

    // Stage the hunk.
    app.editor.goto(hunk_row + 2, None, app.layout.editor);
    app.run_action("git.review_stage_hunk");
    assert!(t665_cached(&dir).contains("+two"));

    // Discard asks; `n` keeps the change, `y` undoes it.
    app.run_action("git.review_discard_hunk");
    assert_eq!(
        app.confirm.as_ref().map(|c| c.kind),
        Some(vix::app::ConfirmKind::GitReviewDiscard)
    );
    app.on_key(key('n'));
    assert!(app.confirm.is_none());
    assert_eq!(fs::read_to_string(&file).unwrap(), "one\ntwo\n");
    app.run_action("git.review_discard_hunk");
    app.on_key(key('y'));
    assert!(app.confirm.is_none());
    assert_eq!(fs::read_to_string(&file).unwrap(), "one\n");

    // The hunk no longer applies: git's reason is reported, nothing changes.
    app.run_action("git.review_discard_hunk");
    app.on_key(key('y'));
    assert_eq!(fs::read_to_string(&file).unwrap(), "one\n");
    fs::remove_dir_all(dir.parent().unwrap()).ok();
}

/// T665 follow-up: `r` in the stash chooser reviews the highlighted stash.
#[test]
fn stash_chooser_r_opens_the_stash_in_review() {
    let dir = t634_repo("t665stash");
    let mut app = app_at(&dir);
    app.refresh_git();
    fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
    app.run_action("git.stash");
    app.run_action("git.stash_list");
    app.on_key(key('r'));
    assert!(app.chooser.stash().is_none());
    let state = app.review.as_ref().expect("the stash opens in review");
    assert_eq!(state.review.range, "stash@{0}");
    assert_eq!(state.review.files[0].path, "a.txt");
    assert!(app.editor.active_tab().unwrap().text().contains("+two"));
    fs::remove_dir_all(dir.parent().unwrap()).ok();
}

/// T665 leftovers: Stage All Review Hunks stages a whole stash, and the
/// review is re-diffed afterwards (the stash itself is unchanged, so the
/// buffer still shows it with its comments).
#[test]
fn review_stage_all_stages_a_whole_stash() {
    let dir = t634_repo("t665all");
    let mut app = app_at(&dir);
    app.refresh_git();
    fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
    fs::write(dir.join("b.txt"), "bee\n").unwrap();
    t665_run(&dir, &["add", "b.txt"]);
    app.run_action("git.stash");
    assert!(!t665_cached(&dir).contains("+two"));
    app.run_action("git.stash_list");
    app.on_key(key('r'));
    assert!(app.review.is_some());
    app.run_action("git.review_stage_all");
    let cached = t665_cached(&dir);
    assert!(cached.contains("+two"), "{cached}");
    assert!(
        app.review.is_some(),
        "the review stays open after re-diffing"
    );
    fs::remove_dir_all(dir.parent().unwrap()).ok();
}

/// T665 leftovers: a hunk that touches a line with no trailing newline keeps
/// git's marker, so staging it applies.
#[test]
fn review_stages_a_hunk_with_a_no_newline_marker() {
    let dir = t634_repo("t665nonl");
    let mut app = app_at(&dir);
    app.refresh_git();
    fs::write(dir.join("a.txt"), "one").unwrap();
    app.open_review_from_diff("work", &t665_diff(&dir));
    assert!(
        app.editor
            .active_tab()
            .unwrap()
            .text()
            .contains("No newline")
    );
    let hunk_row = app.review.as_ref().unwrap().rendered.hunk_rows()[0];
    app.editor.goto(hunk_row + 2, None, app.layout.editor);
    app.run_action("git.review_stage_hunk");
    assert!(t665_cached(&dir).contains("No newline"));
    fs::remove_dir_all(dir.parent().unwrap()).ok();
}
