//! Round-3 behavior tests for `app.rs` and the `app/` modules (test-only).

use super::*;
use crate::app::cov4::{app_at, temp};

fn k(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn wheel(kind: MouseEventKind) -> MouseEvent {
    MouseEvent {
        kind,
        column: 1,
        row: 1,
        modifiers: KeyModifiers::NONE,
    }
}

/// Every edit/search/fold/escape action quietly does nothing when no tab is
/// open, and leaves the workspace without a tab.
#[test]
fn editing_actions_without_any_tab_are_harmless() {
    let mut a = app_at(&temp("r3-notab"));
    a.editor.tabs.clear();
    for id in [
        "edit.undo",
        "edit.redo",
        "edit.cut",
        "edit.copy",
        "edit.paste",
        "edit.toggle_comment",
        "edit.select_all_occurrences",
        "edit.column_select_down",
        "edit.column_select_up",
        "editor.fold_all",
        "editor.unfold_all",
        "unhighlight_search",
        "escape",
        "edit.emmet_expand",
        "edit.comment_banner",
        "edit.toggle_read_only",
    ] {
        a.run_action(id);
        assert!(a.editor.tabs.is_empty(), "{id} must not create a tab");
    }
    assert_eq!(a.breadcrumb(), "");
    assert!(a.active_tab_symbols().is_empty());
    assert_eq!(a.word_prefix_before_cursor(), "");
    assert_eq!(a.cursor_lsp_position(Path::new("x.rs")), (0, 0));
    a.goto_matching_tag();
    a.expand_selection_to_node();
    a.revert_active();
    a.project_test_at_point();
    a.restore_persistent_undo(Path::new("x.rs"));
    a.toggle_read_only();
    assert!(a.editor.tabs.is_empty());
}

/// Panel key/mouse handlers do nothing when their panel is not open.
#[test]
fn panel_navigation_is_ignored_when_the_panel_is_closed() {
    let mut a = app_at(&temp("r3-closed"));
    for code in [
        KeyCode::Up,
        KeyCode::Down,
        KeyCode::Enter,
        KeyCode::Char('x'),
    ] {
        a.tasks_key(k(code));
        a.clipboard_key(k(code));
        a.macro_key(k(code));
        a.encoding_key(k(code));
        a.recent_key(k(code));
        a.location_key(k(code));
        a.workspace_chooser_key(k(code));
        a.code_action_key(k(code));
        a.spell_suggest_key(k(code));
        a.context_menu_key(k(code));
        a.keybinding_editor_key(k(code));
        a.edit_table_key(k(code));
        a.edit_outline_key(k(code));
        a.edit_value_key(k(code));
        a.edit_bytes_key(k(code));
        a.outline_key(k(code));
        assert!(!a.jump_key(k(code)));
    }
    a.tasks_mouse(wheel(MouseEventKind::ScrollUp));
    a.help_mouse(wheel(MouseEventKind::ScrollUp));
    a.help_mouse(wheel(MouseEventKind::ScrollDown));
    a.file_browser_mouse(wheel(MouseEventKind::ScrollUp));
    a.file_browser_mouse(wheel(MouseEventKind::ScrollDown));
    a.keybinding_editor_mouse(wheel(MouseEventKind::ScrollUp));
    a.keybinding_editor_mouse(wheel(MouseEventKind::ScrollDown));
    a.context_move(true);
    a.context_move(false);
    a.run_selected_task();
    a.paste_selected_clipboard();
    a.run_selected_macro();
    a.apply_selected_encoding();
    a.switch_to_selected_workspace();
    a.open_selected_recent();
    a.open_selected_location();
    a.spell_apply_selected();
    a.file_browser_open_selected();
    a.apply_selected_code_action();
    assert!(a.chooser.task_mut().is_none());
    assert!(a.keybinding_editor.is_none());
    assert!(a.code_actions.is_none());
    assert!(a.context_menu.is_none());
}

/// Pomodoro: Enter on a missing timer does nothing; Start hides the dialog
/// and runs; Esc cancels a run and drops the timer.
#[test]
fn pomodoro_primary_and_escape_cover_idle_running_and_missing_timer() {
    let mut a = app_at(&temp("r3-pomo"));
    a.pomodoro_primary();
    assert!(a.pomodoro.is_none());
    a.pomodoro_key(k(KeyCode::Esc));
    assert!(a.pomodoro.is_none());
    a.open_pomodoro();
    assert!(a.visible.contains(Visible::POMODORO));
    a.pomodoro_key(k(KeyCode::Char(' ')));
    assert!(a.pomodoro_running());
    assert!(!a.visible.contains(Visible::POMODORO));
    // A running timer: the primary button stops it; the dialog stays up.
    a.visible.set(Visible::POMODORO, true);
    a.pomodoro_key(k(KeyCode::Enter));
    assert!(!a.pomodoro_running());
    a.pomodoro_key(k(KeyCode::Char('z')));
    a.pomodoro_key(k(KeyCode::Esc));
    assert!(a.pomodoro.is_none());
    assert!(!a.visible.contains(Visible::POMODORO));
}

/// The edit surfaces (table, outline, JSON/YAML, bytes) refuse to open with
/// no buffer, and their save/close handlers are no-ops when closed.
#[test]
fn edit_surfaces_warn_without_a_buffer_and_ignore_saves_when_closed() {
    let mut a = app_at(&temp("r3-edit-none"));
    a.editor.tabs.clear();
    let rows = [
        (
            "tools.edit_table",
            t!("msg.edit_table_no_buffer").to_string(),
        ),
        (
            "tools.edit_outline",
            t!("msg.edit_outline_no_buffer").to_string(),
        ),
        (
            "tools.edit_json",
            t!("msg.edit_value_no_buffer").to_string(),
        ),
        (
            "tools.edit_yaml",
            t!("msg.edit_value_no_buffer").to_string(),
        ),
        (
            "tools.edit_bytes",
            t!("msg.edit_bytes_no_buffer").to_string(),
        ),
    ];
    let sql_row =
        cfg!(feature = "sql").then(|| ("tools.edit_sql", t!("msg.edit_sql_no_buffer").to_string()));
    for (id, want) in rows.into_iter().chain(sql_row) {
        a.run_action(id);
        assert_eq!(
            a.messages.items.last().map(|m| m.text.clone()),
            Some(want),
            "{id}"
        );
    }
    a.save_edit_table();
    a.save_edit_outline();
    a.save_edit_value();
    a.save_edit_bytes();
    assert!(a.edit.edit_table().is_none());
    assert!(a.edit.edit_outline().is_none());
    assert!(a.edit.edit_value().is_none());
    assert!(a.edit.edit_bytes().is_none());
}

/// Saving each edit surface writes its serialized text into the buffer and
/// the file; closing with Esc removes the overlay.
#[test]
fn edit_surfaces_save_into_the_buffer_and_close() {
    let dir = temp("r3-edit-save");
    let mut a = app_at(&dir);
    let csv = dir.join("t.csv");
    std::fs::write(&csv, "a,b\n1,2\n").unwrap();
    a.open_path(&csv, false);
    a.run_action("tools.edit_table");
    assert!(a.edit.edit_table().is_some());
    a.edit_table_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert!(!a.editor.active_tab().unwrap().dirty);
    assert_eq!(std::fs::read_to_string(&csv).unwrap(), "a,b\n1,2\n");
    a.edit_table_key(k(KeyCode::Esc));
    assert!(a.edit.edit_table().is_none());

    let org = dir.join("o.txt");
    std::fs::write(&org, "one\n  two\n").unwrap();
    a.open_path(&org, false);
    a.run_action("tools.edit_outline");
    assert!(a.edit.edit_outline().is_some());
    a.edit_outline_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert_eq!(std::fs::read_to_string(&org).unwrap(), "one\n  two\n");
    a.edit_outline_key(k(KeyCode::Esc));
    assert!(a.edit.edit_outline().is_none());

    let json = dir.join("j.json");
    std::fs::write(&json, "{\"a\": 1}\n").unwrap();
    a.open_path(&json, false);
    a.run_action("tools.edit_json");
    assert!(a.edit.edit_value().is_some());
    a.edit_value_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert!(!a.editor.active_tab().unwrap().dirty);
    a.edit_value_key(k(KeyCode::Esc));
    assert!(a.edit.edit_value().is_none());

    // Keys that JSON would write alike: warns, the file is left as it was (T850).
    let alike = "1: a\n\"1\": b\n";
    std::fs::write(&json, alike).unwrap();
    a.editor.active_tab_mut().unwrap().editor.set_content(alike);
    a.run_action("tools.edit_json");
    a.edit_value_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert_eq!(
        a.messages.items.last().map(|m| m.text.clone()),
        Some(t!("msg.edit_value_duplicate_key", key = "\"1\"").to_string())
    );
    assert_eq!(std::fs::read_to_string(&json).unwrap(), alike);
    a.edit_value_key(k(KeyCode::Esc));
    let complex = "? [a]\n: x\n";
    a.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_content(complex);
    a.run_action("tools.edit_json");
    a.edit_value_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert_eq!(
        a.messages.items.last().map(|m| m.text.clone()),
        Some(t!("msg.edit_value_complex_key").to_string())
    );
    a.edit_value_key(k(KeyCode::Esc));
    // A YAML mapping whose only key is tagged: its own warning (T853).
    let lone = "!k x: 0\n";
    a.editor.active_tab_mut().unwrap().editor.set_content(lone);
    a.run_action("tools.edit_yaml");
    a.edit_value_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert_eq!(
        a.messages.items.last().map(|m| m.text.clone()),
        Some(t!("msg.edit_value_lone_tagged_key", key = "!k x").to_string())
    );
    assert_eq!(a.editor.active_tab().unwrap().editor.get_content(), lone);
    a.edit_value_key(k(KeyCode::Esc));

    // Not valid JSON: warns, nothing opens.
    std::fs::write(&json, "{nope").unwrap();
    a.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_content("{nope");
    a.run_action("tools.edit_json");
    assert!(a.edit.edit_value().is_none());
    assert!(a.messages.items.last().is_some_and(|m| {
        m.text
            .starts_with(&t!("msg.edit_value_parse", error = "").to_string()[..4])
    }));
}

/// The byte editor saves the buffer's bytes; a file opened straight into it
/// is written back byte for byte.
#[test]
fn byte_editor_saves_buffer_and_binary_files() {
    let dir = temp("r3-bytes");
    let mut a = app_at(&dir);
    let txt = dir.join("b.txt");
    std::fs::write(&txt, "hi\n").unwrap();
    a.open_path(&txt, false);
    a.run_action("tools.edit_bytes");
    assert!(a.edit.edit_bytes().is_some());
    a.edit_bytes_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert_eq!(std::fs::read(&txt).unwrap(), b"hi\n");
    a.edit_bytes_key(k(KeyCode::Char('q')));
    assert!(a.edit.edit_bytes().is_none());

    let bin = dir.join("raw.bin");
    std::fs::write(&bin, [0u8, 159, 146, 150, 0, 1]).unwrap();
    a.open_path(&bin, false);
    assert_eq!(
        a.confirm.as_ref().map(|c| c.kind),
        Some(ConfirmKind::OpenBinary)
    );
    // Typing something else keeps the question open; n cancels it.
    a.confirm_key(k(KeyCode::Char('x')));
    assert!(a.confirm.is_some());
    a.confirm_key(k(KeyCode::Char('n')));
    assert!(a.confirm.is_none());
    assert_eq!(a.status, t!("status.open_cancelled").to_string());
    a.open_path(&bin, false);
    a.confirm_key(k(KeyCode::Char('y')));
    assert!(a.edit.edit_bytes().is_some());
    assert_eq!(a.edit_bytes_file.as_deref(), Some(bin.as_path()));
    a.edit_bytes_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert_eq!(std::fs::read(&bin).unwrap(), [0u8, 159, 146, 150, 0, 1]);
    assert_eq!(
        a.status,
        t!("status.saved", path = bin.display()).to_string()
    );
}

/// A file over `large_file_mb` asks first; `v` opens the viewer, `y` the
/// text buffer anyway.
#[test]
fn a_large_file_can_open_in_the_viewer_or_as_text() {
    let dir = temp("r3-large");
    let mut settings = Settings::default();
    settings.editor_behavior.large_file_mb = 1;
    let mut a = crate::app::cov4::app_with(&dir, settings);
    let big = dir.join("big.txt");
    std::fs::write(&big, "x".repeat(1024 * 1024 + 10)).unwrap();
    a.open_path(&big, false);
    assert_eq!(
        a.confirm.as_ref().map(|c| c.kind),
        Some(ConfirmKind::OpenLarge)
    );
    a.confirm_key(k(KeyCode::Char('v')));
    assert!(a.confirm.is_none());
    assert!(a.large_file.is_some());
    a.large_file = None;
    a.open_path(&big, false);
    a.confirm_key(k(KeyCode::Char('y')));
    assert_eq!(a.active_path().as_deref(), Some(big.as_path()));
    // An arrow-key preview of a too-large file only reports it.
    let big2 = dir.join("big2.txt");
    std::fs::write(&big2, "y".repeat(1024 * 1024 + 10)).unwrap();
    a.open_path(&big2, true);
    assert!(a.confirm.is_none());
    assert_eq!(
        a.status,
        t!(
            "msg.open_failed_path",
            path = big2.display(),
            error = t!("msg.open_too_large")
        )
        .to_string()
    );
}

/// The delete confirmation answers y/n; a stale save-as target is dropped
/// when a different question replaces it.
#[test]
fn confirm_keys_delete_cancel_and_stale_save_as_target() {
    let dir = temp("r3-confirm");
    let mut a = app_at(&dir);
    let f = dir.join("gone.txt");
    std::fs::write(&f, "x").unwrap();
    a.confirm = Some(Confirm {
        message: String::new(),
        paths: vec![f.clone()],
        kind: ConfirmKind::Delete,
    });
    a.confirm_key(k(KeyCode::Char('n')));
    assert!(a.confirm.is_none());
    assert_eq!(a.status, t!("status.delete_cancelled").to_string());
    assert!(f.exists());
    a.save_as_overwrite = Some(dir.join("elsewhere.txt"));
    a.confirm = Some(Confirm {
        message: String::new(),
        paths: vec![f],
        kind: ConfirmKind::Delete,
    });
    a.confirm_key(k(KeyCode::Char('x')));
    assert!(a.save_as_overwrite.is_none());
    assert!(a.confirm.is_some());
    // A save-as overwrite question: n cancels and says so.
    let target = dir.join("target.txt");
    a.save_as_overwrite = Some(target.clone());
    a.confirm = Some(Confirm {
        message: String::new(),
        paths: vec![target],
        kind: ConfirmKind::Delete,
    });
    a.confirm_key(k(KeyCode::Char('?')));
    assert!(a.confirm.is_some());
    a.confirm_key(k(KeyCode::Esc));
    assert!(a.confirm.is_none() && a.save_as_overwrite.is_none());
    assert_eq!(a.status, t!("status.save_as_cancelled").to_string());
}

/// Unsaved-changes prompt: discarding while quitting moves on to the next
/// dirty tab, then quits; a stale prompt with no mode is just dropped.
#[test]
fn unsaved_prompt_discards_through_every_dirty_tab_then_quits() {
    let dir = temp("r3-unsaved");
    let mut a = app_at(&dir);
    a.editor.new_tab_with_content("one");
    a.editor.active_tab_mut().unwrap().dirty = true;
    a.editor.new_tab_with_content("two");
    a.editor.active_tab_mut().unwrap().dirty = true;
    a.unsaved = Some(UnsavedPrompt {
        mode: UnsavedMode::Quit,
        name: String::new(),
    });
    a.advance_quit();
    assert!(a.unsaved.is_some());
    assert_eq!(a.unsaved.as_ref().unwrap().name, a.active_tab_name());
    a.unsaved_key(k(KeyCode::Char('d')));
    assert!(
        a.unsaved.is_some(),
        "the other dirty tab is asked about next"
    );
    a.unsaved_key(k(KeyCode::Char('d')));
    assert!(a.unsaved.is_none());
    assert!(a.flags.contains(AppFlags::SHOULD_QUIT));
    // No guarded action: d and s only drop the prompt.
    let mut b = app_at(&dir);
    b.unsaved_discard();
    assert!(b.unsaved.is_none());
    b.unsaved = None;
    b.unsaved_save();
    assert!(b.unsaved.is_none());
}

fn with_text(tag: &str, text: &str) -> App {
    let mut a = app_at(&temp(tag));
    a.editor.new_tab_with_content(text);
    a.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
    a
}

fn alt_key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT)
}

/// Find: an invalid regex is reported in the box, no-match highlights and
/// the sticky-highlight toggle all behave.
#[test]
fn find_reports_bad_regex_and_toggles_highlights() {
    let mut a = with_text("r3-find", "foo bar foo\n");
    a.run_action("edit.find");
    for c in "(".chars() {
        a.search_key(k(KeyCode::Char(c)));
    }
    a.search_key(alt_key('r'));
    assert!(
        a.search
            .as_ref()
            .unwrap()
            .flags
            .contains(SearchFlags::REGEX)
    );
    a.find_step(true);
    assert!(
        a.search
            .as_ref()
            .unwrap()
            .status
            .starts_with(&t!("msg.bad_regex", error = "").to_string())
    );
    // Backspace removes the "(" and a valid query steps to a match.
    a.search_key(k(KeyCode::Backspace));
    assert_eq!(a.search.as_ref().unwrap().query, "");
    a.search_key(k(KeyCode::Char('f')));
    a.search_key(k(KeyCode::Char('o')));
    assert_eq!(
        a.search.as_ref().unwrap().status,
        t!("status.match_of", index = 1, total = 2).to_string()
    );
    // Tab to the replace field in replace mode, Alt+H toggles replace mode.
    a.search_key(alt_key('h'));
    assert!(
        a.search
            .as_ref()
            .unwrap()
            .flags
            .contains(SearchFlags::REPLACING)
    );
    assert_eq!(a.search.as_ref().unwrap().field, Field::Replace);
    a.search_key(k(KeyCode::Tab));
    assert_eq!(a.search.as_ref().unwrap().field, Field::Query);
    a.search_key(alt_key('q'));
    // Esc without sticky highlights clears the marks.
    a.settings.misc.sticky_search_highlight = false;
    a.search_key(k(KeyCode::Esc));
    assert!(a.search.is_none());
    assert!(!a.editor.active_tab().unwrap().editor.has_marks());
    // Find Next with the box closed repeats the remembered search.
    a.find_step(true);
    assert_eq!(
        a.status,
        t!("status.match_of", index = 2, total = 2).to_string()
    );
    // Toggle highlight: on -> off -> on again, and a bad remembered pattern.
    a.toggle_search_highlight();
    assert_eq!(a.status, t!("status.highlights_off").to_string());
    a.toggle_search_highlight();
    assert_eq!(a.status, t!("status.matches", count = 2).to_string());
    a.clear_marks();
    a.last_search = Some("zzz".into());
    a.toggle_search_highlight();
    assert_eq!(a.status, t!("status.no_matches").to_string());
    a.last_search = Some("(".into());
    a.status.clear();
    a.toggle_search_highlight();
    assert_eq!(a.status, "");
    a.last_search = None;
    a.toggle_search_highlight();
    assert_eq!(a.status, t!("status.no_matches").to_string());
}

/// Find with no remembered search falls back to the selection or word under
/// the cursor; an empty buffer says nothing is selected.
#[test]
fn find_without_a_pattern_uses_the_selection_or_word() {
    let mut a = with_text("r3-findsel", "");
    a.find_step(true);
    assert_eq!(a.status, t!("status.no_selection").to_string());
    let mut a = with_text("r3-findsel2", "word other word\n");
    a.editor.active_tab_mut().unwrap().editor.set_cursor(1);
    a.find_step(true);
    assert_eq!(
        a.status,
        t!("status.match_of", index = 2, total = 2).to_string()
    );
    a.editor.tabs.clear();
    assert_eq!(a.find_with(&Regex::new("x").unwrap(), true), (0, 0));
    assert_eq!(a.highlight_all(&Regex::new("x").unwrap()), 0);
    // A box with an empty query just clears the marks.
    let mut a = with_text("r3-findempty", "abc\n");
    a.run_action("edit.find");
    a.find_step(true);
    assert!(!a.editor.active_tab().unwrap().editor.has_marks());
}

/// Replace once / all: counts, no-match message, bad regex, no search open.
#[test]
fn replace_once_and_all_report_through_the_find_box() {
    let mut a = with_text("r3-repl", "aa bb aa\n");
    a.replace_once();
    a.replace_all();
    assert!(a.search.is_none());
    a.run_action("edit.replace");
    a.replace_once(); // empty query: nothing to do
    for c in "aa".chars() {
        a.search_key(k(KeyCode::Char(c)));
    }
    a.search_key(k(KeyCode::Tab));
    a.search_key(k(KeyCode::Char('X')));
    a.replace_once();
    assert_eq!(
        a.search.as_ref().unwrap().status,
        t!("status.replaced", count = 1).to_string()
    );
    assert_eq!(a.editor.active_tab().unwrap().text(), "aa bb X\n");
    // Enter on the replace field replaces everything left.
    a.search_key(k(KeyCode::Enter));
    assert_eq!(a.editor.active_tab().unwrap().text(), "X bb X\n");
    assert_eq!(
        a.search.as_ref().unwrap().status,
        t!("status.replaced", count = 1).to_string()
    );
    a.search.as_mut().unwrap().query = "nomatch".into();
    a.replace_once();
    assert_eq!(
        a.search.as_ref().unwrap().status,
        t!("status.qr_no_matches").to_string()
    );
    a.search.as_mut().unwrap().flags.insert(SearchFlags::REGEX);
    a.search.as_mut().unwrap().query = "(".into();
    a.replace_once();
    assert!(
        a.search
            .as_ref()
            .unwrap()
            .status
            .starts_with(&t!("msg.bad_regex", error = "").to_string())
    );
    a.replace_all();
    assert!(
        a.search
            .as_ref()
            .unwrap()
            .status
            .starts_with(&t!("msg.bad_regex", error = "").to_string())
    );
    a.search.as_mut().unwrap().query = "b+".into();
    a.search.as_mut().unwrap().replace = "[$0]".into();
    a.editor.tabs.clear();
    a.replace_once();
    a.replace_all();
    assert!(a.editor.tabs.is_empty());
}

/// Clicking the box's scope button widens the search to the workspace
/// panel, then to the bottom dock; the toggles flip flags; field rows focus.
#[test]
fn search_box_mouse_scope_toggles_and_fields() {
    use ratatui::layout::Rect;
    let mut a = with_text("r3-searchmouse", "alpha beta\n");
    std::fs::write(a.root.join("f.txt"), "alpha\n").unwrap();
    a.run_action("edit.replace");
    a.search.as_mut().unwrap().query = "alpha".into();
    a.layout.search_scope = Rect::new(0, 0, 4, 1);
    a.layout.search_replace_toggle = Rect::new(10, 0, 4, 1);
    a.layout.search_case = Rect::new(0, 3, 2, 1);
    a.layout.search_smartcase = Rect::new(3, 3, 2, 1);
    a.layout.search_word = Rect::new(6, 3, 2, 1);
    a.layout.search_regex = Rect::new(9, 3, 2, 1);
    a.layout.search_once = Rect::new(0, 5, 2, 1);
    a.layout.search_ask = Rect::new(3, 5, 2, 1);
    a.layout.search_all = Rect::new(6, 5, 2, 1);
    a.layout.search = Rect::new(0, 8, 20, 4);
    let click = |c, r| MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: c,
        row: r,
        modifiers: KeyModifiers::NONE,
    };
    a.search_mouse(wheel(MouseEventKind::ScrollUp));
    a.search_mouse(click(11, 0));
    assert!(
        !a.search
            .as_ref()
            .unwrap()
            .flags
            .contains(SearchFlags::REPLACING)
    );
    a.search_mouse(click(11, 0));
    assert!(
        a.search
            .as_ref()
            .unwrap()
            .flags
            .contains(SearchFlags::REPLACING)
    );
    a.search_mouse(click(0, 3));
    assert!(
        a.search
            .as_ref()
            .unwrap()
            .flags
            .contains(SearchFlags::CASE_SENSITIVE)
    );
    a.search_mouse(click(3, 3));
    a.search_mouse(click(6, 3));
    assert!(
        a.search
            .as_ref()
            .unwrap()
            .flags
            .contains(SearchFlags::WHOLE_WORD)
    );
    a.search_mouse(click(9, 3));
    assert!(
        a.search
            .as_ref()
            .unwrap()
            .flags
            .contains(SearchFlags::REGEX)
    );
    a.search_mouse(click(0, 10));
    assert_eq!(a.search.as_ref().unwrap().field, Field::Replace);
    a.search_mouse(click(0, 8));
    assert_eq!(a.search.as_ref().unwrap().field, Field::Query);
    a.search_mouse(click(60, 20));
    a.search.as_mut().unwrap().flags.remove(SearchFlags::REGEX);
    a.search_mouse(click(0, 5));
    a.search_mouse(click(6, 5));
    a.search_mouse(click(3, 5));
}

/// The scope button widens a find to the workspace panel, then to the bottom
/// dock; an empty query asks for a pattern; cycling back keeps the box.
#[test]
fn the_search_scope_widens_to_files_and_the_dock() {
    use ratatui::layout::Rect;
    let click = |c, r| MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: c,
        row: r,
        modifiers: KeyModifiers::NONE,
    };
    // Scope: buffer -> files.
    let mut a = with_text("r3-scope", "alpha\n");
    std::fs::write(a.root.join("f.txt"), "alpha\n").unwrap();
    a.run_action("edit.replace");
    a.search.as_mut().unwrap().query = "alpha".into();
    a.search.as_mut().unwrap().replace = "omega".into();
    a.layout.search_scope = Rect::new(0, 0, 4, 1);
    a.search_mouse(click(1, 0));
    assert!(a.search.is_none());
    let ws = a.workspace_search.as_ref().unwrap();
    assert_eq!(ws.query, "alpha");
    assert_eq!(ws.replace, "omega");
    // files -> workspace with a query: results go to the bottom dock.
    let mut a = with_text("r3-scope2", "alpha\n");
    a.run_action("edit.find");
    a.search.as_mut().unwrap().query = "alpha".into();
    a.widen_search_scope();
    a.workspace_search = None;
    a.run_action("edit.find");
    a.search.as_mut().unwrap().query = "alpha".into();
    a.search.as_mut().unwrap().scope = Scope::Files;
    a.widen_search_scope();
    assert!(a.search.is_none());
    assert!(a.visible.contains(Visible::BOTTOM_DOCK));
    // ... and with an empty query it asks for the pattern.
    let mut a = with_text("r3-scope3", "alpha\n");
    a.run_action("edit.find");
    a.search.as_mut().unwrap().scope = Scope::Files;
    a.widen_search_scope();
    assert!(
        a.prompt
            .as_ref()
            .is_some_and(|p| p.kind == PromptKind::SearchToDock)
    );
    // Back to the buffer keeps the box.
    let mut a = with_text("r3-scope4", "alpha\n");
    a.run_action("edit.find");
    a.search.as_mut().unwrap().scope = Scope::Workspace;
    a.widen_search_scope();
    assert!(a.search.is_some());
    // No box: nothing happens.
    a.search = None;
    a.widen_search_scope();
}

fn paste_op(target: &Path, cut: bool, queue: &[PathBuf]) -> PasteOp {
    PasteOp {
        target: target.to_path_buf(),
        cut,
        queue: queue.iter().cloned().collect(),
        overwrite_all: false,
        skip_all: false,
        conflict: None,
    }
}

/// T832: a dangling symlink at the destination is a conflict like any other
/// name there, so the paste asks before replacing it; it never creates the
/// file the link points at.
#[cfg(unix)]
#[test]
fn a_dangling_symlink_at_the_paste_destination_asks_first() {
    let dir = temp("r3-paste-dangling");
    let src = dir.join("src");
    let dst = dir.join("dst");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&dst).unwrap();
    std::fs::write(src.join("a"), "new-a").unwrap();
    std::os::unix::fs::symlink(dir.join("planted"), dst.join("a")).unwrap();
    let all = vec![src.join("a")];
    let mut a = app_at(&dir);
    a.paste = Some(paste_op(&dst, false, &all));
    a.process_paste();
    assert_eq!(
        a.paste.as_ref().unwrap().conflict.as_deref(),
        Some(all[0].as_path())
    );
    a.paste_key(k(KeyCode::Char('o')));
    assert!(
        !dir.join("planted").exists(),
        "the link's target was created"
    );
    assert!(std::fs::symlink_metadata(dst.join("a")).unwrap().is_file());
    assert_eq!(std::fs::read_to_string(dst.join("a")).unwrap(), "new-a");
}

/// Paste conflict handling: `o` overwrites one, `O` all, `s` skips one,
/// `S` all, `c` cancels; every choice resumes the queue.
#[test]
fn paste_conflicts_overwrite_skip_and_cancel() {
    let dir = temp("r3-paste");
    let src = dir.join("src");
    let dst = dir.join("dst");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&dst).unwrap();
    for n in ["a", "b", "c", "d"] {
        std::fs::write(src.join(n), format!("new-{n}")).unwrap();
        std::fs::write(dst.join(n), format!("old-{n}")).unwrap();
    }
    let all: Vec<PathBuf> = ["a", "b", "c", "d"].iter().map(|n| src.join(n)).collect();
    let mut a = app_at(&dir);
    a.paste = Some(paste_op(&dst, false, &all));
    a.process_paste();
    // Stopped on the first conflict.
    assert_eq!(
        a.paste.as_ref().unwrap().conflict.as_deref(),
        Some(all[0].as_path())
    );
    a.paste_key(k(KeyCode::Char('o')));
    assert_eq!(std::fs::read_to_string(dst.join("a")).unwrap(), "new-a");
    assert_eq!(
        a.paste.as_ref().unwrap().conflict.as_deref(),
        Some(all[1].as_path())
    );
    a.paste_key(k(KeyCode::Char('s')));
    assert_eq!(std::fs::read_to_string(dst.join("b")).unwrap(), "old-b");
    assert_eq!(
        a.paste.as_ref().unwrap().conflict.as_deref(),
        Some(all[2].as_path())
    );
    a.paste_key(k(KeyCode::Char('O')));
    assert_eq!(std::fs::read_to_string(dst.join("c")).unwrap(), "new-c");
    assert_eq!(std::fs::read_to_string(dst.join("d")).unwrap(), "new-d");
    assert!(a.paste.is_none());
    assert_eq!(a.status, t!("status.paste_complete").to_string());
    // Skip all.
    std::fs::write(dst.join("c"), "old-c").unwrap();
    std::fs::write(dst.join("d"), "old-d").unwrap();
    a.paste = Some(paste_op(&dst, false, &all[2..]));
    a.process_paste();
    a.paste_key(k(KeyCode::Char('S')));
    assert_eq!(std::fs::read_to_string(dst.join("d")).unwrap(), "old-d");
    assert!(a.paste.is_none());
    // Cancel; an unknown key does nothing.
    a.paste = Some(paste_op(&dst, false, &all));
    a.process_paste();
    a.paste_key(k(KeyCode::Char('x')));
    assert!(a.paste.is_some());
    a.paste_key(k(KeyCode::Esc));
    assert!(a.paste.is_none());
    assert_eq!(a.status, t!("status.paste_cancelled").to_string());
    // 'o' and 's' with no paste in progress just resume nothing.
    a.paste_key(k(KeyCode::Char('o')));
    a.paste_key(k(KeyCode::Char('s')));
    a.paste_key(k(KeyCode::Char('S')));
    a.paste_key(k(KeyCode::Char('O')));
}

/// Copying into the source's own directory makes "name copy"; cutting there
/// is a no-op; a failing copy is reported; a cut re-points open buffers.
#[test]
fn paste_into_the_same_directory_and_failures() {
    let dir = temp("r3-paste2");
    let f = dir.join("note.txt");
    std::fs::write(&f, "hello").unwrap();
    let mut a = app_at(&dir);
    a.paste = Some(paste_op(&dir, false, std::slice::from_ref(&f)));
    a.process_paste();
    let copies: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("note") && n != "note.txt")
        .collect();
    assert_eq!(copies.len(), 1, "{copies:?}");
    assert_eq!(
        std::fs::read_to_string(dir.join(&copies[0])).unwrap(),
        "hello"
    );
    // Cut into the same directory: nothing moves and the clipboard clears.
    a.flags.insert(AppFlags::CLIP_CUT);
    a.clip = vec![f.clone()];
    a.paste = Some(paste_op(&dir, true, std::slice::from_ref(&f)));
    a.process_paste();
    assert!(f.exists());
    assert_eq!(a.clip.len(), 0);
    assert!(!a.flags.contains(AppFlags::CLIP_CUT));
    // A source that no longer exists fails loudly.
    let gone = dir.join("gone.txt");
    let sub = dir.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    let errors = |a: &App| {
        a.messages
            .items
            .iter()
            .filter(|m| matches!(m.level, Level::Error))
            .count()
    };
    let before = errors(&a);
    a.paste = Some(paste_op(&sub, false, std::slice::from_ref(&gone)));
    a.process_paste();
    // Not the translated text: the locale is process-global.
    assert!(errors(&a) > before);
    // Moving re-points an open buffer at the new path.
    a.open_path(&f, false);
    a.paste = Some(paste_op(&sub, true, std::slice::from_ref(&f)));
    a.process_paste();
    assert!(!f.exists());
    assert_eq!(
        a.active_path().as_deref(),
        Some(sub.join("note.txt").as_path())
    );
    // A directory move re-points buffers under it.
    let deep = sub.join("deep");
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::write(deep.join("x.txt"), "x").unwrap();
    a.open_path(&deep.join("x.txt"), false);
    let canon_deep = deep.canonicalize().unwrap();
    a.relocate_buffers(&canon_deep, &dir.join("moved"));
    assert_eq!(
        a.active_path().as_deref(),
        Some(dir.join("moved/x.txt").as_path())
    );
    // Closing buffers under a deleted path drops them and keeps one tab.
    a.close_buffers_under(&dir.join("moved"));
    assert!(a.editor.active < a.editor.tabs.len());
    a.close_buffers_under(&sub);
    assert!(!a.editor.tabs.is_empty());
}

/// Explorer clipboard keys: copy, cut, cancel the cut, paste with nothing on
/// the clipboard, and the navigation keys.
#[test]
fn explorer_clipboard_and_navigation_keys() {
    let dir = temp("r3-explorer");
    std::fs::write(dir.join("one.txt"), "1").unwrap();
    std::fs::write(dir.join("two.txt"), "2").unwrap();
    let mut a = app_at(&dir);
    a.explorer.rebuild();
    a.explorer_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL));
    assert!(a.paste.is_none());
    a.explorer_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert_eq!(a.status, t!("status.copied_n", n = 1).to_string());
    a.explorer_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL));
    assert_eq!(a.status, t!("status.cut_n", n = 1).to_string());
    assert!(a.flags.contains(AppFlags::CLIP_CUT));
    a.explorer_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL));
    a.explorer_key(k(KeyCode::Esc));
    assert_eq!(a.status, t!("status.cut_cancelled").to_string());
    assert_eq!(a.clip.len(), 0);
    a.explorer_key(k(KeyCode::Home));
    a.explorer_key(k(KeyCode::End));
    a.explorer_key(k(KeyCode::PageUp));
    a.explorer_key(k(KeyCode::PageDown));
    a.explorer_key(k(KeyCode::Left));
    a.explorer_key(k(KeyCode::Char('?')));
    a.explorer_key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT));
    a.explorer_key(KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT));
    a.explorer_key(k(KeyCode::Down));
    a.explorer_key(k(KeyCode::Up));
    // Escape with marks clears them; with none, hands focus to the editor.
    a.explorer_key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT));
    a.explorer_key(k(KeyCode::Esc));
    assert!(a.explorer.marked.is_empty());
    a.focus = Focus::Explorer;
    a.explorer_key(k(KeyCode::Esc));
    assert!(matches!(a.focus, Focus::Editor));
    // Delete with an empty selection asks nothing.
    a.explorer.clear_marks();
    a.explorer_key(k(KeyCode::Delete));
}

/// Jump-to-line mode: a label jumps, Esc and unknown characters cancel.
#[test]
fn jump_mode_jumps_on_a_label_and_cancels_otherwise() {
    let text: String = (1..=40)
        .map(|n| ["line ", &n.to_string(), "\n"].concat())
        .collect();
    let mut a = with_text("r3-jump", &text);
    a.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 40);
    assert!(!a.jump_key(k(KeyCode::Char('a'))));
    a.open_jump();
    assert!(a.jump.as_ref().is_some_and(|j| j.labels.len() == 40));
    assert!(a.jump_key(k(KeyCode::Esc)));
    assert!(a.jump.is_none());
    a.open_jump();
    assert!(a.jump_key(k(KeyCode::Char('1'))));
    assert!(a.jump.is_none());
    assert_eq!(a.editor.cursor_1based().0, 1);
    a.open_jump();
    assert!(a.jump_key(k(KeyCode::Char('c'))));
    assert_eq!(a.editor.cursor_1based().0, 3);
    a.layout.editor.height = 0;
    a.open_jump();
    assert!(a.jump.is_none());
}

/// Clicking or dragging the minimap and scrollbar moves the cursor along the
/// track; a collapsed track does nothing; the horizontal bars set offsets.
#[test]
fn minimap_scrollbar_and_hbar_drag_move_the_view() {
    use ratatui::layout::Rect;
    let text: String = (1..=100)
        .map(|n| ["line ", &n.to_string(), "\n"].concat())
        .collect();
    let mut a = with_text("r3-bars", &text);
    a.minimap_click(5);
    a.scrollbar_drag(5);
    assert_eq!(a.editor.cursor_1based().0, 1);
    a.layout.minimap = Rect::new(70, 0, 10, 20);
    a.minimap_click(10);
    assert_eq!(a.editor.cursor_1based().0, 51);
    a.layout.scrollbar = Rect::new(79, 0, 1, 11);
    a.scrollbar_drag(10);
    assert_eq!(a.editor.cursor_1based().0, a.editor.active_line_count());
    a.scrollbar_drag(0);
    assert_eq!(a.editor.cursor_1based().0, 1);
    a.layout.explorer_hscrollbar = Rect::new(0, 0, 11, 1);
    a.layout.messages_hscrollbar = Rect::new(0, 0, 11, 1);
    a.layout.bottom_hscrollbar = Rect::new(0, 0, 11, 1);
    a.layout.editor_hscrollbar = Rect::new(0, 0, 11, 1);
    a.explorer_hmax = 10;
    a.messages_hmax = 10;
    a.bottom_hmax = 10;
    a.editor_hmax = 10;
    a.hbar_drag(HBar::Explorer, 10);
    a.hbar_drag(HBar::Messages, 10);
    a.hbar_drag(HBar::Bottom, 10);
    a.hbar_drag(HBar::Editor, 10);
    assert_eq!(a.explorer_hscroll, 10);
    assert_eq!(a.messages_hscroll, 10);
    assert_eq!(a.bottom_hscroll, 10);
    assert_eq!(a.editor.active_tab().unwrap().editor.get_offset_x(), 10);
}

/// Dragging the dock edges clamps to the minimum dock and keeps room for the
/// editor; a drag with nothing grabbed does nothing.
#[test]
fn dock_resize_clamps_the_widths_and_heights() {
    use ratatui::layout::Rect;
    let mut a = app_at(&temp("r3-dock"));
    a.layout.menu = Rect::new(0, 0, 100, 1);
    a.layout.explorer = Rect::new(0, 1, 30, 20);
    a.layout.messages = Rect::new(70, 1, 30, 20);
    a.layout.bottom_dock = Rect::new(0, 21, 100, 5);
    a.dock_resize = None;
    let before = (a.settings.explorer_width, a.settings.messages_width);
    a.resize_dock(40);
    assert_eq!(
        (a.settings.explorer_width, a.settings.messages_width),
        before
    );
    a.dock_resize = Some(DockResize::Bottom);
    a.resize_dock(40);
    a.visible.set(Visible::MESSAGES, false);
    a.visible.set(Visible::EXPLORER, false);
    a.dock_resize = Some(DockResize::Left);
    a.resize_dock(5);
    assert_eq!(a.settings.explorer_width, 12);
    a.resize_dock(95);
    assert_eq!(a.settings.explorer_width, 80);
    a.dock_resize = Some(DockResize::Right);
    a.resize_dock(95);
    assert_eq!(a.settings.messages_width, 12);
    a.resize_dock(0);
    assert_eq!(a.settings.messages_width, 80);
    a.resize_bottom_dock(0);
    assert_eq!(a.settings.bottom_dock_height, 22);
    a.resize_bottom_dock(25);
    assert_eq!(a.settings.bottom_dock_height, 3);
}

/// Esc backs out of a menu one level at a time; the scrollbar toggle flips
/// and reports; an unknown locale code is ignored.
#[test]
fn menu_escape_levels_scrollbar_toggle_and_unknown_locale() {
    let mut a = app_at(&temp("r3-menu"));
    a.menu.open_index(0);
    a.menu.sub = Some(0);
    a.menu.subsub_open = true;
    a.menu.subsub = Some(0);
    a.menu_key(k(KeyCode::Esc));
    assert!(!a.menu.subsub_open && a.menu.subsub.is_none());
    assert!(a.menu.sub.is_some());
    a.menu_key(k(KeyCode::Esc));
    assert!(a.menu.sub.is_none());
    assert!(a.menu.is_open());
    a.menu_key(k(KeyCode::Esc));
    assert!(!a.menu.is_open());
    // Enter with nothing highlighted does not close or run anything.
    a.menu.open_index(0);
    a.menu_key(k(KeyCode::Right));
    a.menu_key(k(KeyCode::Left));
    a.menu_key(k(KeyCode::Down));
    a.menu_key(k(KeyCode::Up));

    let was = a.visible.contains(Visible::SCROLLBAR);
    a.toggle_scrollbar();
    assert_eq!(a.visible.contains(Visible::SCROLLBAR), !was);
    a.toggle_scrollbar();
    assert_eq!(a.visible.contains(Visible::SCROLLBAR), was);
    assert_eq!(a.settings.viewport.show_scrollbar, was);
    let locale = a.settings.locale.clone();
    a.run_action("view.locale:zz-not-a-locale");
    assert_eq!(a.settings.locale, locale);
    // A theme preview of a non-theme action does nothing.
    a.preview_menu_theme("file.save");
    assert!(a.theme_preview.is_none());
    a.preview_menu_theme("view.theme:no-such-theme");
    assert!(a.theme_preview.is_none());
}

/// Clicking the status-bar git indicator opens the Git panel action; outside
/// a repository it says so.
#[cfg(feature = "git")]
#[test]
fn clicking_the_git_status_bar_runs_the_git_changes_action() {
    use ratatui::layout::Rect;
    let mut a = app_at(&temp("r3-gitbar"));
    a.layout.git_status_bar = Rect::new(0, 23, 10, 1);
    a.dispatch_mouse(crate::app::cov4::click(2, 23));
    assert_eq!(a.status, t!("status.git_not_repo").to_string());
}

/// A split editor is recorded in the session and rebuilt on restore, with the
/// focused pane and the cursor positions; the restore reports its count.
#[test]
fn a_split_workspace_round_trips_through_the_session() {
    let dir = temp("r3-session-split");
    let (a_txt, b_txt) = (dir.join("a.txt"), dir.join("b.txt"));
    std::fs::write(&a_txt, "alpha line\nsecond\n").unwrap();
    std::fs::write(&b_txt, "beta line\nsecond\n").unwrap();
    let mut first = app_at(&dir);
    first.open_path(&a_txt, false);
    first.open_path(&b_txt, false);
    first.editor.active_tab_mut().unwrap().editor.set_cursor(4);
    first.run_action("view.split_vertical");
    assert!(first.editor.is_split());
    let ws = first.workspace_session();
    let split = ws.split.as_ref().expect("the split is recorded");
    assert_eq!(split.focused, first.editor.focused_leaf);
    assert_eq!(ws.files.len(), 2);

    let mut second = app_at(&dir);
    let opened = second.apply_session(&ws);
    assert_eq!(opened, 2);
    assert!(second.editor.is_split());
    assert_eq!(second.editor.focused_leaf, first.editor.focused_leaf);
    assert!(second.editor.tabs.iter().all(|t| t.path.is_some()));
    // The blank starting tab was dropped in favor of the real files.
    assert_eq!(second.editor.tabs.len(), 2);
    // A recorded split that no longer has two panes is ignored.
    let mut flat = ws.clone();
    flat.split = Some(crate::session::SplitSession {
        tree: crate::session::PaneNode::Leaf(0),
        focused: 0,
    });
    let mut third = app_at(&dir);
    assert_eq!(third.apply_session(&flat), 2);
    assert!(!third.editor.is_split());
    // Files that have vanished are skipped.
    std::fs::remove_file(&a_txt).unwrap();
    let mut fourth = app_at(&dir);
    assert_eq!(fourth.apply_session(&ws), 1);
    assert!(!fourth.editor.is_split());
    // restore_session without a saved session or with restore off does nothing.
    let mut off = app_at(&dir);
    off.settings.startup.restore_session = false;
    off.restore_session();
    assert!(off.editor.tabs.iter().all(|t| t.path.is_none()));
}

/// The settings file path is unknown for an App without a config dir; the
/// workspace chooser wraps its selection both ways.
#[test]
fn workspace_chooser_keys_wrap_and_close() {
    let dir = temp("r3-ws-chooser");
    let mut a = app_at(&dir);
    a.chooser = Some(crate::app::chooser::Chooser::Workspace(WorkspaceChooser {
        roots: vec!["/one".to_string(), "/two".to_string()],
        selected: 0,
    }));
    a.workspace_chooser_key(k(KeyCode::Up));
    assert_eq!(a.chooser.workspace_mut().unwrap().selected, 1);
    a.workspace_chooser_key(k(KeyCode::Down));
    assert_eq!(a.chooser.workspace_mut().unwrap().selected, 0);
    a.workspace_chooser_key(k(KeyCode::Esc));
    assert!(a.chooser.workspace_mut().is_none());
    // With the chooser gone, Up/Down are ignored.
    a.workspace_chooser_key(k(KeyCode::Up));
    a.workspace_chooser_key(k(KeyCode::Down));
}

fn word_marks(a: &App) -> Vec<(usize, usize)> {
    a.editor
        .active_tab()
        .unwrap()
        .editor
        .word_marks()
        .cloned()
        .unwrap_or_default()
}

/// Dynamic abbreviation: completes from buffer words, cycles both ways, and
/// reports when there is nothing to complete.
#[test]
fn autocomplete_cycles_through_buffer_words() {
    let mut a = with_text("r3-autocomplete", "foobar foobaz fo");
    let end = "foobar foobaz fo".chars().count();
    a.editor.active_tab_mut().unwrap().editor.set_cursor(end);
    a.autocomplete(true);
    assert_eq!(content_of(&a), "foobar foobaz foobar");
    a.autocomplete(true);
    assert_eq!(content_of(&a), "foobar foobaz foobaz");
    a.autocomplete(true);
    assert_eq!(content_of(&a), "foobar foobaz foobar");
    a.autocomplete(false);
    assert_eq!(content_of(&a), "foobar foobaz foobaz");
    // A non-word prefix has nothing to complete.
    a.complete_session = None;
    a.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_content("foo ");
    a.editor.active_tab_mut().unwrap().editor.set_cursor(4);
    a.autocomplete(true);
    assert_eq!(a.status, t!("status.no_completions").to_string());
    // A prefix with no longer word.
    a.status.clear();
    a.editor.active_tab_mut().unwrap().editor.set_content("zzz");
    a.editor.active_tab_mut().unwrap().editor.set_cursor(3);
    a.autocomplete(true);
    assert_eq!(a.status, t!("status.no_completions").to_string());
    a.editor.tabs.clear();
    a.autocomplete(true);
    assert!(a.editor.tabs.is_empty());
}

fn content_of(a: &App) -> String {
    a.editor.active_tab().unwrap().text()
}

/// Auto-save writes the dirty file-backed buffer at most every few seconds.
#[test]
fn auto_save_writes_a_dirty_file_once_per_interval() {
    let dir = temp("r3-autosave");
    let f = dir.join("n.txt");
    std::fs::write(&f, "old").unwrap();
    let mut a = app_at(&dir);
    a.open_path(&f, false);
    a.settings.editor_behavior.auto_save = crate::settings::AutoSave::Off;
    a.editor.active_tab_mut().unwrap().editor.set_content("new");
    a.editor.active_tab_mut().unwrap().dirty = true;
    a.poll_auto_save();
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "old");
    a.settings.editor_behavior.auto_save = crate::settings::AutoSave::Interval;
    a.poll_auto_save();
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "new\n");
    assert_eq!(a.status, t!("status.auto_saved").to_string());
    // Within the interval a second poll does nothing even if dirty again.
    a.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_content("newer");
    a.editor.active_tab_mut().unwrap().dirty = true;
    a.poll_auto_save();
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "new\n");
    // An untitled buffer is never auto-saved.
    a.last_auto_save = None;
    a.editor.new_tab_with_content("scratch");
    a.editor.active_tab_mut().unwrap().dirty = true;
    a.status.clear();
    a.poll_auto_save();
    assert_eq!(a.status, "");
}

/// Highlight Word marks whole-word repeats, clears off a word, ignores a
/// lone occurrence and caps the marks.
#[test]
fn word_highlight_marks_repeats_and_clears_elsewhere() {
    let mut a = with_text("r3-wordhl", "foo bar foo foobar foo");
    a.settings.editor_visual.highlight_word = false;
    a.refresh_word_highlight();
    assert!(a.word_highlight_key.is_none());
    a.settings.editor_visual.highlight_word = true;
    a.editor.active_tab_mut().unwrap().editor.set_cursor(1);
    a.refresh_word_highlight();
    assert_eq!(word_marks(&a), vec![(0, 3), (8, 11), (19, 22)]);
    // Same key: nothing recomputed.
    a.refresh_word_highlight();
    // Cursor between two spaces: the marks are cleared.
    a.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_content("foo  foo");
    a.editor.active_tab_mut().unwrap().editor.set_cursor(4);
    a.refresh_word_highlight();
    assert_eq!(word_marks(&a).len(), 0);
    a.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_content("foo bar foo foobar foo");
    // A word that occurs once is not worth marking.
    a.editor.active_tab_mut().unwrap().editor.set_cursor(5);
    a.refresh_word_highlight();
    assert_eq!(word_marks(&a).len(), 0);
    // The marks are capped at 500.
    let many = "ab ".repeat(700);
    a.editor.active_tab_mut().unwrap().editor.set_content(&many);
    a.editor.active_tab_mut().unwrap().editor.set_cursor(0);
    a.refresh_word_highlight();
    assert_eq!(word_marks(&a).len(), 500);
}

/// Point the active tab's viewport at `top` without moving the cursor.
fn scroll_to(a: &mut App, top: usize) {
    a.editor.active_tab_mut().unwrap().editor.set_offset_y(top);
}

/// Whether `marks` is sorted by position, as the renderer gets it.
fn sorted(marks: &[(usize, usize)]) -> bool {
    marks.windows(2).all(|w| w[0] < w[1])
}

/// T852: a word repeated 10,000 times, cursor on the last one: the 500
/// marks are on and around the visible rows at the end of the file, not
/// the first 500 in the buffer, both once the view has scrolled there and
/// on the tick before (the cursor's band).
#[test]
fn word_highlight_marks_the_occurrences_on_screen() {
    let mut a = with_text("t852-screen", &"foo\n".repeat(10_000));
    a.settings.editor_visual.highlight_word = true;
    let last = 4 * 9_999;
    a.editor.active_tab_mut().unwrap().editor.set_cursor(last);
    // Before the draw scrolls to the cursor: the view is still at the top.
    a.refresh_word_highlight();
    let marks = word_marks(&a);
    assert_eq!(marks.len(), 500);
    assert!(sorted(&marks));
    assert!(marks.contains(&(last, last + 3)), "the cursor's own word");
    // The view at the bottom (24 rows): every visible occurrence is marked.
    scroll_to(&mut a, 10_001 - 24);
    a.refresh_word_highlight();
    let marks = word_marks(&a);
    assert_eq!(marks.len(), 500, "the cap holds");
    assert!(sorted(&marks));
    for line in (10_001 - 24)..10_000 {
        assert!(marks.contains(&(4 * line, 4 * line + 3)), "line {line}");
    }
    assert!(
        marks[0].0 > 4 * 9_000,
        "nothing from the top: {:?}",
        marks[0]
    );
}

/// T852: scrolling without moving the cursor moves the marks onto the new
/// view; an unchanged view and cursor rebuild nothing.
#[test]
fn word_highlight_follows_the_view_when_scrolling() {
    let mut a = with_text("t852-scroll", &"foo\n".repeat(10_000));
    a.settings.editor_visual.highlight_word = true;
    a.editor.active_tab_mut().unwrap().editor.set_cursor(0);
    a.refresh_word_highlight();
    let marks = word_marks(&a);
    assert_eq!(marks.len(), 500);
    assert_eq!(marks[0], (0, 3));
    assert!(!marks.contains(&(4 * 5_000, 4 * 5_000 + 3)));
    let key = a.word_highlight_key;
    a.refresh_word_highlight();
    assert_eq!(a.word_highlight_key, key, "nothing changed: no rebuild");
    scroll_to(&mut a, 5_000);
    a.refresh_word_highlight();
    assert_ne!(a.word_highlight_key, key);
    let marks = word_marks(&a);
    assert_eq!(marks.len(), 500);
    assert!(sorted(&marks));
    for line in 5_000..5_024 {
        assert!(marks.contains(&(4 * line, 4 * line + 3)), "line {line}");
    }
    assert!(
        marks.contains(&(0, 3)),
        "the cursor's band is still searched"
    );
}

/// T852: Highlight Word looks a bounded number of lines past the view: a
/// word whose other occurrence is farther away than that is marked
/// nowhere (it shows as found once), one within reach is marked.
#[test]
fn word_highlight_looks_a_bounded_distance_from_the_view() {
    let filler = "x\n".repeat(10_000);
    let text = format!("needle\n{filler}needle\npin\n{}pin\n", "x\n".repeat(100));
    let mut a = with_text("t852-reach", &text);
    a.settings.editor_visual.highlight_word = true;
    let second = text.rfind("needle").unwrap();
    a.editor.active_tab_mut().unwrap().editor.set_cursor(second);
    scroll_to(&mut a, 10_001 - 10);
    a.refresh_word_highlight();
    assert_eq!(
        word_marks(&a),
        Vec::new(),
        "the first needle is out of reach"
    );
    let pin = text.find("pin").unwrap();
    a.editor.active_tab_mut().unwrap().editor.set_cursor(pin);
    a.refresh_word_highlight();
    let far = text.rfind("pin").unwrap();
    assert_eq!(word_marks(&a), vec![(pin, pin + 3), (far, far + 3)]);
}

/// An outside edit reloads a clean buffer; an unsaved one only warns; a file
/// that vanished is skipped; polls are throttled.
#[test]
fn external_changes_reload_clean_buffers_and_warn_on_dirty_ones() {
    let dir = temp("r3-extchange");
    let (clean, dirty, gone) = (dir.join("c.txt"), dir.join("d.txt"), dir.join("g.txt"));
    for f in [&clean, &dirty, &gone] {
        std::fs::write(f, "v1").unwrap();
    }
    let mut a = app_at(&dir);
    for f in [&clean, &dirty, &gone] {
        a.open_path(f, false);
    }
    a.poll_file_changes(); // records the mtimes
    // Throttled: an immediate second poll does nothing.
    a.poll_file_changes();
    let bump = |f: &Path, text: &str| {
        std::fs::write(f, text).unwrap();
        let file = std::fs::File::options().write(true).open(f).unwrap();
        file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(60))
            .unwrap();
    };
    bump(&clean, "v2");
    bump(&dirty, "v2");
    std::fs::remove_file(&gone).unwrap();
    let dirty_tab = a
        .editor
        .tabs
        .iter_mut()
        .find(|t| t.path.as_deref() == Some(dirty.as_path()))
        .unwrap();
    dirty_tab.editor.set_content("mine");
    dirty_tab.dirty = true;
    a.last_disk_poll = None;
    a.poll_file_changes();
    let text_of = |a: &App, p: &Path| {
        a.editor
            .tabs
            .iter()
            .find(|t| t.path.as_deref() == Some(p))
            .unwrap()
            .text()
    };
    assert_eq!(text_of(&a, &clean), "v2");
    assert_eq!(text_of(&a, &dirty), "mine");
    assert_eq!(
        a.status,
        t!("status.reloaded_external", count = 1).to_string()
    );
    assert!(
        a.messages
            .items
            .iter()
            .any(|m| m.text == t!("msg.external_change_unsaved", path = dirty.display()))
    );
}

/// Goto prompts: a non-number is ignored; word/percent/byte jumps land on
/// the described offset.
#[test]
fn goto_number_prompts_jump_or_ignore_bad_input() {
    let mut a = with_text("r3-gotonum", "one two three\nfour five\n\nsix\n");
    let cursor = |a: &App| a.editor.active_tab().unwrap().editor.get_cursor();
    a.accept_goto_number(PromptKind::GotoWord, "abc");
    assert_eq!(cursor(&a), 0);
    a.accept_goto_number(PromptKind::GotoWord, "2");
    assert_eq!(cursor(&a), 4);
    a.accept_goto_number(PromptKind::GotoByte, "10");
    assert_eq!(cursor(&a), 10);
    a.accept_goto_number(PromptKind::GotoPercent, "0%");
    assert_eq!(cursor(&a), 0);
    a.accept_goto_number(PromptKind::GotoParagraph, "2");
    assert_eq!(a.editor.cursor_1based().0, 4);
    a.accept_goto_number(PromptKind::GotoSentence, "1");
    a.accept_goto_number(PromptKind::GotoSection, "1");
}

/// The project TODO scan stops at 2000 tags; definition search stops at 200
/// hits; very large files are not read for a search.
#[test]
fn project_scans_are_capped() {
    let dir = temp("r3-scan-caps");
    let todos: String = (0..2100)
        .map(|i| ["// TODO: item ", &i.to_string(), "\n"].concat())
        .collect();
    std::fs::write(dir.join("todos.rs"), todos).unwrap();
    let defs: String = (0..260).map(|_| "fn thing() {}\n".to_string()).collect();
    std::fs::write(dir.join("defs.rs"), defs).unwrap();
    let mut a = app_at(&dir);
    a.build_file_index();
    assert_eq!(a.collect_todos().len(), 2000);
    assert_eq!(a.find_definitions("thing").len(), 200);
    // A definition keyword followed by another name finds nothing.
    assert!(a.find_definitions("absent_symbol").is_empty());
    let big = dir.join("big.txt");
    std::fs::write(&big, vec![b'x'; 2_000_100]).unwrap();
    assert_eq!(a.current_text(&big), None);
    // An open buffer wins over the file on disk, unsaved edits included.
    let small = dir.join("small.txt");
    std::fs::write(&small, "disk").unwrap();
    assert_eq!(a.current_text(&small).as_deref(), Some("disk"));
    a.open_path(&small, false);
    a.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_content("buffer");
    assert_eq!(a.current_text(&small).as_deref(), Some("buffer"));
    assert_eq!(a.current_text(&dir.join("missing.txt")), None);
}

/// Auto-pair toggles for every tab; the closed-tab stack is capped and
/// de-duplicated; recents tolerate a path that does not exist.
#[test]
fn auto_pair_closed_tabs_and_recents() {
    let dir = temp("r3-misc-state");
    let mut a = app_at(&dir);
    let was = a.settings.typing.auto_pair;
    a.toggle_auto_pair();
    assert_eq!(a.settings.typing.auto_pair, !was);
    assert_eq!(
        a.status,
        t!(if was {
            "status.auto_pair_off"
        } else {
            "status.auto_pair_on"
        })
        .to_string()
    );
    a.toggle_auto_pair();
    assert_eq!(a.settings.typing.auto_pair, was);
    for i in 0..25 {
        a.push_closed_tab(dir.join(format!("f{i}")));
    }
    assert_eq!(a.closed_tabs.len(), 20);
    assert_eq!(a.closed_tabs.first(), Some(&dir.join("f5")));
    a.push_closed_tab(dir.join("f10"));
    assert_eq!(a.closed_tabs.last(), Some(&dir.join("f10")));
    assert_eq!(a.closed_tabs.len(), 20);
    // None of those files exist, so nothing reopens.
    a.reopen_closed_tab();
    assert_eq!(a.status, t!("status.no_closed_tab").to_string());
    assert_eq!(a.closed_tabs.len(), 0);
    let ghost = dir.join("ghost.txt");
    a.record_recent(&ghost);
    assert_eq!(
        a.settings.recent_files.first(),
        Some(&ghost.to_string_lossy().into_owned())
    );
}

/// Rename: nothing to rename, an empty or unchanged name, an existing target
/// and a failing move are all reported without changing the buffer's path.
#[test]
fn rename_file_reports_each_refusal_and_succeeds_when_free() {
    let dir = temp("r3-rename");
    let f = dir.join("old.txt");
    std::fs::write(&f, "x").unwrap();
    std::fs::write(dir.join("taken.txt"), "y").unwrap();
    let mut a = app_at(&dir);
    a.rename_file("whatever.txt");
    assert_eq!(a.status, t!("status.rename_no_file").to_string());
    a.open_path(&f, false);
    a.rename_file("   ");
    a.rename_file("old.txt");
    assert_eq!(a.active_path().as_deref(), Some(f.as_path()));
    a.rename_file("taken.txt");
    assert_eq!(
        a.status,
        t!(
            "status.rename_exists",
            name = dir.join("taken.txt").display()
        )
        .to_string()
    );
    a.rename_file("no/such/dir/new.txt");
    assert!(
        a.messages
            .items
            .last()
            .is_some_and(|m| m.text.contains("new.txt"))
    );
    assert!(f.exists());
    a.rename_file("fresh.txt");
    assert!(!f.exists());
    assert!(dir.join("fresh.txt").exists());
    assert_eq!(
        a.active_path().as_deref(),
        Some(dir.join("fresh.txt").as_path())
    );
    assert_eq!(
        a.status,
        t!("status.renamed", path = dir.join("fresh.txt").display()).to_string()
    );
}

/// On an image tab every text-editing action declines quietly.
#[cfg(feature = "image")]
#[test]
fn text_actions_decline_on_an_image_tab() {
    let dir = temp("r3-imgtab");
    let mut a = app_at(&dir);
    let picker = ratatui_image::picker::Picker::halfblocks();
    let proto = picker.new_resize_protocol(image::DynamicImage::new_rgb8(4, 4));
    a.editor.open_image(&dir.join("pic.png"), proto);
    assert!(a.editor.active_tab().unwrap().is_image());
    assert_eq!(a.breadcrumb(), "pic.png");
    assert_eq!(a.symbol_under_cursor(), None);
    a.change_case(crate::case::upper);
    assert_eq!(a.status, t!("status.no_selection").to_string());
    a.status.clear();
    a.transform_selection_or_buffer(str::to_uppercase);
    assert_eq!(a.status, t!("status.no_selection").to_string());
    a.rewrite_at_cursor(|t, c| Some((t.to_uppercase(), c)), None);
    a.revert_active();
    a.comment_banner();
    let warn = |a: &App| a.messages.items.last().map(|m| m.text.clone());
    for (id, key) in [
        ("tools.edit_table", "msg.edit_table_no_buffer"),
        ("tools.edit_outline", "msg.edit_outline_no_buffer"),
        ("tools.edit_json", "msg.edit_value_no_buffer"),
        ("tools.edit_bytes", "msg.edit_bytes_no_buffer"),
    ] {
        a.run_action(id);
        assert_eq!(warn(&a), Some(t!(key).to_string()), "{id}");
    }
    #[cfg(feature = "sql")]
    {
        a.run_action("tools.edit_sql");
        assert_eq!(warn(&a), Some(t!("msg.edit_sql_no_buffer").to_string()));
    }
    assert!(a.editor.active_tab().unwrap().is_image());
}

/// Comment banner: the line becomes a ruled title (an empty line is titled
/// "Section"), and a read-only buffer is refused.
#[test]
fn comment_banner_wraps_the_line_and_respects_read_only() {
    let mut a = with_text("r3-banner", "  Title here\n");
    a.comment_banner();
    assert_eq!(
        a.editor.active_tab().unwrap().text(),
        "  // ==========\n  // Title here\n  // ==========\n"
    );
    let mut b = with_text("r3-banner2", "\n");
    b.comment_banner();
    assert!(b.editor.active_tab().unwrap().text().contains("// Section"));
    let mut c = with_text("r3-banner3", "x\n");
    c.toggle_read_only();
    c.comment_banner();
    assert_eq!(c.status, t!("status.read_only_blocked").to_string());
    assert_eq!(c.editor.active_tab().unwrap().text(), "x\n");
}

/// Reopen/Save with an encoding: the chooser needs a saved file; reopening
/// reports unsaved edits, a vanished file, a lossy decode and success; a
/// failing save keeps the old encoding.
#[test]
fn encoding_actions_report_each_outcome() {
    let dir = temp("r3-encoding");
    let f = dir.join("latin.txt");
    std::fs::write(&f, [b'c', b'a', b'f', 0xe9, b'\n']).unwrap();
    let mut a = app_at(&dir);
    a.open_encoding_chooser(EncodingAction::Reopen);
    assert_eq!(a.status, t!("status.encoding_no_file").to_string());
    a.open_path(&f, false);
    a.open_encoding_chooser(EncodingAction::Reopen);
    let n = crate::encoding::CHOICES.len();
    let initial = a.chooser.encoding_mut().unwrap().selected;
    a.encoding_key(k(KeyCode::Up));
    assert_eq!(
        a.chooser.encoding_mut().unwrap().selected,
        initial.saturating_sub(1)
    );
    a.encoding_key(k(KeyCode::Down));
    assert_eq!(a.chooser.encoding_mut().unwrap().selected, initial);
    a.chooser.encoding_mut().unwrap().selected = n + 3;
    a.encoding_key(k(KeyCode::Enter));
    assert!(
        a.chooser.encoding_mut().is_none(),
        "a bad row just closes it"
    );

    let latin = crate::encoding::TextEncoding::from_label("windows-1252").unwrap();
    let utf8 = crate::encoding::TextEncoding::from_label("UTF-8").unwrap();
    a.reopen_with_encoding(latin);
    assert_eq!(
        a.status,
        t!("status.reopened_encoding", encoding = latin.to_string()).to_string()
    );
    assert_eq!(a.editor.active_tab().unwrap().text(), "caf\u{e9}\n");
    a.reopen_with_encoding(utf8);
    assert_eq!(
        a.status,
        t!(
            "status.reopened_encoding_lossy",
            encoding = utf8.to_string()
        )
        .to_string()
    );
    a.editor.active_tab_mut().unwrap().dirty = true;
    a.reopen_with_encoding(latin);
    assert_eq!(a.status, t!("status.reopen_encoding_unsaved").to_string());
    a.editor.active_tab_mut().unwrap().dirty = false;
    std::fs::remove_file(&f).unwrap();
    a.reopen_with_encoding(latin);
    assert_eq!(a.status, t!("status.encoding_no_file").to_string());
    // A directory in place of the file is an error message.
    std::fs::create_dir(&f).unwrap();
    a.reopen_with_encoding(latin);
    assert!(a.messages.items.last().is_some());

    // Saving in an encoding that cannot be written keeps the old one.
    let mut b = app_at(&dir);
    b.editor.new_tab_with_content("x");
    b.editor.active_tab_mut().unwrap().path = Some(dir.join("no/such/dir/x.txt"));
    b.editor.active_tab_mut().unwrap().dirty = true;
    let before = b.editor.active_tab().unwrap().encoding;
    b.save_with_encoding(latin);
    assert_eq!(b.editor.active_tab().unwrap().encoding, before);
    b.toggle_read_only();
    b.save_with_encoding(latin);
    assert_eq!(b.status, t!("status.read_only_blocked").to_string());
    b.editor.tabs.clear();
    b.save_with_encoding(latin);
}

/// Query replace: an empty query asks for one, a bad pattern is reported,
/// and skip / replace / replace-rest / quit step through the matches.
#[test]
fn query_replace_steps_through_matches() {
    let mut a = with_text("r3-qr", "ab ab ab ab\n");
    a.begin_query_replace(); // no find box
    assert!(a.query_replace.is_none());
    a.run_action("edit.query_replace");
    a.begin_query_replace();
    assert_eq!(
        a.search.as_ref().unwrap().status,
        t!("status.type_to_find").to_string()
    );
    a.search.as_mut().unwrap().query = "(".into();
    a.search.as_mut().unwrap().flags.insert(SearchFlags::REGEX);
    a.begin_query_replace();
    assert!(
        a.search
            .as_ref()
            .unwrap()
            .status
            .starts_with(&t!("msg.bad_regex", error = "").to_string())
    );
    {
        let s = a.search.as_mut().unwrap();
        s.query = "(a)b".into();
        s.replace = "<$1>".into();
    }
    a.begin_query_replace();
    assert!(a.search.is_none());
    assert!(a.query_replace.is_some());
    a.qr_apply(Decision::Skip);
    a.qr_apply(Decision::Replace);
    assert_eq!(a.editor.active_tab().unwrap().text(), "ab <a> ab ab\n");
    a.qr_apply(Decision::ReplaceRest);
    assert!(a.query_replace.is_none());
    assert_eq!(a.editor.active_tab().unwrap().text(), "ab <a> <a> <a>\n");
    assert_eq!(a.status, t!("status.qr_replaced", count = 3).to_string());
    a.qr_apply(Decision::Quit); // nothing running
    // Quit ends the session cleanly.
    a.run_action("edit.query_replace");
    a.search.as_mut().unwrap().query = "ab".into();
    a.editor.active_tab_mut().unwrap().editor.set_cursor(0);
    a.begin_query_replace();
    a.qr_apply(Decision::Quit);
    assert!(a.query_replace.is_none());
    assert_eq!(a.status, t!("status.qr_replaced", count = 0).to_string());
    // A tab-less app cannot begin or advance a session.
    a.run_action("edit.query_replace");
    a.search.as_mut().unwrap().query = "ab".into();
    a.editor.tabs.clear();
    a.begin_query_replace();
    assert!(a.query_replace.is_none());
}

/// Prompts: Esc cancels, Alt+Enter only adds a newline in multi-line prompts,
/// Alt+C/R flip the dock search options, and unknown keys do nothing.
#[test]
fn prompt_keys_edit_cancel_and_toggle_options() {
    let mut a = app_at(&temp("r3-prompt"));
    a.prompt = Some(Prompt::new(PromptKind::SearchToDock, "p".into()));
    a.prompt_key(k(KeyCode::Char('x')));
    a.prompt_key(k(KeyCode::Char('y')));
    a.prompt_key(k(KeyCode::Backspace));
    assert_eq!(a.prompt.as_ref().unwrap().input, "x");
    a.prompt_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::ALT));
    a.prompt_key(KeyEvent::new(KeyCode::Char('R'), KeyModifiers::ALT));
    a.prompt_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::ALT));
    let p = a.prompt.as_ref().unwrap();
    assert!(p.case_sensitive && p.regex);
    a.prompt_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT));
    assert_eq!(a.prompt.as_ref().unwrap().input, "x");
    a.prompt_key(k(KeyCode::Tab));
    a.prompt_key(k(KeyCode::Esc));
    assert!(a.prompt.is_none());
    a.prompt = Some(Prompt::new(PromptKind::GitCommit, "m".into()));
    a.prompt_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT));
    assert_eq!(a.prompt.as_ref().unwrap().input, "\n");
    // Alt-chars on a non-dock prompt change nothing.
    a.prompt_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::ALT));
    assert!(!a.prompt.as_ref().unwrap().case_sensitive);
    // No prompt: editing keys are harmless; accepting nothing does nothing.
    a.prompt = None;
    a.prompt_key(k(KeyCode::Char('q')));
    a.prompt_key(k(KeyCode::Backspace));
    a.prompt_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT));
    a.accept_prompt();
}

#[cfg(all(feature = "tasks", unix))]
mod commands {
    use super::*;

    fn finish(a: &mut App) {
        let start = std::time::Instant::now();
        while a.running_command.is_some() && start.elapsed().as_secs() < 20 {
            a.poll_command();
            std::thread::yield_now();
        }
        assert!(a.running_command.is_none(), "the command finished");
    }

    fn dock(a: &App) -> String {
        a.bottom_dock.lines.join("\n")
    }

    /// A shell command streams into the bottom dock with its exit status; a
    /// second one while the first runs is refused; failures are errors.
    #[test]
    fn commands_stream_into_the_dock_and_report_their_exit() {
        let mut a = app_at(&temp("r3-cmd"));
        a.run_command("   ");
        assert!(a.running_command.is_none());
        a.run_command("echo hello; echo oops >&2");
        assert!(a.visible.contains(Visible::BOTTOM_DOCK));
        // Busy: another command is refused while this one runs.
        a.run_command("echo second");
        assert_eq!(a.status, t!("status.command_busy").to_string());
        finish(&mut a);
        let text = dock(&a);
        assert!(text.contains("$ echo hello"), "{text}");
        assert!(text.contains("hello") && text.contains("oops"), "{text}");
        assert!(text.contains("[exit 0]"));
        assert!(!text.contains("second"));
        assert_eq!(a.status, t!("status.command_done", code = 0).to_string());
        assert!(a.messages.items.last().unwrap().text.contains("echo hello"));

        a.run_command("exit 3");
        finish(&mut a);
        assert!(dock(&a).contains("[exit 3]"));
        assert_eq!(a.status, t!("status.command_done", code = 3).to_string());

        // An authentication failure names the likely cause.
        a.run_command("echo 'fatal: could not read Username'");
        finish(&mut a);
        assert!(dock(&a).contains("could not read Username"));
        a.poll_command(); // nothing running: harmless
    }

    /// The task chooser lists `tasks.toml`, wraps its selection, runs the
    /// highlighted task (also by click) and remembers it for "repeat last".
    #[test]
    fn the_task_chooser_runs_a_named_task_and_repeat_reruns_it() {
        let dir = temp("r3-tasks");
        std::fs::write(
            dir.join("tasks.toml"),
            "[[task]]\nname = \"greet\"\ncommand = \"echo greeting\"\n\n[[task]]\nname = \"bye\"\ncommand = \"echo goodbye\"\n",
        )
        .unwrap();
        let mut a = app_at(&dir);
        a.repeat_last_project_task();
        assert_eq!(a.status, t!("status.project_no_last_command").to_string());
        a.open_tasks();
        assert_eq!(a.chooser.task_mut().unwrap().tasks.len(), 2);
        a.tasks_key(k(KeyCode::Up));
        assert_eq!(a.chooser.task_mut().unwrap().selected, 1);
        a.tasks_key(k(KeyCode::Down));
        assert_eq!(a.chooser.task_mut().unwrap().selected, 0);
        a.tasks_key(k(KeyCode::Char('z')));
        a.tasks_key(k(KeyCode::Esc));
        assert!(a.chooser.task_mut().is_none());
        // A click on a row runs that task.
        a.open_tasks();
        a.layout.chooser = ratatui::layout::Rect::new(0, 0, 30, 5);
        a.tasks_mouse(crate::app::cov4::click(2, 1));
        assert!(a.chooser.task_mut().is_none());
        finish(&mut a);
        assert!(dock(&a).contains("goodbye"), "{}", dock(&a));
        assert_eq!(a.project_last_command.as_deref(), Some("echo goodbye"));
        // Enter runs the highlighted one; repeat runs it again.
        a.open_tasks();
        a.tasks_key(k(KeyCode::Enter));
        finish(&mut a);
        assert_eq!(a.project_last_command.as_deref(), Some("echo greeting"));
        a.bottom_dock.lines.clear();
        a.repeat_last_project_task();
        finish(&mut a);
        assert!(dock(&a).contains("greeting"));
        // A click outside the rows does nothing.
        a.open_tasks();
        a.tasks_mouse(crate::app::cov4::click(2, 40));
        assert!(a.chooser.task_mut().is_some());
    }

    /// The project-command prompt runs the (edited) command and records it; an
    /// empty answer or no pending prompt does nothing.
    #[test]
    fn the_project_command_prompt_runs_and_remembers_the_command() {
        let mut a = app_at(&temp("r3-projcmd"));
        a.accept_project_command_prompt("echo ignored");
        assert!(a.running_command.is_none());
        a.pending_project_command = Some(PendingProjectCommand {
            slot: ProjectSlot::Test,
            dir: None,
        });
        a.accept_project_command_prompt("   ");
        assert!(a.running_command.is_none());
        a.pending_project_command = Some(PendingProjectCommand {
            slot: ProjectSlot::Test,
            dir: None,
        });
        a.accept_project_command_prompt("echo from-prompt");
        finish(&mut a);
        assert!(dock(&a).contains("from-prompt"));
        assert_eq!(a.project_last_command.as_deref(), Some("echo from-prompt"));
        let sub = a.root.clone();
        a.pending_project_command = Some(PendingProjectCommand {
            slot: ProjectSlot::Run,
            dir: Some(sub),
        });
        a.accept_project_command_prompt("echo in-subdir");
        finish(&mut a);
        assert!(dock(&a).contains("in-subdir"));
    }

    /// The test runner captures the command's output and refuses to start over
    /// a running command.
    #[test]
    fn run_tests_captures_output_and_respects_a_running_command() {
        let mut a = app_at(&temp("r3-runtests"));
        a.settings.test_command = "echo running-tests".into();
        a.run_command("sleep 0.2");
        a.run_tests();
        assert_eq!(a.status, t!("status.command_busy").to_string());
        assert!(!a.flags.contains(AppFlags::TEST_CAPTURE));
        finish(&mut a);
        a.run_tests();
        assert!(a.flags.contains(AppFlags::TEST_CAPTURE));
        assert!(a.visible.contains(Visible::TEST_PANEL));
        finish(&mut a);
        assert!(
            !a.flags.contains(AppFlags::TEST_CAPTURE)
                || a.test_buffer.iter().any(|l| l.contains("running-tests"))
        );
        assert!(dock(&a).contains("running-tests"));
    }

    /// The shell filter returns stdout and stderr separately, and a command
    /// that outlives its timeout is killed and reported.
    #[test]
    fn the_shell_filter_collects_output_and_enforces_its_timeout() {
        let dir = temp("r3-filter");
        let out = run_shell_filter(
            "cat; echo err >&2",
            &dir,
            b"piped in",
            std::time::Duration::from_secs(20),
        )
        .unwrap();
        assert!(out.success);
        assert_eq!(out.stdout, b"piped in");
        assert_eq!(out.stderr, b"err\n");
        let failed =
            run_shell_filter("exit 1", &dir, b"", std::time::Duration::from_secs(20)).unwrap();
        assert!(!failed.success);
        let timed = run_shell_filter("sleep 30", &dir, b"", std::time::Duration::from_millis(100));
        let err = timed.err().expect("a timeout is an error");
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
    }

    /// An argv command runs without a shell and busy runs are refused.
    #[cfg(any(feature = "git", feature = "jj", feature = "oc"))]
    #[test]
    fn argv_commands_run_without_a_shell() {
        let mut a = app_at(&temp("r3-argv"));
        let dir = a.root.clone();
        a.run_argv_in(&dir, "echo", &["a b", "$HOME"]);
        a.run_argv_in(&dir, "echo", &["again"]);
        assert_eq!(a.status, t!("status.command_busy").to_string());
        finish(&mut a);
        assert!(dock(&a).contains("a b $HOME"), "{}", dock(&a));
    }
}

/// The contact browser lists `.vcf` files by display name (falling back to the
/// file name), sorted without regard to case, and says so when there are none.
#[test]
fn contacts_are_listed_from_the_configured_directory() {
    let dir = temp("r3-contacts");
    std::fs::write(
        dir.join("a.vcf"),
        "BEGIN:VCARD\nVERSION:3.0\nFN:Zed Zebra\nEND:VCARD\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("b.VCF"),
        "BEGIN:VCARD\nVERSION:3.0\nFN:alice Able\nEND:VCARD\n",
    )
    .unwrap();
    std::fs::write(dir.join("nameless.vcf"), "BEGIN:VCARD\nEND:VCARD\n").unwrap();
    std::fs::write(dir.join("notes.txt"), "not a contact").unwrap();
    let mut a = app_at(&dir);
    a.open_contacts();
    let names: Vec<String> = a
        .contacts
        .as_ref()
        .unwrap()
        .contacts
        .iter()
        .map(|c| c.name.clone())
        .collect();
    assert_eq!(names, ["alice Able", "nameless", "Zed Zebra"]);
    // A configured directory is used instead of the workspace root, and an
    // empty one reports it.
    let empty = temp("r3-contacts-empty");
    let mut b = app_at(&dir);
    b.settings.contacts_dir = format!("  {}  ", empty.display());
    b.open_contacts();
    assert!(b.contacts.as_ref().unwrap().contacts.is_empty());
    assert_eq!(b.status, t!("status.no_contacts").to_string());
}

/// Dashboard metrics arrive through the channel; nothing happens without a
/// panel, or with a panel but no channel.
#[test]
fn dashboard_metrics_are_drained_into_the_open_panel() {
    let mut a = app_at(&temp("r3-dash"));
    let (tx, rx) = std::sync::mpsc::channel();
    a.dashboard_rx = Some(rx);
    a.poll_dashboard();
    assert!(a.dashboard_rx.is_none(), "no panel: the channel is dropped");
    a.dashboard = Some(Dashboard::new(String::from("proj")));
    a.poll_dashboard(); // a panel without a channel: nothing to read
    let (tx2, rx2) = std::sync::mpsc::channel();
    a.dashboard_rx = Some(rx2);
    tx2.send(DashMsg::Disk("12M".into())).unwrap();
    tx2.send(DashMsg::Files(7)).unwrap();
    tx2.send(DashMsg::Commits(3)).unwrap();
    a.poll_dashboard();
    let d = a.dashboard.as_ref().unwrap();
    assert_eq!(d.disk_usage.as_deref(), Some("12M"));
    assert_eq!((d.file_count, d.commit_count), (Some(7), Some(3)));
    drop(tx);
}

const RUST_SRC: &str = "fn alpha() {}\n\nfn beta() {}\n\nstruct Gamma;\n";

/// The document outline: Enter jumps to the symbol and closes it, a click
/// selects and jumps, other keys are ignored, and an empty buffer says so.
#[test]
fn the_outline_panel_jumps_to_symbols() {
    let mut a = with_text("r3-outline", RUST_SRC);
    a.editor.active_tab_mut().unwrap().path = Some(a.root.join("lib.rs"));
    a.layout.outline = ratatui::layout::Rect::new(0, 0, 40, 6);
    a.outline_mouse(crate::app::cov4::click(1, 1)); // no outline open
    a.open_outline();
    assert!(a.outline.is_some());
    a.outline_key(k(KeyCode::Char('x')));
    assert!(a.outline.is_some());
    a.outline_key(k(KeyCode::Down));
    a.outline_key(k(KeyCode::Enter));
    assert!(a.outline.is_none());
    assert_eq!(a.editor.cursor_1based().0, 3);
    a.open_outline();
    a.outline_mouse(crate::app::cov4::click(1, 2));
    assert!(a.outline.is_none());
    assert_eq!(a.editor.cursor_1based().0, 5);
    // A click outside the list, or on a row past its end, keeps it open.
    a.open_outline();
    a.outline_mouse(crate::app::cov4::click(1, 50));
    a.outline_mouse(crate::app::cov4::click(1, 5));
    assert!(a.outline.is_some());
    a.outline_key(k(KeyCode::Esc));
    assert!(a.outline.is_none());
    a.jump_to_outline(); // nothing open
    let mut empty = with_text("r3-outline-empty", "just prose\n");
    empty.open_outline();
    assert!(empty.outline.is_none());
    assert_eq!(empty.status, t!("status.outline_empty").to_string());
}

/// The outline sidebar follows the buffer and the cursor, is cleared when off
/// or when there is nothing to outline, and a click jumps to its row.
#[test]
fn the_outline_dock_tracks_the_buffer_and_jumps_on_click() {
    let mut a = with_text("r3-outlinedock", RUST_SRC);
    a.editor.active_tab_mut().unwrap().path = Some(a.root.join("lib.rs"));
    a.settings.secondary_panels.show_outline_dock = false;
    a.refresh_outline_dock();
    assert!(a.outline_dock.is_none() && a.outline_dock_key.is_none());
    a.outline_dock_click(0); // nothing to click
    a.toggle_outline_dock();
    assert!(a.settings.secondary_panels.show_outline_dock);
    assert_eq!(a.outline_dock.as_ref().unwrap().entries.len(), 3);
    a.outline_dock_click(7);
    assert_eq!(a.editor.cursor_1based().0, 1);
    a.outline_dock_click(2);
    assert_eq!(a.editor.cursor_1based().0, 5);
    // The highlight follows the cursor without rebuilding.
    a.editor
        .goto(3, None, ratatui::layout::Rect::new(0, 0, 80, 24));
    a.refresh_outline_dock();
    assert_eq!(a.outline_dock.as_ref().unwrap().selected, 1);
    // A buffer without symbols, and no buffer at all, clear it.
    a.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_content("plain\n");
    a.refresh_outline_dock();
    assert!(a.outline_dock.is_none());
    a.editor.tabs.clear();
    a.refresh_outline_dock();
    assert!(a.outline_dock.is_none() && a.outline_dock_key.is_none());
}

#[test]
fn diagnostic_severities_map_to_wire_numbers_and_colors() {
    use crate::lsp_core::Severity;
    use ratatui::style::Color;
    let all = [
        (Severity::Error, 1, Color::Red),
        (Severity::Warning, 2, Color::Yellow),
        (Severity::Information, 3, Color::Cyan),
        (Severity::Hint, 4, Color::Blue),
    ];
    for (sev, number, color) in all {
        assert_eq!(severity_number(sev), number);
        assert_eq!(severity_color(sev), color);
    }
}

#[test]
fn html_palette_cells_follow_the_row_columns() {
    let e = crate::html_character_picker::entities().first().unwrap();
    assert_eq!(html_cell_at(e, 0), e.glyph.to_string());
    assert_eq!(html_cell_at(e, 5), e.glyph.to_string());
    assert_eq!(html_cell_at(e, 6), e.name.to_string());
    assert_eq!(html_cell_at(e, 33), e.name.to_string());
    assert_eq!(html_cell_at(e, 34), e.code.to_string());
}

/// The recursive size and file counts skip `.git` and `target` and treat a
/// missing directory as empty.
#[test]
fn directory_totals_skip_generated_trees() {
    let dir = temp("r3-dirsize");
    std::fs::create_dir_all(dir.join("src/deep")).unwrap();
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    std::fs::create_dir_all(dir.join("target")).unwrap();
    std::fs::write(dir.join("a.txt"), "12345").unwrap();
    std::fs::write(dir.join("src/b.txt"), "123").unwrap();
    std::fs::write(dir.join("src/deep/c.txt"), "12").unwrap();
    std::fs::write(dir.join(".git/HEAD"), "ignored").unwrap();
    std::fs::write(dir.join("target/out"), "ignored").unwrap();
    assert_eq!(dir_size(&dir), 10);
    assert_eq!(count_files(&dir), 3);
    assert_eq!(dir_size(&dir.join("missing")), 0);
    assert_eq!(count_files(&dir.join("missing")), 0);
}

/// Replacing at an offset that is no longer a match leaves the text alone and
/// resumes after the old match.
#[test]
fn do_replace_without_a_match_resumes_after_the_old_one() {
    let mut a = with_text("r3-doreplace", "abc\n");
    let re = Regex::new("zzz").unwrap();
    let tab = a.editor.active_tab_mut().unwrap();
    assert_eq!(do_replace(tab, &re, false, "x", (1, 2)), 2);
    assert_eq!(tab.text(), "abc\n");
    let re = Regex::new("b").unwrap();
    assert_eq!(do_replace(tab, &re, false, "XY", (1, 2)), 3);
    assert_eq!(tab.text(), "aXYc\n");
}

#[test]
fn lsp_positions_clamp_to_the_buffer() {
    let a = with_text("r3-lsppos", "ab\ncd\n");
    let code = a.editor.active_tab().unwrap().editor.code_ref();
    let enc = crate::lsp_core::Encoding::default();
    assert_eq!(lsp_pos_to_char(code, 1, 1, enc), 4);
    assert_eq!(lsp_pos_to_char(code, 99, 0, enc), code.len());
    assert_eq!(lsp_pos_to_char(code, u32::MAX, 0, enc), code.len());
}

#[test]
fn the_text_size_limit_follows_the_setting_and_the_warning_threshold() {
    let t = crate::large_file::Thresholds::default();
    let mut settings = Settings::default();
    // No limit of one's own still asks past the 500 MiB warning (T830).
    settings.editor_behavior.large_file_mb = 0;
    assert_eq!(
        App::max_text_bytes(&settings, t),
        crate::large_file::WARN_BYTES
    );
    settings.editor_behavior.large_file_mb = 3;
    assert_eq!(App::max_text_bytes(&settings, t), 3 * 1024 * 1024);
    settings.editor_behavior.large_file_mb = u64::MAX / (1024 * 1024);
    assert_eq!(
        App::max_text_bytes(&settings, t),
        crate::large_file::WARN_BYTES
    );
    const { assert!(crate::large_file::WARN_BYTES < crate::fileops::HARD_MAX_TEXT_BYTES) };
    assert_eq!(
        crate::large_file::VIEWER_BYTES,
        crate::fileops::HARD_MAX_TEXT_BYTES,
        "the viewer takes over exactly where a buffer can no longer hold the file"
    );
}

/// With the thresholds injected small (T830): past the warning threshold
/// the question carries a memory warning, even with `large_file_mb = 0`;
/// past the viewer threshold the file opens in the Large File Viewer with
/// no question.
#[test]
fn large_files_warn_then_degrade_to_the_viewer() {
    let dir = temp("r3-thresholds");
    let mut settings = Settings::default();
    settings.editor_behavior.large_file_mb = 0;
    let mut a = crate::app::cov4::app_with(&dir, settings);
    a.set_open_thresholds(crate::large_file::Thresholds {
        warn: 1_000,
        viewer: 2_000,
    });
    let small = dir.join("small.txt");
    std::fs::write(&small, "s".repeat(900)).unwrap();
    a.open_path(&small, false);
    assert!(a.confirm.is_none(), "under the warning threshold: loads");
    assert_eq!(a.active_path().as_deref(), Some(small.as_path()));

    let warned = dir.join("warned.txt");
    std::fs::write(&warned, "w".repeat(1_500)).unwrap();
    a.open_path(&warned, false);
    let c = a.confirm.as_ref().expect("asks first");
    assert_eq!(c.kind, ConfirmKind::OpenLarge);
    assert!(
        c.message
            .contains(&t!("confirm.open_large_warning").to_string()),
        "{}",
        c.message
    );
    a.confirm_key(k(KeyCode::Char('y')));
    assert_eq!(a.active_path().as_deref(), Some(warned.as_path()));

    let huge = dir.join("huge.txt");
    std::fs::write(&huge, "h".repeat(2_500)).unwrap();
    a.open_path(&huge, false);
    assert!(
        a.confirm.is_none(),
        "past the viewer threshold: no question"
    );
    assert_eq!(
        a.large_file.as_ref().map(|v| v.path().to_path_buf()),
        Some(huge.clone())
    );
    assert_ne!(a.active_path().as_deref(), Some(huge.as_path()));
    // An arrow-key preview still only reports it.
    a.large_file = None;
    a.open_path(&huge, true);
    assert!(a.large_file.is_none());
    assert!(a.confirm.is_none());
}

#[cfg(feature = "git")]
#[test]
fn gutter_marks_have_distinct_colors() {
    use crate::git::LineMark;
    let colors = [LineMark::Added, LineMark::Modified, LineMark::Deleted].map(gutter_hex);
    assert_eq!(colors, ["#3fb950", "#d29922", "#f85149"]);
}

#[cfg(feature = "image")]
#[test]
fn decoding_an_image_reports_why_it_cannot() {
    let dir = temp("r3-decode");
    let ok = dir.join("ok.png");
    image::DynamicImage::new_rgb8(2, 2)
        .save_with_format(&ok, image::ImageFormat::Png)
        .unwrap();
    assert_eq!(decode_image(&ok).unwrap().width(), 2);
    let bad = dir.join("bad.png");
    std::fs::write(&bad, "not an image").unwrap();
    assert!(decode_image(&bad).is_err());
    assert!(decode_image(&dir.join("missing.png")).is_err());
}

#[cfg(all(feature = "ai", feature = "sql"))]
#[test]
fn ai_replies_for_the_chat_and_db_surfaces_are_inline() {
    assert!(AiDest::Panel.is_inline());
    assert!(AiDest::Db.is_inline());
}

/// The code-action chooser: Up/Down move, `p` previews (no edit says so),
/// Enter on an edit-less action reports it, and a closed chooser ignores keys.
#[test]
fn the_code_action_chooser_moves_previews_and_applies() {
    let mut a = with_text("r3-codeaction", "x\n");
    a.code_actions = Some(CodeActionMenu {
        actions: vec![
            ("Only a command".to_string(), Vec::new()),
            ("Another command".to_string(), Vec::new()),
        ],
        selected: 1,
        refactor: false,
    });
    a.code_action_key(k(KeyCode::Up));
    assert_eq!(a.code_actions.as_ref().unwrap().selected, 0);
    a.code_action_key(k(KeyCode::Down));
    a.code_action_key(k(KeyCode::Down));
    assert_eq!(a.code_actions.as_ref().unwrap().selected, 1);
    a.code_action_key(k(KeyCode::Char('p')));
    assert_eq!(
        a.status,
        t!("status.code_action_no_edit", title = "Another command").to_string()
    );
    a.code_action_key(k(KeyCode::Char('?')));
    a.code_action_key(k(KeyCode::Enter));
    assert!(a.code_actions.is_none());
    assert_eq!(
        a.status,
        t!("status.code_action_no_edit", title = "Another command").to_string()
    );
    a.code_action_key(k(KeyCode::Down));
    a.code_action_key(k(KeyCode::Up));
    a.code_action_key(k(KeyCode::Char('p')));
    a.code_action_key(k(KeyCode::Esc));
}

/// Project detection reads `pyproject.toml`; a missing directory lists nothing.
#[cfg(feature = "tasks")]
#[test]
fn project_types_are_detected_from_manifests() {
    let dir = temp("r3-projtypes");
    std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
    std::fs::write(
        dir.join("pyproject.toml"),
        "[project]\nname = \"x\"\n[tool.poetry]\nname = \"x\"\n",
    )
    .unwrap();
    let found = App::detect_project_types_at(&dir);
    assert!(found.len() >= 2, "{found:?}");
    assert_eq!(App::dir_entry_names(&dir.join("absent")).len(), 0);
    let names = App::dir_entry_names(&dir);
    assert!(names.iter().any(|n| n == "Cargo.toml"));
    assert_eq!(App::detect_project_types_at(&dir.join("absent")).len(), 0);
}

/// A key with no token and clicking nothing in a closed help overlay are no-ops.
#[test]
fn override_keys_ignore_unencodable_events() {
    let mut a = app_at(&temp("r3-override"));
    assert!(!a.override_key(KeyEvent::new(KeyCode::Null, KeyModifiers::NONE)));
    assert!(!a.override_key(k(KeyCode::Char('q'))));
    a.key_overrides.insert(
        crate::macros::encode_key(k(KeyCode::Char('q'))),
        "view.scrollbar".to_string(),
    );
    let before = a.visible.contains(Visible::SCROLLBAR);
    assert!(a.override_key(k(KeyCode::Char('q'))));
    assert_ne!(a.visible.contains(Visible::SCROLLBAR), before);
}

/// Pomodoro ticks that change nothing, and a clipboard chooser row that is gone.
#[test]
fn pomodoro_ticks_without_a_phase_change_and_stale_choosers() {
    let mut a = app_at(&temp("r3-pomo2"));
    a.open_pomodoro();
    a.pomodoro_primary();
    assert!(a.pomodoro_running());
    a.pomodoro_last_tick = std::time::Instant::now().checked_sub(std::time::Duration::from_secs(3));
    a.poll_pomodoro();
    assert!(a.pomodoro_running(), "three seconds into a long session");
    a.chooser = Some(crate::app::chooser::Chooser::Clipboard(ClipboardChooser {
        entries: Vec::new(),
        selected: 4,
    }));
    a.paste_selected_clipboard();
    assert!(a.chooser.is_none());
    a.chooser = Some(crate::app::chooser::Chooser::Workspace(WorkspaceChooser {
        roots: Vec::new(),
        selected: 2,
    }));
    a.switch_to_selected_workspace();
    assert!(a.chooser.is_none());
}

/// The file browser: a typed folder re-roots it, a typed new file opens as a
/// new tab (with an optional `:line` jump), and a closed browser ignores Enter.
#[test]
fn the_file_browser_follows_typed_paths() {
    let dir = temp("r3-fbrowser");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub/inner.txt"), "a\nb\nc\n").unwrap();
    let mut a = app_at(&dir);
    a.file_browser_open_selected();
    a.open_file_browser();
    a.file_browser.as_mut().unwrap().query = format!("{}/sub", dir.display());
    a.file_browser_open_selected();
    let fb = a.file_browser.as_ref().expect("still open, re-rooted");
    assert_eq!(fb.root, dir.join("sub"));
    assert_eq!(fb.query, "");
    a.file_browser.as_mut().unwrap().query = format!("{}:3", dir.join("sub/inner.txt").display());
    a.file_browser_open_selected();
    assert!(a.file_browser.is_none());
    assert_eq!(
        a.active_path().as_deref(),
        Some(dir.join("sub/inner.txt").as_path())
    );
    assert_eq!(a.editor.cursor_1based().0, 3);
}

/// Pasting, copying and deleting with nothing selected or nothing to paste is
/// a no-op; a paste with no destination selected goes to the workspace root.
#[test]
fn explorer_operations_with_an_empty_selection_do_nothing() {
    let dir = temp("r3-explorer-empty");
    let mut a = app_at(&dir);
    a.explorer.rebuild();
    a.explorer_copy(false);
    assert_eq!(a.clip.len(), 0);
    a.explorer_delete_request();
    assert!(a.confirm.is_none());
    a.explorer_paste();
    assert!(a.paste.is_none());
    let src = temp("r3-explorer-src").join("only.txt");
    std::fs::write(&src, "x").unwrap();
    a.clip = vec![src.clone()];
    a.explorer_paste();
    assert!(dir.join("only.txt").exists(), "pasted into the root");
    assert!(src.exists());
    a.process_paste(); // nothing in progress
}

/// A clean buffer whose file turned binary or non-regular is kept and warned
/// about when the buffers are reloaded.
#[test]
fn reloading_refuses_files_that_became_binary_or_directories() {
    let dir = temp("r3-reload-refused");
    let (bin, was_file) = (dir.join("a.txt"), dir.join("b.txt"));
    std::fs::write(&bin, "text").unwrap();
    std::fs::write(&was_file, "text").unwrap();
    let mut a = app_at(&dir);
    a.open_path(&bin, false);
    a.open_path(&was_file, false);
    std::fs::write(&bin, [0u8, 1, 2, 0, 3]).unwrap();
    std::fs::remove_file(&was_file).unwrap();
    std::fs::create_dir(&was_file).unwrap();
    assert_eq!(a.reload_clean_buffers(), 0);
    let warned: Vec<String> = a.messages.items.iter().map(|m| m.text.clone()).collect();
    assert!(
        warned
            .iter()
            .any(|m| m.contains("a.txt") && m.contains(&t!("msg.open_binary").to_string())),
        "{warned:?}"
    );
    assert!(
        warned
            .iter()
            .any(|m| m.contains("b.txt") && m.contains(&t!("msg.open_not_regular").to_string())),
        "{warned:?}"
    );
    assert!(a.editor.tabs.iter().any(|t| t.text() == "text"));
}

/// Save As: an empty name does nothing; saving over another file asks first
/// and the answers are honored; a dangling symlink is refused.
#[cfg(unix)]
#[test]
fn save_as_asks_before_overwriting_and_refuses_dangling_links() {
    let dir = temp("r3-saveas");
    let mut a = with_text("r3-saveas-app", "content\n");
    a.root = dir.clone();
    a.accept_file_prompt(PromptKind::SaveAs, "");
    a.accept_file_prompt(PromptKind::Open, "");
    a.accept_file_prompt(PromptKind::Rename, "ignored");
    let target = dir.join("out.txt");
    a.accept_file_prompt(PromptKind::SaveAs, "out.txt");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "content\n");
    assert_eq!(
        a.status,
        t!("status.saved", path = target.display()).to_string()
    );
    // Another buffer, same target: it asks.
    a.editor.new_tab_with_content("other\n");
    a.accept_file_prompt(PromptKind::SaveAs, "out.txt");
    assert!(a.confirm.is_some() && a.save_as_overwrite.is_some());
    a.confirm_key(k(KeyCode::Char('y')));
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "other\n");
    assert!(a.save_as_overwrite.is_none());
    // A save failure is reported.
    a.save_active_as_now(dir.join("no/such/dir/x.txt"));
    assert!(a.messages.items.last().is_some_and(|m| {
        m.text
            .starts_with(t!("msg.save_failed", error = "").trim_end())
    }));
    // A dangling symlink is never written through.
    let link = dir.join("dangling");
    std::os::unix::fs::symlink(dir.join("nowhere"), &link).unwrap();
    a.accept_file_prompt(PromptKind::SaveAs, "dangling");
    assert_eq!(
        a.messages.items.last().map(|m| m.text.clone()),
        Some(t!("msg.save_as_dangling_symlink", path = link.display()).to_string())
    );
    assert!(!dir.join("nowhere").exists());
}

/// A shell command in a directory that does not exist reports the failure and
/// leaves the app free to run the next one.
#[cfg(all(feature = "tasks", unix))]
#[test]
fn a_command_in_a_missing_directory_fails_cleanly() {
    let mut a = app_at(&temp("r3-badcwd"));
    let missing = a.root.join("no-such-dir");
    a.run_command_in(&missing, "echo never");
    assert!(a.running_command.is_none());
    assert!(!a.bottom_dock.lines.iter().any(|l| l == "never"));
    assert!(a.bottom_dock.lines.iter().any(|l| l.starts_with("[error:")));
    assert!(a.messages.items.last().is_some_and(|m| {
        m.text
            .starts_with(t!("msg.command_failed", error = "").trim_end())
    }));
}

/// The dock search: a bad regex is listed; an empty query does nothing.
#[test]
fn the_workspace_dock_search_reports_a_bad_regex() {
    let mut a = app_at(&temp("r3-dock-search"));
    a.search_workspace_to_dock("   ", false, false);
    assert_eq!(a.bottom_dock.lines.len(), 0);
    a.search_workspace_to_dock("(", false, true);
    assert!(
        a.bottom_dock
            .lines
            .iter()
            .any(|l| l.starts_with("[bad regex"))
    );
    assert!(
        a.status
            .starts_with(&t!("msg.bad_regex", error = "").to_string())
    );
    assert!(a.visible.contains(Visible::BOTTOM_DOCK));
}

/// Workspace search with a REGEX flag and an invalid pattern says so, and the
/// project-wide replace refuses the same way.
#[test]
fn workspace_search_and_replace_report_an_invalid_regex() {
    let mut a = app_at(&temp("r3-ws-badre"));
    a.workspace_search = None;
    a.workspace_replace_all();
    a.apply_replace_confirm();
    a.run_workspace_search();
    let mut panel = WorkspaceSearch::new(true);
    panel.query = "(a".into();
    panel.flags.insert(WorkspaceFlags::REGEX);
    a.workspace_search = Some(panel);
    a.run_workspace_search();
    let bad = t!("msg.bad_regex", error = "").to_string();
    assert!(
        a.workspace_search
            .as_ref()
            .unwrap()
            .status
            .starts_with(&bad)
    );
    a.workspace_replace_all();
    assert!(
        a.workspace_search
            .as_ref()
            .unwrap()
            .status
            .starts_with(&bad)
    );
    assert!(a.replace_confirm.is_none());
    // A replace panel without the replace flag plans nothing either.
    let mut plain = WorkspaceSearch::new(false);
    plain.query = "x".into();
    a.workspace_search = Some(plain);
    a.workspace_replace_all();
    assert!(a.replace_confirm.is_none());
}

/// A confirmed replace into a clean open file refreshes that tab; a file
/// that cannot be written is reported.
#[test]
fn applying_a_file_plan_refreshes_clean_tabs_and_reports_write_failures() {
    let dir = temp("r3-fileplan");
    let f = dir.join("f.txt");
    std::fs::write(&f, "old\n").unwrap();
    let mut a = app_at(&dir);
    a.open_path(&f, false);
    let changed = a.apply_file_plan(&[
        (f.clone(), "new\n".to_string()),
        (dir.join("no/such/dir/g.txt"), "x".to_string()),
    ]);
    assert_eq!(changed, vec![f.clone()]);
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "new\n");
    assert_eq!(a.editor.active_tab().unwrap().text(), "new\n");
    assert!(!a.editor.active_tab().unwrap().dirty);
    assert!(a.messages.items.iter().any(|m| m.text.contains("g.txt")));
}

/// Org wiki-link completion only starts after `[[` in an Org buffer with no
/// popup already open.
#[test]
fn wiki_link_completion_needs_org_and_a_fresh_double_bracket() {
    let mut a = with_text("r3-wikilink", "[[");
    a.maybe_complete_wiki_link(); // no path
    a.editor.active_tab_mut().unwrap().path = Some(a.root.join("n.txt"));
    a.maybe_complete_wiki_link(); // not org
    assert!(a.completion.is_none());
    a.editor.active_tab_mut().unwrap().path = Some(a.root.join("n.org"));
    a.editor.active_tab_mut().unwrap().editor.set_cursor(1);
    a.maybe_complete_wiki_link(); // cursor before the brackets are complete
    a.editor.active_tab_mut().unwrap().editor.set_cursor(0);
    a.maybe_complete_wiki_link();
    a.editor.tabs.clear();
    a.maybe_complete_wiki_link();
}

/// Transforms and cursor rewrites with no buffer are quiet no-ops; so are the
/// keybinding-editor actions with no editor open.
#[test]
fn transforms_and_keybinding_actions_with_nothing_open_do_nothing() {
    let mut a = app_at(&temp("r3-nothing-open"));
    a.editor.tabs.clear();
    a.transform_selection_or_buffer(str::to_uppercase);
    a.rewrite_at_cursor(|t, c| Some((t.to_uppercase(), c)), None);
    assert!(a.editor.tabs.is_empty());
    a.begin_rebind_selected_keybinding();
    assert!(a.prompt.is_none() && a.pending_rebind_action_id.is_none());
    a.reset_selected_keybinding();
    a.accept_rebind_key("ctrl-x");
    assert!(a.keybinding_editor.is_none());
    a.refresh_keybinding_editor();
    #[cfg(feature = "sql")]
    {
        a.edit_sql_key(k(KeyCode::Char('d')));
        a.save_edit_sql();
    }
}

/// Refreshing the keybinding editor keeps its selection inside the rows.
#[test]
fn the_keybinding_editor_selection_is_clamped_on_refresh() {
    let mut a = app_at(&temp("r3-kbd-clamp"));
    a.open_keybinding_editor();
    let rows = a.keybinding_editor.as_ref().unwrap().len();
    assert!(rows > 0);
    a.keybinding_editor.as_mut().unwrap().selected = rows + 50;
    a.refresh_keybinding_editor();
    assert_eq!(a.keybinding_editor.as_ref().unwrap().selected, rows - 1);
    a.begin_rebind_selected_keybinding();
    assert!(
        a.prompt
            .as_ref()
            .is_some_and(|p| p.kind == PromptKind::RebindKey)
    );
    assert!(a.pending_rebind_action_id.is_some());
}

/// Go to definition: with no word it warns; with one definition it jumps;
/// with none or several it reports or lists them.
#[test]
fn go_to_definition_falls_back_to_a_workspace_search() {
    let dir = temp("r3-gotodef");
    std::fs::write(dir.join("lib.rs"), "fn only_here() {}\nfn twice() {}\n").unwrap();
    std::fs::write(dir.join("other.rs"), "fn twice() {}\n").unwrap();
    let mut a = app_at(&dir);
    a.editor.new_tab_with_content("");
    a.goto_definition();
    assert_eq!(
        a.messages.items.last().map(|m| m.text.clone()),
        Some(t!("msg.no_symbol").to_string())
    );
    a.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_content("nothing_defines_this");
    a.editor.active_tab_mut().unwrap().editor.set_cursor(3);
    a.goto_definition();
    assert_eq!(
        a.messages.items.last().map(|m| m.text.clone()),
        Some(t!("msg.no_definition", symbol = "nothing_defines_this").to_string())
    );
    a.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_content("only_here");
    a.editor.active_tab_mut().unwrap().editor.set_cursor(2);
    a.goto_definition();
    assert_eq!(
        a.status,
        t!("status.definition_of", symbol = "only_here").to_string()
    );
    assert_eq!(
        a.active_path().as_deref(),
        Some(dir.join("lib.rs").as_path())
    );
    a.editor.new_tab_with_content("twice");
    a.editor.active_tab_mut().unwrap().editor.set_cursor(1);
    a.goto_definition();
    assert!(
        a.workspace_search
            .as_ref()
            .is_some_and(|p| p.hits.len() == 2)
    );
}

/// The clock and calendar boxes insert what is clicked and close on an outside
/// click; the calendar's header arrows page months and ignore other cells.
#[test]
fn clock_and_calendar_clicks_insert_navigate_and_close() {
    use ratatui::layout::Rect;
    let mut a = with_text("r3-clock", "");
    a.layout.clock = Rect::new(0, 0, 30, 8);
    a.visible.set(Visible::CLOCK, true);
    a.clock_mouse(wheel(MouseEventKind::ScrollUp));
    a.clock_mouse(crate::app::cov4::click(2, 1));
    assert_ne!(a.editor.active_tab().unwrap().text().trim(), "");
    a.clock_mouse(crate::app::cov4::click(2, 60));
    assert!(!a.visible.contains(Visible::CLOCK));

    a.layout.calendar = Rect::new(0, 0, 30, 10);
    a.visible.set(Visible::CALENDAR, true);
    a.calendar_mouse(wheel(MouseEventKind::ScrollUp));
    let before = a.editor.active_tab().unwrap().text();
    a.calendar_mouse(crate::app::cov4::click(5, 1)); // the weekday header
    assert_eq!(a.editor.active_tab().unwrap().text(), before);
    a.calendar_mouse(crate::app::cov4::click(10, 0)); // inside the header, off the arrows
    a.calendar_mouse(crate::app::cov4::click(0, 0));
    a.calendar_mouse(crate::app::cov4::click(20, 0));
    a.calendar_mouse(crate::app::cov4::click(20, 0));
    a.calendar_mouse(crate::app::cov4::click(1, 3)); // a day cell
    a.calendar_mouse(crate::app::cov4::click(40, 40));
    assert!(!a.visible.contains(Visible::CALENDAR));
}

/// Workspace search stops at 5000 hits, as does the dock search; a replace
/// plan skips a file that vanished after the search.
#[test]
fn workspace_searches_are_capped_and_replace_skips_vanished_files() {
    let dir = temp("r3-ws-cap");
    let body: String = (0..5100).map(|_| "needle in a haystack\n").collect();
    std::fs::write(dir.join("big.txt"), body).unwrap();
    std::fs::write(dir.join("small.txt"), "needle here\n").unwrap();
    let mut a = app_at(&dir);
    a.build_file_index();
    let mut panel = WorkspaceSearch::new(true);
    panel.query = "needle".into();
    panel.replace = "pin".into();
    a.workspace_search = Some(panel);
    a.run_workspace_search();
    assert_eq!(a.workspace_search.as_ref().unwrap().hits.len(), 5000);
    std::fs::remove_file(dir.join("big.txt")).unwrap();
    std::fs::remove_file(dir.join("small.txt")).unwrap();
    a.workspace_replace_all();
    assert!(a.replace_confirm.is_none(), "nothing left to replace");
    a.build_file_index();
    std::fs::write(dir.join("big.txt"), "needle in a haystack\n".repeat(5100)).unwrap();
    a.build_file_index();
    a.search_workspace_to_dock("needle", true, false);
    // The dock keeps only its scrollback; the search stopped at the cap.
    assert_eq!(a.bottom_dock.lines.len(), 1000);
    assert!(
        a.bottom_dock
            .lines
            .iter()
            .filter(|l| l.starts_with("big.txt:"))
            .count()
            >= 990
    );
}

/// Editing the search-results buffer: lines that are not results, results for
/// files no longer present and a closed results buffer all leave the files
/// alone, and only a real change asks for confirmation.
#[test]
fn editable_results_ignore_unusable_lines() {
    let dir = temp("r3-wgrep");
    let f = dir.join("a.txt");
    std::fs::write(&f, "needle one\nneedle two\n").unwrap();
    let mut a = app_at(&dir);
    a.prepare_wgrep_confirm(); // no results buffer yet
    assert!(a.wgrep_confirm.is_none());
    a.build_file_index();
    let mut panel = WorkspaceSearch::new(false);
    panel.query = "needle".into();
    a.workspace_search = Some(panel);
    a.run_workspace_search();
    a.open_wgrep_results();
    assert!(a.wgrep_baseline.is_some());
    // Junk and an unknown file are skipped, the real line is edited.
    let text = a.editor.active_tab().unwrap().text();
    let edited = format!(
        "this is not a result line\nunknown.txt:1: whatever\n{}",
        text.replacen("needle one", "NEEDLE ONE", 1)
    );
    a.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_content(&edited);
    std::fs::remove_file(&f).unwrap();
    a.prepare_wgrep_confirm();
    assert!(
        a.wgrep_confirm.is_none(),
        "the file is gone: nothing to write"
    );
    std::fs::write(&f, "needle one\nneedle two\n").unwrap();
    a.prepare_wgrep_confirm();
    assert!(a.wgrep_confirm.is_some());
    a.wgrep_confirm_key(k(KeyCode::Char('x')));
    assert!(a.wgrep_confirm.is_some());
    a.apply_wgrep_confirm();
    assert_eq!(
        std::fs::read_to_string(&f).unwrap(),
        "NEEDLE ONE\nneedle two\n"
    );
    a.apply_wgrep_confirm(); // nothing pending
    a.editor.tabs.clear();
    a.prepare_wgrep_confirm();
}

/// A Cowork question answered with no Cowork session is dropped, not run.
#[test]
fn cowork_confirmations_are_routed_away_from_the_delete_path() {
    let dir = temp("r3-cowork-confirm");
    let f = dir.join("keep.txt");
    std::fs::write(&f, "x").unwrap();
    let mut a = app_at(&dir);
    a.confirm = Some(Confirm {
        message: String::new(),
        paths: vec![f.clone()],
        kind: ConfirmKind::CoworkShareTerminal,
    });
    a.confirm_key(k(KeyCode::Char('y')));
    assert!(f.exists(), "a cowork prompt never deletes");
}

/// Opening an image asks the terminal for graphics support once.
#[cfg(feature = "image")]
#[test]
fn opening_an_image_asks_the_terminal_once_for_graphics() {
    let dir = temp("r3-image-probe");
    let png = dir.join("p.png");
    image::DynamicImage::new_rgba8(2, 2)
        .save_with_format(&png, image::ImageFormat::Png)
        .unwrap();
    let mut a = app_at(&dir);
    a.image_probe = Some(|| Some(ratatui_image::picker::Picker::halfblocks()));
    a.open_path(&png, false);
    assert!(a.picker.is_some());
    assert!(a.editor.active_tab().unwrap().is_image());
    assert!(a.image_probe.is_none(), "the probe runs once");
}

/// A file's own modeline decides soft wrap when it opens.
#[test]
fn a_modeline_sets_soft_wrap_when_the_file_opens() {
    let dir = temp("r3-modeline");
    let nowrap = dir.join("a.txt");
    let wrapped = dir.join("b.txt");
    std::fs::write(&nowrap, "text\n# vim: set nowrap:\n").unwrap();
    std::fs::write(&wrapped, "text\n# vim: set wrap:\n").unwrap();
    let mut a = app_at(&dir);
    a.open_path(&nowrap, false);
    assert!(!a.editor.active_tab().unwrap().editor.soft_wrap_enabled());
    a.open_path(&wrapped, false);
    assert!(a.editor.active_tab().unwrap().editor.soft_wrap_enabled());
}

/// Clicking the HTML palette inserts the cell under the pointer; a click
/// below the last entity inserts nothing.
#[test]
fn html_palette_clicks_pick_a_cell_or_nothing() {
    use ratatui::layout::Rect;
    let mut a = with_text("r3-htmlclick", "");
    a.html_mouse(crate::app::cov4::click(1, 1)); // no panel
    a.open_html_panel();
    a.layout.html_panel = Rect::new(0, 0, 60, 60000);
    a.html_mouse(crate::app::cov4::click(1, 0));
    let glyph = a.editor.active_tab().unwrap().text();
    assert_ne!(glyph, "");
    assert_eq!(
        a.status,
        t!("status.html_inserted", name = glyph).to_string()
    );
    a.html_mouse(crate::app::cov4::click(1, 50000));
    assert_eq!(a.editor.active_tab().unwrap().text(), glyph);
}

/// The spelling popup: Down stops at the last suggestion, a click on a
/// suggestion row that does not exist does nothing, and applying or ignoring
/// with no popup is harmless.
#[test]
fn the_spelling_popup_moves_within_its_suggestions() {
    use ratatui::layout::Rect;
    let mut a = with_text("r3-spellpop", "teh\n");
    a.spell_ignore_word();
    a.spell_apply_selected();
    a.spell_suggest = Some(SpellSuggest {
        word: "teh".into(),
        span: (0, 3),
        suggestions: vec!["the".into(), "ten".into()],
        selected: 0,
    });
    a.spell_suggest_key(k(KeyCode::Down));
    a.spell_suggest_key(k(KeyCode::Down));
    assert_eq!(a.spell_suggest.as_ref().unwrap().selected, 1);
    a.spell_suggest_key(k(KeyCode::Char('?')));
    a.layout.spell_suggest = Rect::new(0, 0, 20, 5);
    a.spell_suggest_mouse(crate::app::cov4::click(1, 4));
    assert!(a.spell_suggest.is_some(), "no such row");
    // A stale selection past the list closes the popup without editing.
    a.spell_suggest.as_mut().unwrap().selected = 9;
    a.spell_apply_selected();
    assert!(a.spell_suggest.is_none());
    assert_eq!(a.editor.active_tab().unwrap().text(), "teh\n");
    a.spell_suggest = Some(SpellSuggest {
        word: "teh".into(),
        span: (0, 3),
        suggestions: vec!["the".into()],
        selected: 0,
    });
    a.spell_suggest_mouse(crate::app::cov4::click(1, 0));
    assert_eq!(a.editor.active_tab().unwrap().text(), "the\n");
    assert!(a.editor.active_tab().unwrap().dirty);
}

/// A query-replace step with no buffer ends nothing and changes nothing.
#[test]
fn a_query_replace_step_without_a_buffer_is_ignored() {
    let mut a = with_text("r3-qr-nobuf", "aa\n");
    a.run_action("edit.query_replace");
    a.search.as_mut().unwrap().query = "a".into();
    a.begin_query_replace();
    assert!(a.query_replace.is_some());
    a.editor.tabs.clear();
    a.qr_apply(Decision::Skip);
    assert!(a.query_replace.is_some());
}

/// While a snippet is expanding Tab walks its stops and Esc ends the session
/// without touching the text.
#[test]
fn escape_ends_a_snippet_session() {
    let mut a = with_text("r3-snippet-esc", "");
    a.insert_snippet_body("fn ${1:name}(${2:args}) {\n    $0\n}");
    assert!(a.snippet_active());
    let text = a.editor.active_tab().unwrap().text();
    assert!(text.starts_with("fn name(args) {"), "{text}");
    a.editor_key(k(KeyCode::Tab));
    assert!(a.snippet_active());
    a.editor_key(k(KeyCode::Esc));
    assert!(!a.snippet_active());
    assert_eq!(a.editor.active_tab().unwrap().text(), text);
}
