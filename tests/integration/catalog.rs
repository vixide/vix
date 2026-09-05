#![warn(clippy::pedantic)]
#![allow(clippy::cast_possible_truncation, clippy::format_collect)]
// Shared fixtures/helpers live in `common.rs`; every test here needs a
// handful of them, so a glob import earns its keep over a long explicit list.
#![allow(clippy::wildcard_imports)]

use crate::common::*;

// ---- Generated smoke tests for the action catalog (crates/vix-editor-core/spec/actions.tsv).
// Each runs the action on a small buffer and checks the app stays sane.

#[test]
fn catalog_cursor_up() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cursor_up");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cursor_down() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cursor_down");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cursor_page_up() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cursor_page_up");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cursor_page_down() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cursor_page_down");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cursor_left() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cursor_left");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cursor_right() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cursor_right");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cursor_start() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cursor_start");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cursor_end() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cursor_end");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cursor_to_view_top() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cursor_to_view_top");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cursor_to_view_center() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cursor_to_view_center");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cursor_to_view_bottom() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cursor_to_view_bottom");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_to_start() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_to_start");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_to_end() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_to_end");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_up() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_up");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_down() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_down");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_left() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_left");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_right() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_right");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_word_right() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("word_right");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_word_left() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("word_left");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_sub_word_right() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("sub_word_right");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_sub_word_left() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("sub_word_left");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_word_right() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_word_right");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_word_left() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_word_left");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_sub_word_right() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_sub_word_right");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_sub_word_left() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_sub_word_left");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_delete_word_right() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("delete_word_right");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_delete_word_left() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("delete_word_left");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_delete_sub_word_right() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("delete_sub_word_right");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_delete_sub_word_left() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("delete_sub_word_left");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_line");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_to_start_of_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_to_start_of_line");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_to_start_of_text() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_to_start_of_text");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_to_start_of_text_toggle() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_to_start_of_text_toggle");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_to_end_of_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_to_end_of_line");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_paragraph_previous() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("paragraph_previous");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_paragraph_next() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("paragraph_next");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_to_paragraph_previous() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_to_paragraph_previous");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_to_paragraph_next() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_to_paragraph_next");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_insert_newline() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("insert_newline");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_backspace() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("backspace");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_delete() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("delete");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_insert_tab() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("insert_tab");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_save() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("save");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_save_all() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("save_all");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_save_as() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("save_as");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_find() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("find");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_find_literal() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("find_literal");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_find_next() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("find_next");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_find_previous() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("find_previous");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_diff_next() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("diff_next");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_diff_previous() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("diff_previous");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_center() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("center");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_undo() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("undo");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_redo() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("redo");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_copy() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("copy");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_copy_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("copy_line");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cut() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cut");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cut_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cut_line");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_duplicate() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("duplicate");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_duplicate_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("duplicate_line");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_delete_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("delete_line");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_move_lines_up() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("move_lines_up");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_move_lines_down() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("move_lines_down");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_join_lines() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("join_lines");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_line_transforms() {
    for action in [
        "sort_unique",
        "reverse_lines",
        "remove_duplicate_lines",
        "trim_trailing_whitespace",
    ] {
        let mut app = app_at(Path::new("."));
        type_str(&mut app, "alpha\nbeta\nalpha  \n");
        app.run_action(action);
        assert!(
            app.editor.active_tab().is_some(),
            "{action} kept the app sane"
        );
    }
}

#[test]
fn catalog_sort_lines() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("sort_lines");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_indent_selection() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("indent_selection");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_outdent_selection() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("outdent_selection");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_autocomplete() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("autocomplete");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_cycle_autocomplete_back() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("cycle_autocomplete_back");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_outdent_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("outdent_line");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_indent_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("indent_line");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_paste() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("paste");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_paste_primary() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("paste_primary");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_all() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_all");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_open_file() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("open_file");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_start() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("start");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_end() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("end");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_page_up() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("page_up");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_page_down() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("page_down");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_page_up() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_page_up");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_select_page_down() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("select_page_down");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_half_page_up() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("half_page_up");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_half_page_down() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("half_page_down");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_start_of_text() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("start_of_text");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_start_of_text_toggle() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("start_of_text_toggle");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_start_of_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("start_of_line");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_end_of_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("end_of_line");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_toggle_help() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("toggle_help");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_toggle_key_menu() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("toggle_key_menu");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_toggle_diff_gutter() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("toggle_diff_gutter");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_toggle_ruler() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("toggle_ruler");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_toggle_highlight_search() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("toggle_highlight_search");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_unhighlight_search() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("unhighlight_search");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_reset_search() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("reset_search");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_clear_status() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("clear_status");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_shell_mode() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("shell_mode");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_command_mode() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("command_mode");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_toggle_overwrite_mode() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("toggle_overwrite_mode");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_escape() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("escape");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_quit() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("quit");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_quit_all() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("quit_all");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_force_quit() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("force_quit");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_add_tab() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("add_tab");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_previous_tab() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("previous_tab");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_next_tab() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("next_tab");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_first_tab() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("first_tab");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_last_tab() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("last_tab");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_next_split() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("next_split");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_previous_split() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("previous_split");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_first_split() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("first_split");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_last_split() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("last_split");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_unsplit() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("unsplit");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_vsplit() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("vsplit");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_hsplit() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("hsplit");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_suspend() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("suspend");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_scroll_up() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("scroll_up");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_scroll_down() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("scroll_down");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_spawn_multi_cursor() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("spawn_multi_cursor");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_spawn_multi_cursor_up() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("spawn_multi_cursor_up");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_spawn_multi_cursor_down() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("spawn_multi_cursor_down");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_spawn_multi_cursor_select() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("spawn_multi_cursor_select");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_remove_multi_cursor() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("remove_multi_cursor");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_remove_all_multi_cursors() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("remove_all_multi_cursors");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_skip_multi_cursor() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("skip_multi_cursor");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_skip_multi_cursor_back() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("skip_multi_cursor_back");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_jump_to_matching_brace() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("jump_to_matching_brace");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_jump_line() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("jump_line");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_deselect() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("deselect");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_clear_info() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("clear_info");
    assert!(app.editor.active_tab().is_some());
}

#[test]
fn catalog_none() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "alpha beta gamma\ndelta epsilon\n");
    app.run_action("none");
    assert!(app.editor.active_tab().is_some());
}
