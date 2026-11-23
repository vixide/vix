//! Behavior tests for the commit-graph overlay, over real throwaway repos.

use super::*;
use crate::app::git::tests::{app_at, ch, click, git, key, last_error, repo, temp};
use ratatui::layout::Rect;
use std::path::PathBuf;
use vix_git_graph::Mode;

/// `main` holds `seed`; branch `feat` adds `second` (touching `f.txt`).
/// HEAD is back on `main`. The graph (all refs) lists `second` then `seed`.
fn two_commit_repo(tag: &str) -> PathBuf {
    let d = repo(tag, &[("a.txt", "x\n")]);
    git(&d, &["checkout", "-q", "-b", "feat"]);
    std::fs::write(d.join("f.txt"), "f\n").unwrap();
    git(&d, &["add", "f.txt"]);
    git(&d, &["commit", "-q", "-m", "second"]);
    git(&d, &["checkout", "-q", "main"]);
    d
}

fn open(tag: &str) -> (PathBuf, App) {
    let d = two_commit_repo(tag);
    let mut app = app_at(&d);
    app.layout.git_graph = Rect::new(0, 0, 80, 10);
    app.open_git_graph();
    (d, app)
}

fn panel(app: &App) -> &Panel {
    app.git_graph.as_ref().unwrap()
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn select_summary(app: &mut App, summary: &str) {
    let idx = panel(app)
        .commits
        .iter()
        .position(|c| c.subject == summary)
        .unwrap();
    app.git_graph.as_mut().unwrap().selected = idx;
}

#[test]
fn opening_outside_a_repo_or_in_an_empty_repo_only_reports() {
    let plain = temp("graph-plain");
    let mut app = app_at(&plain);
    app.open_git_graph();
    assert!(app.git_graph.is_none());
    assert_eq!(app.status, t!("status.git_not_repo").to_string());

    let empty = temp("graph-empty");
    git(&empty, &["init", "-q", "-b", "main"]);
    let mut app = app_at(&empty);
    app.open_git_graph();
    assert!(app.git_graph.is_none());
    assert_eq!(app.status, t!("status.git_no_commits").to_string());
}

#[test]
fn the_graph_lists_all_refs_and_filters_by_typing() {
    let (_d, mut app) = open("graph-open");
    assert_eq!(panel(&app).commits.len(), 2);
    assert!(panel(&app).exhausted);
    assert!(panel(&app).refs.contains(&"feat".to_string()));
    assert_eq!(panel(&app).selected_commit().unwrap().subject, "second");

    app.git_graph_key(key(KeyCode::Down));
    assert_eq!(panel(&app).selected, 1);
    app.git_graph_key(key(KeyCode::Up));
    app.git_graph_key(key(KeyCode::PageDown));
    assert_eq!(panel(&app).selected, 1);
    app.git_graph_key(key(KeyCode::PageUp));
    assert_eq!(panel(&app).selected, 0);

    for c in "seed".chars() {
        app.git_graph_key(ch(c));
    }
    assert_eq!(panel(&app).query, "seed");
    assert_eq!(panel(&app).selected_commit().unwrap().subject, "seed");
    app.git_graph_key(key(KeyCode::Backspace));
    assert_eq!(panel(&app).query, "see");
    app.git_graph_key(key(KeyCode::Home)); // ignored
    app.git_graph_key(key(KeyCode::Esc));
    assert!(app.git_graph.is_none());
    // No panel: keys do nothing.
    app.git_graph_key(ch('x'));
    assert!(app.git_graph.is_none());
}

#[test]
fn tab_cycles_the_ref_filter_and_reloads() {
    let (_d, mut app) = open("graph-tab");
    let refs = panel(&app).refs.clone();
    assert!(refs.len() >= 2, "{refs:?}");
    for r in &refs {
        app.git_graph_key(key(KeyCode::Tab));
        assert_eq!(panel(&app).reference.as_deref(), Some(r.as_str()));
        let expected = if r == "feat" { 2 } else { 1 };
        assert_eq!(panel(&app).commits.len(), expected, "commits under {r}");
    }
    // One more step returns to "all refs" and every commit.
    app.git_graph_key(key(KeyCode::Tab));
    assert_eq!(panel(&app).reference, None);
    assert_eq!(panel(&app).commits.len(), 2);
    // Shift-Tab steps backwards, to the last ref first.
    app.git_graph_key(key(KeyCode::BackTab));
    assert_eq!(panel(&app).reference.as_ref(), refs.last());
}

#[test]
fn enter_opens_the_commit_as_a_read_only_tab() {
    let (_d, mut app) = open("graph-enter");
    app.git_graph_key(key(KeyCode::Enter));
    assert!(app.git_graph.is_none());
    let tab = app.editor.active_tab().unwrap();
    assert!(tab.read_only);
    assert!(tab.title().ends_with(" graph"), "{}", tab.title());
    assert!(tab.text().contains("second"));
}

#[test]
fn a_commit_that_cannot_be_shown_reports_an_error_and_stays_open() {
    let (_d, mut app) = open("graph-bad-show");
    app.git_graph.as_mut().unwrap().commits[0].sha = "deadbeefdeadbeefdeadbeef".into();
    app.git_graph_key(key(KeyCode::Enter));
    assert!(app.git_graph.is_some());
    assert_ne!(last_error(&app), "");
}

#[test]
fn clicks_select_then_open() {
    let (_d, mut app) = open("graph-click");
    app.git_graph_mouse(click(100, 100)); // outside
    assert_eq!(panel(&app).selected, 0);
    app.git_graph_mouse(click(1, 5)); // below the last commit
    assert_eq!(panel(&app).selected, 0);
    app.git_graph_mouse(click(1, 1));
    assert_eq!(panel(&app).selected, 1);
    assert!(app.git_graph.is_some(), "first click only selects");
    app.git_graph_mouse(click(1, 1));
    assert!(app.git_graph.is_none(), "second click opens the diff");
    assert!(app.editor.active_tab().unwrap().text().contains("seed"));
    app.git_graph_mouse(click(1, 1)); // no panel: ignored
}

#[test]
fn copying_the_hash_reports_it() {
    let (_d, mut app) = open("graph-copy");
    let sha = panel(&app).selected_commit().unwrap().sha.clone();
    app.git_graph_key(ctrl('y'));
    assert_eq!(
        app.status,
        t!("status.git_graph_copied", sha = sha).to_string()
    );
    // With a tab open the copy goes through the editor instead.
    app.editor.new_tab_with_content("x\n");
    app.git_graph_key(ctrl('Y'));
    assert_eq!(
        app.status,
        t!("status.git_graph_copied", sha = sha).to_string()
    );
}

#[test]
fn checkout_asks_first_then_switches_branch() {
    let (d, mut app) = open("graph-checkout");
    select_summary(&mut app, "second");
    app.git_graph_key(ctrl('o'));
    assert!(matches!(panel(&app).mode, Mode::Confirm { .. }));
    app.git_graph_key(ch('n'));
    assert!(matches!(panel(&app).mode, Mode::Browse));
    assert_eq!(git(&d, &["branch", "--show-current"]).trim(), "main");

    app.git_graph_key(ctrl('O'));
    app.git_graph_key(ch('y'));
    assert!(matches!(panel(&app).mode, Mode::Browse));
    assert_eq!(git(&d, &["branch", "--show-current"]).trim(), "feat");
    assert_eq!(
        app.status,
        t!("status.git_switched", branch = "feat").to_string()
    );
    assert_eq!(app.git_branch.as_deref(), Some("feat"));
}

#[test]
fn checkout_of_an_unbranched_commit_detaches() {
    let d = repo("graph-detach", &[("a.txt", "x\n")]);
    git(&d, &["checkout", "-q", "--detach"]);
    std::fs::write(d.join("a.txt"), "y\n").unwrap();
    git(&d, &["commit", "-qam", "loose"]);
    let mut app = app_at(&d);
    app.layout.git_graph = Rect::new(0, 0, 80, 10);
    app.open_git_graph();
    // Move HEAD off `loose` first so selecting it is a real checkout.
    let loose = panel(&app)
        .commits
        .iter()
        .find(|c| c.subject == "loose")
        .unwrap()
        .clone();
    git(&d, &["checkout", "-q", "main"]);
    git(&d, &["update-ref", "refs/keep/loose", &loose.sha]);
    select_summary(&mut app, "loose");
    app.git_graph_key(ctrl('o'));
    app.git_graph_key(key(KeyCode::Enter));
    assert_eq!(
        app.status,
        t!("status.git_graph_detached", sha = loose.abbrev).to_string()
    );
    assert_eq!(git(&d, &["branch", "--show-current"]).trim(), "");
}

#[test]
fn a_failing_checkout_is_reported() {
    let (d, app) = open("graph-checkout-fail");
    // A dirty tracked file that differs on `feat`'s side blocks the switch.
    git(&d, &["checkout", "-q", "feat"]);
    std::fs::write(d.join("f.txt"), "local\n").unwrap();
    git(&d, &["commit", "-qam", "third on feat"]);
    git(&d, &["checkout", "-q", "main"]);
    std::fs::write(d.join("f.txt"), "untracked clash\n").unwrap();
    let mut app2 = app_at(&d);
    app2.layout.git_graph = Rect::new(0, 0, 80, 10);
    app2.open_git_graph();
    select_summary(&mut app2, "third on feat");
    app2.git_graph_key(ctrl('o'));
    app2.git_graph_key(ch('y'));
    assert!(!last_error(&app2).is_empty(), "{}", app2.status);
    drop(app);
}

#[test]
fn cherry_pick_applies_a_commit_onto_the_current_branch() {
    let (d, mut app) = open("graph-pick");
    select_summary(&mut app, "second");
    app.git_graph_key(ctrl('p'));
    assert!(matches!(
        panel(&app).mode,
        Mode::Confirm {
            action: Action::CherryPick,
            ..
        }
    ));
    app.git_graph_key(key(KeyCode::Enter));
    let abbrev = panel(&app)
        .commits
        .iter()
        .find(|c| c.subject == "second")
        .unwrap()
        .abbrev
        .clone();
    assert_eq!(
        app.status,
        t!("status.git_graph_picked", sha = abbrev).to_string()
    );
    assert!(d.join("f.txt").exists(), "the commit's file is now on main");
    assert_eq!(git(&d, &["log", "-1", "--format=%s"]).trim(), "second");
}

#[test]
fn a_failing_cherry_pick_reports_the_commit() {
    let (_d, mut app) = open("graph-pick-fail");
    // `seed` is already on `main`: picking it again yields an empty pick.
    select_summary(&mut app, "seed");
    app.git_graph_key(ctrl('p'));
    app.git_graph_key(ch('y'));
    assert_ne!(last_error(&app), "");
}

#[test]
fn new_branch_prompt_edits_creates_and_cancels() {
    let (d, mut app) = open("graph-branch");
    select_summary(&mut app, "second");
    app.git_graph_key(ctrl('b'));
    assert!(matches!(panel(&app).mode, Mode::NewBranch { .. }));
    // Empty name: reported, back to browse.
    app.git_graph_key(key(KeyCode::Enter));
    assert!(matches!(panel(&app).mode, Mode::Browse));
    assert_eq!(app.status, t!("status.git_empty_branch").to_string());

    app.git_graph_key(ctrl('b'));
    for c in "tx".chars() {
        app.git_graph_key(ch(c));
    }
    app.git_graph_key(key(KeyCode::Backspace));
    app.git_graph_key(ch('o'));
    app.git_graph_key(key(KeyCode::Tab)); // ignored while typing a name
    match &panel(&app).mode {
        Mode::NewBranch { name, .. } => assert_eq!(name, "to"),
        _ => panic!("still in the prompt"),
    }
    app.git_graph_key(key(KeyCode::Esc));
    assert!(matches!(panel(&app).mode, Mode::Browse));

    app.git_graph_key(ctrl('b'));
    for c in "newbr".chars() {
        app.git_graph_key(ch(c));
    }
    app.git_graph_key(key(KeyCode::Enter));
    assert_eq!(
        app.status,
        t!("status.git_switched", branch = "newbr").to_string()
    );
    assert!(git(&d, &["branch", "--list", "newbr"]).contains("newbr"));
    assert!(panel(&app).refs.contains(&"newbr".to_string()));

    // The same name again fails.
    app.git_graph_key(ctrl('b'));
    for c in "newbr".chars() {
        app.git_graph_key(ch(c));
    }
    app.git_graph_key(key(KeyCode::Enter));
    assert_ne!(last_error(&app), "");
}

#[test]
fn asking_with_no_visible_commit_does_nothing() {
    let (_d, mut app) = open("graph-ask-empty");
    for c in "zzzz-no-match".chars() {
        app.git_graph_key(ch(c));
    }
    assert!(panel(&app).selected_commit().is_none());
    app.git_graph_key(ctrl('o'));
    app.git_graph_key(ctrl('y'));
    app.git_graph_key(key(KeyCode::Enter));
    assert!(matches!(panel(&app).mode, Mode::Browse));
    assert!(app.git_graph.is_some());
    // Mode keys with no panel at all are no-ops too.
    app.git_graph = None;
    app.git_graph_ask(None);
    app.git_graph_checkout("abc");
    app.git_graph_confirm_key(ch('y'), Action::Checkout, "abc");
}

#[test]
fn pages_load_on_demand_and_failures_end_paging() {
    let (_d, mut app) = open("graph-more");
    app.git_graph.as_mut().unwrap().exhausted = false;
    app.git_graph_key(key(KeyCode::Down));
    assert!(panel(&app).exhausted, "a short page ends the history");
    assert_eq!(panel(&app).commits.len(), 2);

    // A broken repo: loading fails, paging stops and the error is shown.
    let broken = temp("graph-more-broken");
    app.git_graph.as_mut().unwrap().exhausted = false;
    app.root = broken;
    app.git_graph_key(key(KeyCode::Down));
    assert!(panel(&app).exhausted);
    assert_ne!(last_error(&app), "");
    // Reloading against it fails the same way.
    let n = app.messages.items.len();
    app.git_graph_key(key(KeyCode::Tab));
    assert_eq!(app.messages.items.len(), n + 1);
}

#[test]
fn error_text_has_an_install_hint_for_a_missing_git() {
    assert_eq!(
        error_text(&GraphError::NotInstalled),
        t!("msg.git_not_installed").to_string()
    );
    let other = GraphError::InvalidArgument("x".into());
    assert_eq!(error_text(&other), other.to_string());
}
