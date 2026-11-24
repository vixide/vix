//! Behaviour tests for Tools → Compare with File (overlay and key handling).

use super::*;
use crate::app::cov4::{app_at, buffer_with, code, key, last_message, temp};

#[test]
fn an_empty_path_does_nothing() {
    let dir = temp("difftool-empty");
    let mut app = app_at(&dir);
    let before = app.messages.items.len();
    let status = app.status.clone();
    app.open_diff_with("");
    assert!(app.diff_view.is_none());
    assert_eq!(app.messages.items.len(), before);
    assert_eq!(app.status, status);
}

#[test]
fn a_missing_file_is_reported_as_an_open_failure() {
    let dir = temp("difftool-missing");
    let mut app = app_at(&dir);
    app.open_diff_with("nope.txt");
    assert!(app.diff_view.is_none());
    let expected = t!("msg.open_failed", error = dir.join("nope.txt").display()).to_string();
    assert_eq!(last_message(&app), Some(expected));
}

#[test]
fn an_untitled_buffer_gets_the_unified_overlay_titled_with_both_names() {
    let dir = temp("difftool-unified");
    std::fs::write(dir.join("other.txt"), "one\ntwo\nthree\n").unwrap();
    let mut app = app_at(&dir);
    buffer_with(&mut app, "one\n2\nthree\n", 0);
    app.open_diff_with("other.txt");
    let view = app.diff_view.as_ref().expect("the overlay opens");
    assert_eq!(view.title, format!("other.txt ↔ {}", t!("ui.untitled")));
    assert_eq!(view.scroll, 0);
    assert_eq!(
        view.lines,
        crate::diff_view::build("one\ntwo\nthree\n", "one\n2\nthree\n")
    );
}

#[test]
fn identical_contents_report_it_in_the_status_and_open_no_overlay() {
    let dir = temp("difftool-identical");
    std::fs::write(dir.join("same.txt"), "alpha\n").unwrap();
    let mut app = app_at(&dir);
    buffer_with(&mut app, "alpha\n", 0);
    app.open_diff_with("same.txt");
    assert!(app.diff_view.is_none());
    assert_eq!(app.status, t!("status.diff_identical").to_string());
}

#[test]
fn a_file_backed_buffer_compares_side_by_side_instead_of_the_overlay() {
    let dir = temp("difftool-compare");
    let a = dir.join("a.txt");
    std::fs::write(&a, "x\n").unwrap();
    std::fs::write(dir.join("b.txt"), "y\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&a);
    app.open_diff_with("b.txt");
    assert!(
        app.diff_view.is_none(),
        "no overlay for a file-backed buffer"
    );
    assert!(app.compare.is_some(), "a side-by-side comparison starts");
    assert!(app.editor.split_root.is_some());
}

#[test]
fn the_diff_view_scrolls_clamps_and_closes() {
    let dir = temp("difftool-keys");
    let old = "old line\n".repeat(60);
    std::fs::write(dir.join("big.txt"), &old).unwrap();
    let mut app = app_at(&dir);
    buffer_with(&mut app, "entirely different\n", 0);
    app.open_diff_with("big.txt");
    let total = app.diff_view.as_ref().unwrap().lines.len();
    assert!(total > 24, "need more lines than a page, got {total}");
    let page = usize::from(app.layout.editor.height);
    let scroll = |app: &App| app.diff_view.as_ref().unwrap().scroll;

    app.diff_view_key(code(KeyCode::Up));
    assert_eq!(scroll(&app), 0, "cannot scroll above the top");
    app.diff_view_key(code(KeyCode::Down));
    assert_eq!(scroll(&app), 1);
    app.diff_view_key(code(KeyCode::PageDown));
    assert_eq!(scroll(&app), 1 + page);
    app.diff_view_key(code(KeyCode::PageUp));
    assert_eq!(scroll(&app), 1);
    app.diff_view_key(code(KeyCode::Char('x')));
    assert_eq!(scroll(&app), 1, "other keys are ignored");
    for _ in 0..10 {
        app.diff_view_key(code(KeyCode::PageDown));
    }
    assert_eq!(scroll(&app), total - 1, "clamped to the last line");
    app.diff_view_key(code(KeyCode::Down));
    assert_eq!(scroll(&app), total - 1);

    app.diff_view_key(key('q'));
    assert!(app.diff_view.is_none());
    app.open_diff_with("big.txt");
    app.diff_view_key(code(KeyCode::Esc));
    assert!(app.diff_view.is_none());
}
