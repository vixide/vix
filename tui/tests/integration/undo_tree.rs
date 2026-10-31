//! Edit → Undo Tree… (T638): the panel previews a state as the selection
//! moves, `Enter` keeps it, `Esc` puts everything back; shared buffers
//! refuse; previews never reach auto-save or hot-exit backups.
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use crate::integration::common::*;

fn text(app: &App) -> String {
    app.editor.active_tab().unwrap().text()
}

fn current_state(app: &App) -> usize {
    app.editor
        .active_tab()
        .unwrap()
        .editor
        .code_ref()
        .history()
        .current()
}

/// A tree with a branch: "" -> "a" (1) -> "ab" (2), then back to "a" and
/// "ac" (3). The buffer ends at state 3.
fn branched(app: &mut App) {
    let tab = app.editor.active_tab_mut().unwrap();
    tab.editor.set_content("a");
    tab.editor.set_content("ab");
    app.run_action("edit.undo");
    let tab = app.editor.active_tab_mut().unwrap();
    tab.editor.set_content("ac");
    tab.dirty = false;
}

#[test]
fn moving_the_selection_previews_and_enter_keeps_the_state() {
    let mut app = app_at(Path::new("."));
    branched(&mut app);
    assert_eq!(text(&app), "ac");
    app.run_action("edit.undo_tree");
    let panel = &app.undo_tree.as_ref().expect("panel opened").panel;
    assert_eq!(panel.selected_seq(), Some(3), "opens on the current state");
    let graphs: Vec<&str> = panel.rows.iter().map(|r| r.graph.as_str()).collect();
    assert_eq!(graphs, ["o", "o", "|\\", "o |", "  @"]);

    app.on_key(keycode(KeyCode::Up));
    assert_eq!(text(&app), "ab", "the other branch, live");
    app.on_key(keycode(KeyCode::Up));
    assert_eq!(text(&app), "a");
    app.on_key(keycode(KeyCode::Home));
    assert_eq!(text(&app), "", "the original state");
    app.on_key(keycode(KeyCode::Down));
    app.on_key(keycode(KeyCode::Down));
    assert_eq!(text(&app), "ab");
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.undo_tree.is_none());
    assert_eq!(text(&app), "ab");
    assert_eq!(current_state(&app), 2, "history moved with the text");
    assert!(app.editor.active_tab().unwrap().dirty);
    // Undo and redo now work from the kept state.
    app.run_action("edit.undo");
    assert_eq!(text(&app), "a");
    app.run_action("edit.redo");
    assert_eq!(text(&app), "ab", "redo follows the branch we came down");
}

#[test]
fn esc_restores_the_text_cursor_flag_and_branches() {
    let mut app = app_at(Path::new("."));
    branched(&mut app);
    app.editor.active_tab_mut().unwrap().editor.set_cursor(1);
    app.run_action("edit.undo_tree");
    app.on_key(keycode(KeyCode::Up));
    app.on_key(keycode(KeyCode::Home));
    assert_eq!(text(&app), "");
    app.on_key(esc());
    assert!(app.undo_tree.is_none());
    assert_eq!(text(&app), "ac");
    assert_eq!(current_state(&app), 3);
    let tab = app.editor.active_tab().unwrap();
    assert_eq!(tab.editor.get_cursor(), 1);
    assert!(!tab.dirty, "the modified flag is back too");
    // The redo pointer at the branch point still names "ac".
    app.run_action("edit.undo");
    app.run_action("edit.redo");
    assert_eq!(text(&app), "ac");
}

#[test]
fn a_click_selects_and_previews_a_state() {
    let mut app = app_at(Path::new("."));
    branched(&mut app);
    app.run_action("edit.undo_tree");
    app.layout.undo_tree = Rect::new(10, 5, 30, 10);
    app.on_mouse(click(12, 5 + 3)); // row 3: state 2
    assert_eq!(text(&app), "ab");
    app.on_mouse(click(12, 5 + 2)); // the connector row: ignored
    assert_eq!(text(&app), "ab");
}

#[test]
fn a_shared_buffer_refuses_the_undo_tree() {
    let mut app = app_at(Path::new("."));
    branched(&mut app);
    app.editor.active_tab_mut().unwrap().editor.start_edit_log();
    app.run_action("edit.undo_tree");
    assert!(app.undo_tree.is_none(), "local undo is off while shared");
    assert_ne!(app.status, "", "the status bar says why");
    assert_eq!(text(&app), "ac");
}

#[test]
fn no_history_means_no_panel() {
    let mut app = app_at(Path::new("."));
    app.run_action("edit.undo_tree");
    assert!(app.undo_tree.is_none());
    assert_ne!(app.status, "");
}

#[test]
fn previews_are_never_backed_up_and_quitting_restores() {
    let dir = unique_dir("undo-tree-hot-exit");
    let _ = fs::remove_dir_all(dir.join("backup"));
    let file = dir.join("notes.txt");
    fs::write(&file, "one\n").unwrap();
    let mut app = app_at(&dir)
        .with_backup_dir(dir.join("backup"))
        .with_settings_path(dir.join("config.toml"));
    app.open_initial(&file);
    let tab = app.editor.active_tab_mut().unwrap();
    tab.editor.set_content("two\n");
    tab.editor.set_content("three\n");
    tab.dirty = true;
    app.run_action("edit.undo_tree");
    app.on_key(keycode(KeyCode::Up));
    assert_eq!(text(&app), "two\n");
    app.tick();
    assert_eq!(
        fs::read_dir(dir.join("backup")).map_or(0, Iterator::count),
        0,
        "no backup of a previewed state"
    );
    app.on_exit();
    assert!(app.undo_tree.is_none());
    assert_eq!(text(&app), "three\n", "quitting puts the real text back");
    assert_eq!(fs::read_to_string(&file).unwrap(), "one\n");
}
