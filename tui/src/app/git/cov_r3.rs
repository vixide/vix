//! Coverage-round tests for the `git.*` glue: actions with no open file,
//! the Changes and log panels' keys and mouse, the change-gutter bases,
//! inline blame, the branch chooser's wrap-around and the confirm keys.

use super::tests::{
    app_at as base_app_at, app_with as base_app_with, ch, click, git, key, repo, temp,
};
use super::*;
use ratatui::layout::Rect;
use vix_input::{KeyCode, KeyEvent, KeyModifiers};

/// An app rooted at `d` that persists its settings inside `d`, never in the
/// user's real configuration directory.
fn app_at(d: &Path) -> App {
    let mut app = base_app_at(d);
    app.settings_path = Some(d.join("vix-test-config.toml"));
    app
}

/// [`base_app_with`] with the same settings isolation.
fn app_with(tag: &str, files: &[(&str, &str)]) -> (PathBuf, App) {
    let (d, mut app) = base_app_with(tag, files);
    app.settings_path = Some(d.join("vix-test-config.toml"));
    (d, app)
}

/// A repo with two commits (`a.txt` then `b.txt`) and the app on `a.txt`.
fn two_commits(tag: &str) -> (PathBuf, App) {
    let (d, mut app) = app_with(tag, &[("a.txt", "one\n")]);
    std::fs::write(d.join("b.txt"), "bee\n").unwrap();
    git(&d, &["add", "-A"]);
    git(&d, &["commit", "-q", "-m", "second"]);
    app.refresh_git();
    (d, app)
}

#[test]
fn file_actions_with_no_open_tab_report_or_do_nothing() {
    let (d, mut app) = app_with("g-notab", &[("a.txt", "one\n")]);
    app.editor.tabs.clear();
    for action in ["git.blame", "git.diff_head", "git.diff_index"] {
        app.status.clear();
        assert!(app.run_git_action(action));
        assert_eq!(
            app.status,
            t!("status.blame_no_file").to_string(),
            "{action}"
        );
    }
    app.status.clear();
    app.run_git_action("git.diff_next");
    assert_eq!(app.status, t!("status.no_changes").to_string());
    for action in [
        "git.file_history",
        "git.revert_hunk",
        "git.stage_hunk",
        "git.unstage_hunk",
        "git.conflict_ours",
        "git.conflict_theirs",
        "git.conflict_both",
        "git.conflict_next",
        "git.conflict_list",
        "git.blame_inline",
        "git.open_at_revision",
    ] {
        assert!(app.run_git_action(action), "{action}");
    }
    assert!(app.git_log.is_none() && app.conflict_list.is_none());
    assert!(app.diff_view.is_none());
    // A revision typed with no file open does nothing; so does an empty one.
    app.git_open_at_revision("HEAD");
    app.git_open_at_revision("");
    assert!(app.editor.tabs.is_empty());
    assert!(d.join("a.txt").is_file());
}

#[test]
fn an_unsaved_buffer_has_nothing_to_blame_or_show_at_a_revision() {
    let (_d, mut app) = app_with("g-untitled", &[("a.txt", "one\n")]);
    app.editor.tabs.clear();
    app.editor.new_tab();
    app.status.clear();
    app.run_git_action("git.blame");
    assert_eq!(app.status, t!("status.blame_no_file").to_string());
    app.run_git_action("git.file_history");
    assert!(app.git_log.is_none());
    app.refresh_git_gutter();
    app.refresh_inline_blame();
}

#[test]
fn browsing_an_empty_repository_says_there_are_no_commits() {
    let d = temp("g-empty");
    git(&d, &["init", "-q", "-b", "main"]);
    let mut app = app_at(&d);
    app.run_git_action("git.browse_log");
    assert_eq!(app.status, t!("status.git_no_commits").to_string());
    assert!(app.git_log.is_none());
}

#[test]
fn file_history_of_an_untracked_file_is_empty() {
    let (d, mut app) = two_commits("g-hist");
    std::fs::write(d.join("new.txt"), "fresh\n").unwrap();
    app.open_initial(&d.join("new.txt"));
    app.run_git_action("git.file_history");
    assert_eq!(app.status, t!("status.git_no_file_history").to_string());
    assert!(app.git_log.is_none());
}

#[test]
fn the_commit_list_moves_by_row_and_page_and_clicks_open_a_diff() {
    let (_d, mut app) = two_commits("g-log");
    app.run_git_action("git.browse_log");
    assert_eq!(app.git_log.as_ref().unwrap().entries.len(), 2);
    app.on_key(key(KeyCode::Down));
    assert_eq!(app.git_log.as_ref().unwrap().selected, 1);
    app.on_key(key(KeyCode::Up));
    assert_eq!(app.git_log.as_ref().unwrap().selected, 0);
    app.on_key(key(KeyCode::PageDown));
    assert_eq!(app.git_log.as_ref().unwrap().selected, 1);
    app.on_key(key(KeyCode::PageUp));
    assert_eq!(app.git_log.as_ref().unwrap().selected, 0);
    // A click below the last commit is ignored; one on a row opens its diff.
    app.layout.git_log = Rect::new(0, 0, 80, 10);
    app.on_mouse(click(3, 5));
    assert!(app.git_log.is_some());
    app.on_mouse(click(3, 1));
    let tab = app.editor.active_tab().unwrap();
    assert!(tab.read_only);
    assert!(tab.text().contains("+one"), "{}", tab.text());
}

#[test]
fn opening_a_file_at_a_revision_names_the_tab_by_its_short_hash() {
    let (d, mut app) = two_commits("g-at-rev");
    app.open_initial(&d.join("a.txt"));
    app.git_open_at_revision("HEAD");
    let tab = app.editor.active_tab().unwrap();
    assert!(tab.read_only);
    let short = crate::git::resolve_short_sha(&d, "HEAD").unwrap();
    assert_eq!(
        tab.path.as_deref(),
        Some(Path::new(&format!("a.txt @ {short}")))
    );
    assert_eq!(tab.text(), "one\n");
    // A revision that does not exist reports it and opens nothing.
    let tabs = app.editor.tabs.len();
    let before = app.messages.items.len();
    app.git_open_at_revision("no-such-rev");
    assert_eq!(app.messages.items.len(), before + 1);
    assert_eq!(app.editor.tabs.len(), tabs);
}

#[test]
fn the_changes_panel_selects_stages_discards_and_clicks() {
    let (d, mut app) = app_with("g-panel", &[("a.txt", "one\n"), ("b.txt", "bee\n")]);
    std::fs::write(d.join("a.txt"), "one\nmore\n").unwrap();
    std::fs::write(d.join("b.txt"), "bee\nbuzz\n").unwrap();
    app.run_git_action("git.changes");
    assert_eq!(app.git_status.len(), 2);
    app.on_key(key(KeyCode::Up));
    assert_eq!(app.git_panel.as_ref().unwrap().selected, 0);
    app.on_key(key(KeyCode::Down));
    app.on_key(key(KeyCode::Down));
    assert_eq!(app.git_panel.as_ref().unwrap().selected, 1);
    // `s` stages the second file, `u` takes it back out.
    app.on_key(ch('s'));
    assert!(app.git_status[1].is_staged());
    app.on_key(ch('u'));
    assert!(!app.git_status[1].is_staged());
    // Space toggles; `r` refreshes and keeps the selection inside the list.
    app.on_key(ch(' '));
    assert!(app.git_status[1].is_staged());
    app.on_key(ch('r'));
    assert_eq!(app.git_panel.as_ref().unwrap().selected, 1);
    // Enter opens the diff against HEAD.
    app.on_key(key(KeyCode::Enter));
    assert!(app.diff_view.is_some());
    app.diff_view = None;
    // A click on the first row selects it and toggles its staging.
    app.layout.git_panel = Rect::new(0, 0, 60, 10);
    app.on_mouse(click(2, 0));
    assert_eq!(app.git_panel.as_ref().unwrap().selected, 0);
    assert!(app.git_status[0].is_staged());
    // A click past the last row does nothing.
    app.on_mouse(click(2, 5));
    assert_eq!(app.git_panel.as_ref().unwrap().selected, 0);
    // `d` asks before discarding; any other key leaves the question open.
    app.on_key(ch('d'));
    let kind = app.confirm.as_ref().map(|c| c.kind);
    if let Some(kind) = kind {
        app.git_confirm_key(kind, ch('?'));
        assert!(app.confirm.is_some());
        app.git_confirm_key(kind, key(KeyCode::Esc));
        assert!(app.confirm.is_none());
    }
    app.on_key(key(KeyCode::Esc));
    assert!(app.git_panel.is_none());
}

#[test]
fn the_change_gutter_follows_the_chosen_base_and_the_cache() {
    let (d, mut app) = app_with("g-gutter", &[("a.txt", "one\ntwo\n")]);
    app.on_key(ch('X'));
    app.refresh_git_gutter();
    let head_marks = app
        .editor
        .active_tab()
        .unwrap()
        .editor
        .gutter_marks()
        .cloned();
    assert!(head_marks.is_some_and(|m| !m.is_empty()));
    // Staging the edit and comparing to the index leaves nothing marked.
    app.run_action("file.save");
    git(&d, &["add", "a.txt"]);
    app.set_gutter_base("index");
    assert_eq!(
        app.status,
        t!("status.gutter_base_set", base = "index").to_string()
    );
    app.refresh_git_gutter();
    let marks = app
        .editor
        .active_tab()
        .unwrap()
        .editor
        .gutter_marks()
        .cloned();
    assert!(marks.is_none_or(|m| m.is_empty()));
    // The disk base is cached per path until the next refresh.
    app.set_gutter_base("disk");
    app.refresh_git_gutter();
    assert!(app.git_gutter_base_cache.contains_key(&d.join("a.txt")));
    // A branch base compares against that branch's version of the file.
    git(&d, &["branch", "side"]);
    app.set_gutter_base("branch:side");
    app.refresh_git_gutter();
    assert!(app.git_gutter_base_cache.contains_key(&d.join("a.txt")));
    // Outside a repository the gutter is cleared.
    app.flags.set(AppFlags::GIT_REPO, false);
    app.refresh_git_gutter();
    let marks = app
        .editor
        .active_tab()
        .unwrap()
        .editor
        .gutter_marks()
        .cloned();
    assert!(marks.is_none_or(|m| m.is_empty()));
}

#[test]
fn the_gutter_base_chooser_lists_the_fixed_bases_then_the_branches() {
    let (d, mut app) = app_with("g-gbase", &[("a.txt", "one\n")]);
    git(&d, &["branch", "topic"]);
    app.run_git_action("git.gutter_base");
    let c = app.chooser.branch().unwrap();
    assert_eq!(&c.branches[..3], ["head", "index", "disk"]);
    assert!(c.branches.contains(&"branch:topic".to_string()));
    assert_eq!(c.selected, 0);
    // The list wraps both ways.
    app.on_key(key(KeyCode::Up));
    let last = app.chooser.branch().unwrap().branches.len() - 1;
    assert_eq!(app.chooser.branch().unwrap().selected, last);
    app.on_key(key(KeyCode::Down));
    assert_eq!(app.chooser.branch().unwrap().selected, 0);
    app.on_key(key(KeyCode::Down));
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.settings.git_gutter_base, "index");
    // With no chooser open, activating does nothing.
    app.checkout_selected_branch();
    let plain = temp("g-gbase-norepo");
    let mut app = app_at(&plain);
    app.run_git_action("git.gutter_base");
    assert!(app.chooser.branch().is_none());
}

#[test]
fn inline_blame_follows_the_cursor_and_turns_off_cleanly() {
    let (d, mut app) = app_with("g-blame", &[("a.txt", "one\ntwo\n")]);
    app.settings.subsystems.inline_blame = false;
    app.run_git_action("git.blame_inline");
    assert!(app.settings.subsystems.inline_blame);
    app.refresh_inline_blame();
    assert!(app.blame_cache.is_some());
    // A second refresh at the same spot is a no-op (cached).
    app.refresh_inline_blame();
    // An uncommitted file has no blame, so no note.
    std::fs::write(d.join("new.txt"), "x\n").unwrap();
    app.open_initial(&d.join("new.txt"));
    app.refresh_inline_blame();
    // Turning it off clears the cache and the note.
    app.run_git_action("git.blame_inline");
    assert!(!app.settings.subsystems.inline_blame);
    assert!(app.blame_cache.is_none());
    app.refresh_inline_blame();
}

#[test]
fn confirm_keys_for_git_questions_ignore_unrelated_keys() {
    let (_d, mut app) = app_with("g-confirm", &[("a.txt", "one\n")]);
    for kind in [
        ConfirmKind::GitReviewDiscard,
        ConfirmKind::GitStashDrop,
        ConfirmKind::GitWorktreeRemove,
    ] {
        app.confirm = Some(Confirm {
            message: String::new(),
            paths: Vec::new(),
            kind,
        });
        // An unrelated key leaves the question open.
        app.git_confirm_key(kind, ch('?'));
        assert!(app.confirm.is_some());
    }
    // An answer with no path attached just closes the question.
    app.confirm = Some(Confirm {
        message: String::new(),
        paths: Vec::new(),
        kind: ConfirmKind::GitStashDrop,
    });
    app.git_confirm_key(ConfirmKind::GitStashDrop, ch('y'));
    assert!(app.confirm.is_none());
    // A discard answered yes with no file attached does nothing.
    app.confirm = Some(Confirm {
        message: String::new(),
        paths: Vec::new(),
        kind: ConfirmKind::GitDiscard,
    });
    app.git_confirm_key(ConfirmKind::GitDiscard, ch('y'));
    assert!(app.confirm.is_none());
}

#[test]
fn a_missing_file_shows_only_what_head_had() {
    let (d, mut app) = app_with("g-diff-gone", &[("a.txt", "one\n"), ("b.txt", "bee\n")]);
    std::fs::remove_file(d.join("b.txt")).unwrap();
    app.git_show_diff("b.txt", DiffBase::Head);
    let view = app.diff_view.as_ref().unwrap();
    assert!(view.title.ends_with("b.txt"));
    assert_ne!(view.lines, []);
    // A file identical to its base reports it.
    app.diff_view = None;
    app.git_show_diff("a.txt", DiffBase::Head);
    assert!(app.diff_view.is_none());
    assert_eq!(app.status, t!("status.diff_identical").to_string());
}

#[test]
fn the_conflict_list_mouse_selects_and_jumps_and_o_resolves() {
    const CONFLICTED: &str = "top\n<<<<<<< ours\nA\n=======\nB\n>>>>>>> theirs\nmid\n<<<<<<< ours\nC\n=======\nD\n>>>>>>> theirs\nend\n";
    let (_d, mut app) = app_with("g-conflicts", &[("a.txt", "x\n")]);
    {
        let tab = app.editor.active_tab_mut().unwrap();
        tab.editor.set_content(CONFLICTED);
    }
    app.layout.conflict_list = Rect::new(0, 0, 60, 10);
    app.run_git_action("git.conflict_list");
    assert_eq!(app.conflict_list.as_ref().unwrap().entries.len(), 2);
    // `o` resolves the first in place; the list shrinks and stays open.
    app.on_key(ch('o'));
    assert_eq!(app.conflict_list.as_ref().unwrap().entries.len(), 1);
    // A click on the remaining row jumps there and closes the list.
    app.on_mouse(click(1, 0));
    assert!(app.conflict_list.is_none());
    // Resolving the last conflict from a reopened list closes it for good.
    app.run_git_action("git.conflict_list");
    app.on_key(ch('t'));
    assert!(app.conflict_list.is_none());
    // `resolve_*` and `jump_*` with no list do nothing.
    app.jump_to_conflict_list_selection();
    app.resolve_conflict_list_selection(crate::conflict_tool::Resolution::Both);
    let _ = (KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), repo);
}

#[test]
fn the_gutter_without_a_tab_or_after_a_cached_base_does_nothing_more() {
    let (d, mut app) = app_with("g-gutter2", &[("a.txt", "one\n")]);
    app.set_gutter_base("disk");
    app.refresh_git_gutter();
    let cached = app.git_gutter_base_cache.get(&d.join("a.txt")).cloned();
    assert_eq!(cached.as_deref(), Some("one\n"));
    // An edit changes the revision but not the cached disk base.
    app.on_key(ch('x'));
    app.refresh_git_gutter();
    assert_eq!(
        app.git_gutter_base_cache
            .get(&d.join("a.txt"))
            .map(String::as_str),
        Some("one\n")
    );
    let marks = app
        .editor
        .active_tab()
        .unwrap()
        .editor
        .gutter_marks()
        .cloned();
    assert!(marks.is_some_and(|m| !m.is_empty()));
    // No tab at all: nothing to mark, in or out of a repository.
    app.editor.tabs.clear();
    app.refresh_git_gutter();
    app.flags.set(AppFlags::GIT_REPO, false);
    app.refresh_git_gutter();
    assert!(app.editor.tabs.is_empty());
}

#[test]
fn an_empty_changes_panel_ignores_staging_keys_and_enter() {
    let (_d, mut app) = app_with("g-panel-empty", &[("a.txt", "one\n")]);
    app.run_git_action("git.changes");
    assert_eq!(app.git_status, []);
    for c in ['s', 'u', ' ', 'd'] {
        app.on_key(ch(c));
    }
    app.on_key(key(KeyCode::Enter));
    assert!(app.confirm.is_none());
    assert!(app.diff_view.is_none());
    // A discard question needs a selected changed file.
    app.git_panel = None;
    app.git_confirm_discard();
    assert!(app.confirm.is_none());
    app.git_panel = Some(GitPanel { selected: 99 });
    app.git_confirm_discard();
    assert!(app.confirm.is_none());
    app.clamp_git_selection();
    assert_eq!(app.git_panel.as_ref().unwrap().selected, 0);
}

#[test]
fn staging_that_git_refuses_is_reported() {
    let (d, mut app) = app_with("g-stage-fail", &[("a.txt", "one\n")]);
    std::fs::write(d.join("a.txt"), "one\nmore\n").unwrap();
    app.run_git_action("git.changes");
    // A leftover lock makes git refuse every index change.
    std::fs::write(d.join(".git/index.lock"), "").unwrap();
    let before = app.messages.items.len();
    app.on_key(ch('s'));
    assert_eq!(app.messages.items.len(), before + 1);
    app.on_key(ch('u'));
    assert_eq!(app.messages.items.len(), before + 2);
    std::fs::remove_file(d.join(".git/index.lock")).unwrap();
    app.on_key(ch('s'));
    assert!(app.git_status[0].is_staged());
}

#[test]
fn the_panel_selection_moves_up_and_the_branch_chooser_rejects_stale_picks() {
    let (d, mut app) = app_with("g-up", &[("a.txt", "one\n"), ("b.txt", "bee\n")]);
    std::fs::write(d.join("a.txt"), "one\nx\n").unwrap();
    std::fs::write(d.join("b.txt"), "bee\ny\n").unwrap();
    app.run_git_action("git.changes");
    app.on_key(key(KeyCode::Down));
    assert_eq!(app.git_panel.as_ref().unwrap().selected, 1);
    app.on_key(key(KeyCode::Up));
    assert_eq!(app.git_panel.as_ref().unwrap().selected, 0);
    app.git_panel = None;
    // A chooser whose highlight points past its rows checks nothing out.
    app.chooser.open_branch(BranchChooser {
        branches: vec!["main".to_string()],
        selected: 5,
        merge: false,
        gutter: false,
    });
    app.checkout_selected_branch();
    assert!(app.chooser.branch().is_none());
    assert_eq!(app.git_branch.as_deref(), Some("main"));
}

#[test]
fn closing_a_blame_note_clears_it_and_a_missing_file_has_no_blame() {
    let (d, mut app) = app_with("g-blame2", &[("a.txt", "one\n")]);
    app.settings.subsystems.inline_blame = true;
    app.refresh_inline_blame();
    assert_eq!(app.blame_cache, Some((d.join("a.txt"), 1)));
    // Moving to an untracked buffer is looked at too (and has no blame).
    std::fs::write(d.join("loose.txt"), "x\n").unwrap();
    app.open_initial(&d.join("loose.txt"));
    app.refresh_inline_blame();
    assert_eq!(app.blame_cache, Some((d.join("loose.txt"), 1)));
    // With no tab, the cache note is cleared without a panic.
    app.editor.tabs.clear();
    app.refresh_inline_blame();
    app.settings.subsystems.inline_blame = false;
    app.refresh_inline_blame();
}
