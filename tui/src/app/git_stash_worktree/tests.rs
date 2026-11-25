//! Behavior tests for the stash and worktree choosers: navigation, mouse
//! clicks, review/apply/pop/drop and their failure reporting, and the
//! worktree add/switch/remove guards. Every test builds a throwaway repository
//! through the `vix_git::run` helper and drives the real `App`.

use super::*;
use crate::app::git::tests::{app_at as base_app_at, ch, click, git, key, repo, temp};
use ratatui::layout::Rect;

/// An app rooted at `d` that persists its settings inside `d`, never in the
/// user's real configuration directory.
fn app_at(d: &Path) -> App {
    let mut app = base_app_at(d);
    app.settings_path = Some(d.join("vix-test-config.toml"));
    app
}

/// A repo whose `a.txt` was stashed twice (`stash@{1}` adds "two",
/// `stash@{0}` adds "three"), with a clean tree afterwards.
fn repo_with_two_stashes(tag: &str) -> PathBuf {
    let d = repo(tag, &[("a.txt", "one\n")]);
    std::fs::write(d.join("a.txt"), "one\ntwo\n").unwrap();
    git(&d, &["stash", "push", "-q"]);
    std::fs::write(d.join("a.txt"), "one\nthree\n").unwrap();
    git(&d, &["stash", "push", "-q"]);
    d
}

/// The text of the newest message.
fn last_error(app: &App) -> String {
    app.messages.items.last().map_or_default(|m| m.text.clone())
}

#[test]
fn listing_stashes_without_any_says_so_and_outside_a_repo_does_nothing() {
    let d = repo("stash-none", &[("a.txt", "one\n")]);
    let mut app = app_at(&d);
    assert!(app.run_git_stash_worktree_action("git.stash_list"));
    assert!(app.chooser.stash().is_none());
    assert_eq!(app.status, t!("status.git_no_stashes").to_string());

    let plain = temp("stash-norepo");
    let mut app = app_at(&plain);
    app.run_git_stash_worktree_action("git.stash_list");
    assert!(app.chooser.stash().is_none());
    assert_eq!(app.status, t!("status.git_not_repo").to_string());
    // Anything else is not this module's action.
    assert!(!app.run_git_stash_worktree_action("git.commit"));
}

#[test]
fn the_stash_chooser_moves_clamps_ignores_unknown_keys_and_closes() {
    let d = repo_with_two_stashes("stash-nav");
    let mut app = app_at(&d);
    app.run_git_stash_worktree_action("git.stash_list");
    assert_eq!(app.chooser.stash().unwrap().entries.len(), 2);
    assert_eq!(app.chooser.stash().unwrap().selected, 0);
    app.on_key(key(KeyCode::Up));
    assert_eq!(app.chooser.stash().unwrap().selected, 0);
    app.on_key(key(KeyCode::Down));
    app.on_key(key(KeyCode::Down));
    assert_eq!(app.chooser.stash().unwrap().selected, 1);
    app.on_key(ch('x'));
    assert_eq!(app.chooser.stash().unwrap().selected, 1);
    app.on_key(key(KeyCode::Up));
    assert_eq!(app.chooser.stash().unwrap().selected, 0);
    app.on_key(key(KeyCode::Esc));
    assert!(app.chooser.stash().is_none());
}

#[test]
fn clicking_a_stash_row_shows_its_diff_and_clicks_elsewhere_do_nothing() {
    let d = repo_with_two_stashes("stash-click");
    let mut app = app_at(&d);
    app.run_git_stash_worktree_action("git.stash_list");
    app.layout.chooser = Rect::new(0, 0, 60, 10);
    // Below the last entry: nothing happens.
    app.on_mouse(click(3, 5));
    assert!(app.chooser.stash().is_some());
    // Outside the list rectangle: nothing happens.
    app.on_mouse(click(70, 1));
    assert!(app.chooser.stash().is_some());
    // The second row opens stash@{1}'s diff in a read-only tab.
    app.on_mouse(click(3, 1));
    assert!(app.chooser.stash().is_none());
    let tab = app.editor.active_tab().unwrap();
    assert_eq!(tab.path.as_deref(), Some(Path::new("stash@{1}")));
    assert!(tab.text().contains("+two"));
}

#[test]
fn r_reviews_the_highlighted_stash() {
    let d = repo_with_two_stashes("stash-review");
    let mut app = app_at(&d);
    app.run_git_stash_worktree_action("git.stash_list");
    app.on_key(ch('r'));
    assert!(app.chooser.stash().is_none());
    assert!(app.review.is_some(), "{}", app.status);
}

#[test]
fn applying_a_stash_onto_conflicting_changes_reports_git_and_keeps_the_chooser() {
    let d = repo_with_two_stashes("stash-conflict");
    let mut app = app_at(&d);
    app.run_git_stash_worktree_action("git.stash_list");
    // A local edit to the same line makes both apply and pop refuse.
    std::fs::write(d.join("a.txt"), "local edit\n").unwrap();
    let before = app.messages.items.len();
    app.on_key(ch('a'));
    assert_eq!(app.messages.items.len(), before + 1);
    assert!(last_error(&app).starts_with(t!("msg.git_failed_reason", error = "").trim_end()));
    assert!(app.chooser.stash().is_some());
    app.on_key(ch('p'));
    assert_eq!(app.messages.items.len(), before + 2);
    assert_eq!(
        app.chooser.stash().unwrap().entries.len(),
        2,
        "nothing dropped"
    );
}

#[test]
fn popping_reselects_near_the_same_row() {
    let d = repo_with_two_stashes("stash-pop");
    let mut app = app_at(&d);
    app.run_git_stash_worktree_action("git.stash_list");
    app.on_key(ch('p'));
    assert_eq!(
        app.status,
        t!("status.git_stash_popped_ref", stash = "stash@{0}").to_string()
    );
    assert_eq!(app.chooser.stash().unwrap().entries.len(), 1);
    assert_eq!(
        std::fs::read_to_string(d.join("a.txt")).unwrap(),
        "one\nthree\n"
    );
}

#[test]
fn dropping_a_stash_asks_first_and_failures_are_reported() {
    let d = repo_with_two_stashes("stash-drop");
    let mut app = app_at(&d);
    app.run_git_stash_worktree_action("git.stash_list");
    app.on_key(key(KeyCode::Delete));
    let confirm = app.confirm.take().unwrap();
    assert_eq!(confirm.kind, ConfirmKind::GitStashDrop);
    assert_eq!(confirm.paths, vec![PathBuf::from("stash@{0}")]);
    assert!(app.chooser.stash().is_none());

    // A name that is not `stash@{N}` is ignored entirely.
    app.git_stash_drop_answered(Path::new("nonsense"), true);
    assert!(app.chooser.stash().is_none());
    // Declining keeps both stashes and reopens the list.
    app.git_stash_drop_answered(Path::new("stash@{0}"), false);
    assert_eq!(app.chooser.stash().unwrap().entries.len(), 2);
    // Dropping a stash that does not exist reports git's refusal.
    let before = app.messages.items.len();
    app.git_stash_drop_answered(Path::new("stash@{9}"), true);
    assert_eq!(app.messages.items.len(), before + 1);
    // A real drop says so, and the list shrinks.
    app.git_stash_drop_answered(Path::new("stash@{0}"), true);
    assert_eq!(
        app.status,
        t!("status.git_stash_dropped", stash = "stash@{0}").to_string()
    );
    assert_eq!(app.chooser.stash().unwrap().entries.len(), 1);
}

#[test]
fn worktree_add_needs_a_path_and_reports_git_failures() {
    let d = repo("wt-add", &[("a.txt", "one\n")]);
    let mut app = app_at(&d);
    app.run_git_stash_worktree_action("git.worktree_add");
    assert!(app.prompt.is_some());
    // Empty input does nothing.
    let status = app.status.clone();
    app.git_worktree_add("");
    assert_eq!(app.status, status);
    // A path git cannot create (inside a missing, unwritable root) is reported.
    let before = app.messages.items.len();
    app.git_worktree_add("a.txt");
    assert_eq!(app.messages.items.len(), before + 1, "{}", app.status);
    // A good path succeeds and says so.
    app.git_worktree_add("linked");
    assert_eq!(
        app.status,
        t!("status.git_worktree_added", path = "linked").to_string()
    );
    assert!(d.join("linked").join("a.txt").is_file());

    // Outside a repository nothing opens.
    let plain = temp("wt-norepo");
    let mut app = app_at(&plain);
    app.run_git_stash_worktree_action("git.worktree_add");
    assert!(app.prompt.is_none());
    app.run_git_stash_worktree_action("git.worktree_list");
    assert!(app.chooser.worktree().is_none());
}

#[test]
fn the_worktree_chooser_guards_the_current_and_main_trees() {
    let d = repo("wt-guards", &[("a.txt", "one\n")]);
    let mut app = app_at(&d);
    app.git_worktree_add("linked");
    app.run_git_stash_worktree_action("git.worktree_list");
    let c = app.chooser.worktree().unwrap();
    assert_eq!(c.worktrees.len(), 2);
    assert!(!c.remove);
    // Enter on the current worktree only says so.
    app.on_key(key(KeyCode::Enter));
    assert_eq!(app.status, t!("status.git_worktree_current").to_string());
    assert!(app.chooser.worktree().is_some());
    // Removing the main worktree is refused.
    app.on_key(ch('d'));
    assert_eq!(app.status, t!("status.git_worktree_main").to_string());
    // Down/Up clamp at the ends; unknown keys do nothing.
    app.on_key(key(KeyCode::Down));
    app.on_key(key(KeyCode::Down));
    assert_eq!(app.chooser.worktree().unwrap().selected, 1);
    app.on_key(ch('z'));
    app.on_key(key(KeyCode::Up));
    app.on_key(key(KeyCode::Up));
    assert_eq!(app.chooser.worktree().unwrap().selected, 0);
    // `a` starts adding one from list mode and closes the chooser.
    app.on_key(ch('a'));
    assert!(app.prompt.is_some());
    assert!(app.chooser.worktree().is_none());
    app.prompt = None;

    // In remove mode `a` is ignored and Esc closes.
    app.run_git_stash_worktree_action("git.worktree_remove");
    assert!(app.chooser.worktree().unwrap().remove);
    app.on_key(ch('a'));
    assert!(app.prompt.is_none());
    assert!(app.chooser.worktree().is_some());
    app.on_key(key(KeyCode::Esc));
    assert!(app.chooser.worktree().is_none());
}

#[test]
fn removing_a_worktree_confirms_and_reports_git_refusals() {
    let d = repo("wt-remove", &[("a.txt", "one\n")]);
    let mut app = app_at(&d);
    app.git_worktree_add("linked");
    let linked = d.join("linked");
    app.run_git_stash_worktree_action("git.worktree_remove");
    app.layout.chooser = Rect::new(0, 0, 60, 10);
    // Clicking the linked worktree's row asks first.
    app.on_mouse(click(3, 1));
    let confirm = app.confirm.take().unwrap();
    assert_eq!(confirm.kind, ConfirmKind::GitWorktreeRemove);
    assert_eq!(confirm.paths.len(), 1);
    assert!(app.chooser.worktree().is_none());

    // Declined: the tree stays and the chooser comes back in list mode.
    app.git_worktree_remove_answered(&linked, false);
    assert!(linked.is_dir());
    assert!(!app.chooser.worktree().unwrap().remove);
    // git refuses a dirty tree; the refusal is reported and nothing is removed.
    std::fs::write(linked.join("untracked.txt"), "x").unwrap();
    let before = app.messages.items.len();
    app.git_worktree_remove_answered(&linked, true);
    assert_eq!(app.messages.items.len(), before + 1);
    assert!(linked.is_dir());
    // Clean again: removed, and the status says so.
    std::fs::remove_file(linked.join("untracked.txt")).unwrap();
    app.git_worktree_remove_answered(&linked, true);
    assert_eq!(
        app.status,
        t!("status.git_worktree_removed", path = linked.display()).to_string()
    );
    assert!(!linked.exists());
}

#[test]
fn clicking_a_worktree_in_list_mode_switches_the_workspace_to_it() {
    let d = repo("wt-switch", &[("a.txt", "one\n")]);
    let mut app = app_at(&d);
    app.git_worktree_add("linked");
    app.run_git_stash_worktree_action("git.worktree_list");
    app.layout.chooser = Rect::new(0, 0, 60, 10);
    app.on_mouse(click(3, 1));
    assert!(app.chooser.worktree().is_none());
    assert_eq!(
        app.root.canonicalize().unwrap(),
        d.join("linked").canonicalize().unwrap()
    );
}
