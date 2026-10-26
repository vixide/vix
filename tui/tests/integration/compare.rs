//! Side-by-side Compare Files (T660): two editable tabs, aligned, with
//! next/previous difference and copy-change-across.
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use std::fs;
use std::path::{Path, PathBuf};

use crate::common::*;

/// A workspace with `old.txt` and `new.txt`, and an app comparing them.
fn comparing(tag: &str, old: &str, new: &str) -> (App, PathBuf) {
    let dir = unique_dir(tag);
    fs::write(dir.join("old.txt"), old).unwrap();
    fs::write(dir.join("new.txt"), new).unwrap();
    let mut app = app_at(&dir);
    app.open_compare_files(&dir.join("old.txt"), &dir.join("new.txt"));
    (app, dir)
}

fn tab_text(app: &App, name: &str) -> String {
    app.editor
        .tabs
        .iter()
        .find(|t| t.path.as_deref().and_then(Path::file_name) == Some(name.as_ref()))
        .map(vix::editor::Tab::text)
        .expect("tab is open")
}

fn active_name(app: &App) -> String {
    app.editor
        .active_tab()
        .and_then(|t| t.path.as_deref())
        .and_then(Path::file_name)
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

const OLD: &str = "a\nb\nc\nd\ne\n";
const NEW: &str = "a\nB\nc\nx\ny\nd\ne\n";

#[test]
fn opens_both_files_side_by_side_with_marks() {
    let (mut app, dir) = comparing("cmp-open", OLD, NEW);
    let root = app.editor.split_root.clone().expect("a split opened");
    assert_eq!(root.leaf_count(), 2);
    assert_eq!(active_name(&app), "old.txt", "left side focused");
    assert!(app.refresh_compare());
    let marks = app.editor.active_tab().unwrap().editor.gutter_marks();
    assert_eq!(
        marks.map(|m| m.iter().map(|&(l, _)| l).collect::<Vec<_>>()),
        Some(vec![1, 3]),
        "the changed line and the insertion point are marked on the left"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn next_and_previous_difference_move_the_cursor() {
    let (mut app, dir) = comparing("cmp-nav", OLD, NEW);
    app.run_action("diff.next");
    assert_eq!(app.editor.active_tab().unwrap().editor.cursor_line(), 1);
    app.run_action("diff.next");
    assert_eq!(app.editor.active_tab().unwrap().editor.cursor_line(), 3);
    app.run_action("diff.next");
    assert_eq!(
        app.editor.active_tab().unwrap().editor.cursor_line(),
        3,
        "no more differences: the cursor stays"
    );
    app.run_action("diff.prev");
    assert_eq!(app.editor.active_tab().unwrap().editor.cursor_line(), 1);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn copy_change_to_right_then_save_writes_that_file() {
    let (mut app, dir) = comparing("cmp-copy", OLD, NEW);
    app.run_action("diff.next"); // line 1: "b" vs "B"
    app.run_action("diff.copy_to_right");
    assert_eq!(tab_text(&app, "new.txt"), "a\nb\nc\nx\ny\nd\ne\n");
    assert_eq!(
        tab_text(&app, "old.txt"),
        OLD,
        "the source side is unchanged"
    );
    // Focus the right pane (the other leaf) and save it.
    app.run_action("view.focus_other_pane");
    assert_eq!(active_name(&app), "new.txt", "the right pane is focused");
    app.run_action("file.save");
    assert_eq!(
        fs::read_to_string(dir.join("new.txt")).unwrap(),
        "a\nb\nc\nx\ny\nd\ne\n"
    );
    // Copying the insertion back from the left deletes it on the right; the
    // right side's cursor is still on line 1, so the next change is at 3.
    app.run_action("diff.next");
    app.run_action("diff.copy_to_right");
    assert_eq!(tab_text(&app, "new.txt"), OLD);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn copy_change_to_left_is_one_undo_step() {
    let (mut app, dir) = comparing("cmp-undo", OLD, NEW);
    app.run_action("diff.next");
    app.run_action("diff.copy_to_left");
    assert_eq!(tab_text(&app, "old.txt"), "a\nB\nc\nd\ne\n");
    app.run_action("edit.undo");
    assert_eq!(tab_text(&app, "old.txt"), OLD);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_other_pane_follows_the_focused_scroll() {
    let many_old: String = (0..60).map(|i| format!("{i}\n")).collect();
    let many_new = format!("extra\nextra\n{many_old}");
    let (mut app, dir) = comparing("cmp-scroll", &many_old, &many_new);
    app.editor.active_tab_mut().unwrap().editor.set_offset_y(20);
    app.refresh_compare();
    let right = app
        .editor
        .tabs
        .iter()
        .find(|t| t.path.as_deref().and_then(Path::file_name) == Some("new.txt".as_ref()))
        .unwrap();
    assert_eq!(
        right.editor.get_offset_y(),
        22,
        "aligned past the 2-line insertion"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn close_comparison_keeps_both_tabs() {
    let (mut app, dir) = comparing("cmp-close", OLD, NEW);
    app.run_action("diff.close");
    assert!(app.editor.split_root.is_none());
    assert!(!app.refresh_compare());
    assert_eq!(tab_text(&app, "old.txt"), OLD, "both tabs stay open");
    assert_eq!(tab_text(&app, "new.txt"), NEW);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn closing_a_compared_tab_ends_the_comparison() {
    let (mut app, dir) = comparing("cmp-close-tab", OLD, NEW);
    app.run_action("file.close");
    assert!(!app.refresh_compare());
    app.run_action("diff.next");
    assert!(
        app.editor
            .active_tab()
            .is_none_or(|t| t.editor.cursor_line() == 0),
        "diff actions do nothing once the comparison has ended"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn unified_view_stays_available() {
    let (mut app, dir) = comparing("cmp-unified", OLD, NEW);
    app.run_action("diff.unified");
    let view = app.diff_view.as_ref().expect("unified overlay opened");
    assert!(view.title.contains("old.txt") && view.title.contains("new.txt"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn compare_with_file_on_a_saved_buffer_opens_side_by_side() {
    let dir = unique_dir("cmp-prompt");
    fs::write(dir.join("old.txt"), OLD).unwrap();
    fs::write(dir.join("new.txt"), NEW).unwrap();
    let mut app = app_at(&dir);
    app.editor.open(&dir.join("new.txt"), false).unwrap();
    app.run_action("tools.diff");
    for c in "old.txt".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.editor.split_root.is_some(), "side-by-side opened");
    assert!(app.diff_view.is_none(), "not the unified overlay");
    assert_eq!(
        active_name(&app),
        "old.txt",
        "the other file is the left side"
    );
    fs::remove_dir_all(&dir).ok();
}
