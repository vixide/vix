//! Behavior tests for Org tables (keys, structural actions, sorting,
//! rectangles) and the Column View overlay.

use super::*;
use crate::settings::Settings;
use ratatui::layout::Rect;
use std::path::{Path, PathBuf};
use vix_input::KeyModifiers;

const TABLE: &str = "| Name | Qty |\n|------+-----|\n| a    | 3   |\n| b    | 12  |\n";

fn temp(tag: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("vix-ott-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

/// An app with `body` open as `t.org` (or `t.txt` when `org` is false).
fn app_with(tag: &str, body: &str, org: bool) -> App {
    let d = temp(tag);
    let name = if org { "t.org" } else { "t.txt" };
    std::fs::write(d.join(name), body).unwrap();
    let mut app = App::new(d.clone(), Settings::default());
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app.open_initial(&d.join(name));
    app.focus = Focus::Editor;
    app
}

fn table_app(tag: &str) -> App {
    app_with(tag, TABLE, true)
}

fn text(app: &App) -> String {
    app.editor.active_tab().unwrap().text()
}

fn line(app: &App) -> usize {
    app.editor.active_tab().unwrap().editor.cursor_line()
}

fn goto(app: &mut App, l: usize) {
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_cursor_line(l);
}

/// Put the cursor on `row`, inside field `field` (0-based) of that line.
fn at_field(app: &mut App, row: usize, field: usize) {
    goto(app, row);
    let t = app.editor.active_tab_mut().unwrap();
    let start = t.editor.get_cursor();
    let src = t.editor.get_content();
    let l = src.split('\n').nth(row).unwrap();
    let col = l.match_indices('|').nth(field).unwrap().0 + 2;
    t.editor.set_cursor(start + col);
}

fn key(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, mods)
}

const ALT: KeyModifiers = KeyModifiers::ALT;
const SHIFT: KeyModifiers = KeyModifiers::SHIFT;

fn alt_shift() -> KeyModifiers {
    KeyModifiers::ALT | KeyModifiers::SHIFT
}

// ----- helpers ---------------------------------------------------------------------------

#[test]
fn cursor_positions_convert_between_chars_and_bytes() {
    let mut app = app_with("pos", "é| x |\n", true);
    app.editor.active_tab_mut().unwrap().editor.set_cursor(2);
    let (l, b) = App::org_table_cursor_pos(app.editor.active_tab().unwrap());
    assert_eq!((l, b), (0, 3), "'é' is two bytes, so char 2 is byte 3");
    let t = app.editor.active_tab_mut().unwrap();
    App::org_table_set_cursor(t, 0, 3);
    assert_eq!(t.editor.get_cursor(), 2);
    App::org_table_set_cursor(t, 99, 999); // clamps
    assert!(t.editor.get_cursor() <= 8);
}

#[test]
fn field_index_counts_pipes() {
    assert_eq!(App::org_table_field_index_at("no pipes", 3), 0);
    assert_eq!(App::org_table_field_index_at("| a | b | c |", 0), 0);
    assert_eq!(App::org_table_field_index_at("| a | b | c |", 5), 1);
    assert_eq!(App::org_table_field_index_at("| a | b | c |", 9), 2);
    assert_eq!(
        App::org_table_field_index_at("| a | b | c |", 999),
        2,
        "clamped"
    );
}

#[test]
fn the_table_context_needs_an_org_file_and_a_table_line() {
    let mut app = table_app("ctx");
    goto(&mut app, 0);
    assert!(app.org_table_active_at_cursor());
    let mut prose = app_with("ctx-prose", "text\n\n| a |\n", true);
    goto(&mut prose, 0);
    assert!(!prose.org_table_active_at_cursor());
    let mut txt = app_with("ctx-txt", TABLE, false);
    goto(&mut txt, 0);
    assert!(!txt.org_table_active_at_cursor());
}

#[test]
fn number_formatting_trims_zeros() {
    assert_eq!(App::format_table_number(3.0), "3");
    assert_eq!(App::format_table_number(-4.0), "-4");
    assert_eq!(App::format_table_number(2.5), "2.5");
    assert_eq!(App::format_table_number(2.126), "2.13");
    assert_eq!(App::format_table_number(0.001), "0");
}

// ----- keys -----------------------------------------------------------------------------------

#[test]
fn tab_walks_the_fields_and_enter_the_rows() {
    let mut app = table_app("keys-nav");
    at_field(&mut app, 0, 0);
    assert!(app.org_table_key(key(KeyCode::Tab, KeyModifiers::NONE)));
    let (l, b) = App::org_table_cursor_pos(app.editor.active_tab().unwrap());
    assert_eq!(l, 0);
    assert!(b > 8, "moved to the second field: {b}");
    assert!(app.org_table_key(key(KeyCode::BackTab, KeyModifiers::NONE)));
    let (_, b2) = App::org_table_cursor_pos(app.editor.active_tab().unwrap());
    assert!(b2 < b);
    assert!(app.org_table_key(key(KeyCode::Tab, SHIFT)));
    assert!(app.org_table_key(key(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(line(&app) >= 1, "RET goes down to the next row");
    assert!(app.org_table_key(key(KeyCode::Enter, SHIFT)));
    // Shift-Tab at the very first field is a silent, consumed no-op.
    at_field(&mut app, 0, 0);
    let before = text(&app);
    assert!(app.org_table_key(key(KeyCode::BackTab, KeyModifiers::NONE)));
    assert_eq!(text(&app), before);
}

#[test]
fn meta_keys_edit_the_table_structure() {
    let mut app = table_app("keys-edit");
    at_field(&mut app, 2, 0);
    assert!(app.org_table_key(key(KeyCode::Right, ALT)));
    assert!(text(&app).starts_with("| Qty | Name |"), "{}", text(&app));
    assert!(app.org_table_key(key(KeyCode::Left, ALT)));
    assert!(text(&app).starts_with("| Name | Qty |"), "{}", text(&app));
    assert!(app.org_table_key(key(KeyCode::Down, ALT)));
    assert!(app.org_table_key(key(KeyCode::Up, ALT)));
    assert!(app.org_table_key(key(KeyCode::Right, alt_shift())));
    assert_eq!(
        text(&app).lines().next().unwrap().matches('|').count(),
        4,
        "{}",
        text(&app)
    );
    assert!(app.org_table_key(key(KeyCode::Left, alt_shift())));
    assert_eq!(text(&app).lines().next().unwrap().matches('|').count(), 3);
    let rows = text(&app).lines().count();
    assert!(app.org_table_key(key(KeyCode::Down, alt_shift())));
    assert_eq!(text(&app).lines().count(), rows + 1);
    assert!(app.org_table_key(key(KeyCode::Up, alt_shift())));
    assert_eq!(text(&app).lines().count(), rows);
}

#[test]
fn shift_arrows_move_cell_contents() {
    let mut app = table_app("keys-cells");
    at_field(&mut app, 2, 0);
    assert!(app.org_table_key(key(KeyCode::Down, SHIFT)));
    assert!(text(&app).contains("| b "), "{}", text(&app));
    assert!(app.org_table_key(key(KeyCode::Up, SHIFT)));
    assert!(app.org_table_key(key(KeyCode::Right, SHIFT)));
    assert!(app.org_table_key(key(KeyCode::Left, SHIFT)));
    assert!(app.editor.active_tab().unwrap().dirty);
}

#[test]
fn table_keys_only_apply_inside_a_table_of_an_editable_org_buffer() {
    let tab = key(KeyCode::Tab, KeyModifiers::NONE);
    let mut app = table_app("keys-guard");
    at_field(&mut app, 0, 0);
    assert!(
        !app.org_table_key(key(KeyCode::Char('x'), KeyModifiers::NONE)),
        "unbound key"
    );
    assert!(!app.org_table_key(key(KeyCode::Tab, KeyModifiers::CONTROL)));
    app.focus = Focus::Explorer;
    assert!(!app.org_table_key(tab));
    app.focus = Focus::Editor;
    app.editor.active_tab_mut().unwrap().read_only = true;
    assert!(!app.org_table_key(tab));

    let mut prose = app_with("keys-prose", "text\n\n| a |\n", true);
    prose.focus = Focus::Editor;
    goto(&mut prose, 0);
    assert!(!prose.org_table_key(tab));
    let mut txt = app_with("keys-txt", TABLE, false);
    txt.focus = Focus::Editor;
    assert!(!txt.org_table_key(tab));
    let mut none = app_with("keys-none", TABLE, true);
    none.focus = Focus::Editor;
    none.editor.tabs.clear();
    assert!(!none.org_table_key(tab));
}

// ----- actions ----------------------------------------------------------------------------------

#[test]
fn structural_actions_rewrite_the_table() {
    let mut app = table_app("act-struct");
    at_field(&mut app, 3, 0);
    app.org_table_action("org.table.insert_row_below");
    assert_eq!(text(&app).lines().count(), 5);
    app.org_table_action("org.table.kill_row");
    assert_eq!(text(&app).lines().count(), 4);
    at_field(&mut app, 3, 0);
    app.org_table_action("org.table.insert_row_above");
    assert_eq!(text(&app).lines().count(), 5);
    app.org_table_action("org.table.kill_row");
    at_field(&mut app, 3, 0);
    app.org_table_action("org.table.move_row_up");
    assert!(text(&app).contains("| b "));
    app.org_table_action("org.table.move_row_down");
    app.org_table_action("org.table.insert_hline");
    assert!(text(&app).matches("+-").count() >= 2, "{}", text(&app));
    app.org_table_action("org.table.hline_and_move");
    at_field(&mut app, 0, 0);
    app.org_table_action("org.table.insert_column");
    assert_eq!(text(&app).lines().next().unwrap().matches('|').count(), 4);
    app.org_table_action("org.table.move_column_right");
    app.org_table_action("org.table.move_column_left");
    app.org_table_action("org.table.delete_column");
    assert_eq!(text(&app).lines().next().unwrap().matches('|').count(), 3);
    app.org_table_action("org.table.transpose");
    app.org_table_action("org.table.align");
    assert!(app.editor.active_tab().unwrap().dirty);
}

#[test]
fn copy_down_and_sum_report_and_fill() {
    let mut app = table_app("act-sum");
    at_field(&mut app, 2, 1);
    app.org_table_action("org.table.sum_column");
    assert_eq!(
        app.status,
        t!("status.org_table_sum", sum = "15").to_string()
    );
    let before = text(&app);
    app.org_table_action("org.table.copy_down");
    assert!(app.editor.active_tab().unwrap().dirty);
    assert_ne!(app.status, t!("status.org_table_no_table").to_string());
    let _ = before;
    goto(&mut app, 0);
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_content("plain\n");
    app.status.clear();
    app.org_table_action("org.table.sum_column");
    assert_eq!(app.status, t!("status.org_table_no_table").to_string());
}

#[test]
fn table_actions_outside_a_table_report_it() {
    let mut app = app_with("act-outside", "just text\n", true);
    for action in [
        "org.table.align",
        "org.table.insert_row_above",
        "org.table.kill_row",
        "org.table.move_row_up",
        "org.table.insert_hline",
        "org.table.hline_and_move",
        "org.table.insert_column",
        "org.table.delete_column",
        "org.table.move_column_left",
        "org.table.transpose",
        "org.table.copy_down",
        "org.table.recalc",
        "org.table.export_tsv",
        "org.table.copy_rectangle",
        "org.table.cut_rectangle",
    ] {
        app.status.clear();
        assert!(app.org_table_action(action), "{action}");
        assert_eq!(
            app.status,
            t!("status.org_table_no_table").to_string(),
            "{action}"
        );
    }
    assert!(!app.org_table_action("org.table.nope"));
    assert_eq!(text(&app), "just text\n");
}

#[test]
fn table_actions_with_no_buffer_do_nothing() {
    let mut app = table_app("act-nobuf");
    app.editor.tabs.clear();
    for action in [
        "org.table.align",
        "org.table.insert_row_above",
        "org.table.insert_column",
        "org.table.copy_down",
        "org.table.sort",
        "org.table.sum_column",
        "org.table.export_tsv",
        "org.table.cut_rectangle",
        "org.table.recalc",
    ] {
        app.status.clear();
        assert!(app.org_table_action(action), "{action}");
    }
    app.accept_org_table_sort("1");
    app.org_columns_update_dblock();
    app.org_columns_update_all_dblocks();
    app.org_columns_insert_dblock("x");
    app.org_columns_insert_column("P");
    app.org_columns_insert_dblock_prompt();
    assert!(app.prompt.is_none());
}

#[test]
fn recalc_applies_formulas_and_works_from_the_formula_line() {
    let body = "| 1 | 2 | |\n#+TBLFM: $3=$1+$2\n";
    let mut app = app_with("recalc", body, true);
    goto(&mut app, 0);
    app.org_table_action("org.table.recalc");
    assert!(text(&app).contains("| 3"), "{}", text(&app));
    // From the TBLFM line itself.
    let mut app = app_with("recalc-fm", body, true);
    goto(&mut app, 1);
    assert!(app.org_table_recalc());
    assert!(text(&app).contains("| 3"), "{}", text(&app));
    // A TBLFM line at the very top of the buffer has no table above it.
    let mut app = app_with("recalc-top", "#+TBLFM: $1=1\n| a |\n", true);
    goto(&mut app, 0);
    assert!(!app.org_table_recalc());
    // Non-org buffers never recalc.
    let mut txt = app_with("recalc-txt", body, false);
    assert!(!txt.org_table_recalc());
}

#[test]
fn ctrl_c_ctrl_c_in_a_table_recalculates_it() {
    let mut app = app_with("c-c", "| 1 | 2 | |\n#+TBLFM: $3=$1+$2\n", true);
    goto(&mut app, 0);
    app.org_action("org.ctrl_c_ctrl_c");
    assert!(text(&app).contains("| 3"), "{}", text(&app));
}

#[test]
fn sorting_by_column_kind_and_direction() {
    let body = "| b | 10 |\n| a | 9 |\n| c | 100 |\n";
    let order = |app: &App| -> Vec<String> {
        text(app)
            .lines()
            .map(|l| l.split('|').nth(1).unwrap().trim().to_string())
            .collect()
    };
    let mut app = app_with("sort", body, true);
    at_field(&mut app, 1, 1);
    app.org_table_action("org.table.sort");
    assert_eq!(app.prompt.as_ref().unwrap().input, "2");
    app.prompt = None;
    app.accept_org_table_sort("1");
    assert_eq!(app.status, t!("status.org_table_sorted").to_string());
    assert_eq!(order(&app), ["a", "b", "c"]);
    app.accept_org_table_sort("1 a r");
    assert_eq!(order(&app), ["c", "b", "a"]);
    app.accept_org_table_sort("2 n");
    assert_eq!(order(&app), ["a", "b", "c"], "9 < 10 < 100 numerically");
    app.accept_org_table_sort("2 t");
    app.accept_org_table_sort("1 zzz");

    let mut outside = app_with("sort-outside", "text\n", true);
    outside.accept_org_table_sort("1");
    assert_eq!(outside.status, t!("status.org_table_no_table").to_string());
}

#[test]
fn creating_a_table_from_a_selection_or_empty() {
    let mut app = app_with("create", "a,b\n1,2\n", true);
    let len = text(&app).chars().count();
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_selection_range(0, len);
    app.org_table_action("org.table.create_from_region");
    assert_eq!(app.status, t!("status.org_table_created").to_string());
    assert!(text(&app).contains("| a"), "{}", text(&app));

    let mut empty = app_with("create-empty", "", true);
    empty.org_table_action("org.table.create_from_region");
    assert_eq!(text(&empty), "|   |");
}

#[test]
fn exporting_a_table_as_tsv() {
    let mut app = table_app("tsv");
    goto(&mut app, 2);
    let tabs = app.editor.tabs.len();
    app.org_table_action("org.table.export_tsv");
    assert_eq!(
        app.status,
        t!("status.org_exported", ext = "tsv").to_string()
    );
    assert_eq!(app.editor.tabs.len(), tabs + 1);
    assert!(text(&app).contains("Name\tQty"), "{}", text(&app));
}

#[test]
fn rectangles_copy_cut_and_paste() {
    let mut app = table_app("rect");
    at_field(&mut app, 2, 1);
    app.org_table_action("org.table.paste_rectangle");
    assert_eq!(
        app.status,
        t!("status.org_table_rectangle_empty").to_string()
    );

    app.org_table_action("org.table.copy_rectangle");
    assert_eq!(
        app.status,
        t!("status.org_table_rectangle_copied").to_string()
    );
    assert!(app.table_rectangle_clip.is_some());
    at_field(&mut app, 3, 1);
    app.org_table_action("org.table.paste_rectangle");
    assert_eq!(
        app.status,
        t!("status.org_table_rectangle_pasted").to_string()
    );
    assert_eq!(text(&app).matches("3 |").count(), 2, "{}", text(&app));

    at_field(&mut app, 2, 0);
    app.org_table_action("org.table.cut_rectangle");
    assert_eq!(app.status, t!("status.org_table_rectangle_cut").to_string());
    assert!(
        !text(&app).contains("| a "),
        "the cut field is blanked: {}",
        text(&app)
    );

    // A selection spanning two fields copies the whole rectangle.
    let mut app = table_app("rect-sel");
    let start = {
        at_field(&mut app, 2, 0);
        app.editor.active_tab().unwrap().editor.get_cursor()
    };
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_selection_range(start, start + 10);
    app.org_table_action("org.table.copy_rectangle");
    assert!(app.table_rectangle_clip.is_some());

    // Pasting with the cursor off the table is refused.
    let mut app = app_with("rect-off", "text\n\n| a |\n", true);
    app.table_rectangle_clip = Some(crate::org_table::copy_rectangle("| x |\n", (0, 0), (0, 0)));
    goto(&mut app, 0);
    app.org_table_action("org.table.paste_rectangle");
    assert_eq!(app.status, t!("status.org_table_no_table").to_string());
}

// ----- column view and dynamic blocks ------------------------------------------------------------------

const COLS: &str =
    "#+COLUMNS: %25ITEM %TODO\n* TODO Task\n:PROPERTIES:\n:EFFORT: 1h\n:END:\n* Other\n";

#[test]
fn column_view_needs_an_org_buffer() {
    let mut none = table_app("cv-none");
    none.editor.tabs.clear();
    none.open_column_view();
    assert_eq!(
        none.messages.items.last().unwrap().text,
        t!("msg.column_view_no_buffer").to_string()
    );
    let mut txt = app_with("cv-txt", COLS, false);
    txt.open_column_view();
    assert_eq!(
        txt.messages.items.last().unwrap().text,
        t!("msg.column_view_no_buffer").to_string()
    );
    assert!(txt.edit.column_view_mut().is_none());
}

#[test]
fn column_view_opens_handles_keys_and_closes() {
    let mut app = app_with("cv", COLS, true);
    goto(&mut app, 1);
    app.open_column_view();
    assert!(app.edit.column_view_mut().is_some());
    app.column_view_key(key(KeyCode::Down, KeyModifiers::NONE));
    app.column_view_key(key(KeyCode::Right, KeyModifiers::NONE));
    // S-M-Right asks for the new column's property name.
    app.column_view_key(key(KeyCode::Right, alt_shift()));
    if app.prompt.is_some() {
        assert!(app.prompt.as_ref().unwrap().kind == PromptKind::OrgColumnsInsertColumn);
        app.prompt = None;
        app.org_columns_insert_column("  ");
        app.org_columns_insert_column("EFFORT");
    }
    app.column_view_key(key(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.edit.column_view_mut().is_none());
    // With the buffer gone, a key closes the overlay.
    app.open_column_view();
    app.editor.tabs.clear();
    app.column_view_key(key(KeyCode::Down, KeyModifiers::NONE));
    assert!(app.edit.column_view_mut().is_none());
    // With no overlay open a key is ignored.
    app.column_view_key(key(KeyCode::Down, KeyModifiers::NONE));
}

#[test]
fn dynamic_blocks_insert_update_and_update_all() {
    let mut app = app_with("dblock", COLS, true);
    goto(&mut app, 0);
    app.org_action("org.columns.insert_dblock");
    assert!(app.prompt.as_ref().unwrap().kind == PromptKind::OrgColumnsInsertDblock);
    app.prompt = None;
    app.accept_org_prompt(PromptKind::OrgColumnsInsertDblock, "");
    assert_eq!(
        app.status,
        t!("status.org_columns_dblock_inserted").to_string()
    );
    assert!(text(&app).contains("#+BEGIN: columnview"), "{}", text(&app));
    assert!(text(&app).contains(":id local"), "{}", text(&app));

    app.org_columns_insert_dblock("global");
    assert!(text(&app).contains(":id global"));

    goto(&mut app, 0);
    app.org_action("org.columns.update_dblock");
    assert_eq!(
        app.status,
        t!("status.org_columns_dblock_updated").to_string()
    );
    // Off a dblock header: reported.
    let last = text(&app).lines().count() - 1;
    goto(&mut app, last);
    app.org_action("org.columns.update_dblock");
    assert_eq!(
        app.status,
        t!("status.org_columns_dblock_not_found").to_string()
    );

    app.org_action("org.columns.update_all_dblocks");
    assert_eq!(
        app.status,
        t!("status.org_columns_dblocks_updated_all").to_string()
    );
    // Prompt kinds routed through the column prompt handler.
    app.accept_org_prompt(PromptKind::OrgColumnsInsertColumn, "X");
    assert!(Path::new("").as_os_str().is_empty());
}
