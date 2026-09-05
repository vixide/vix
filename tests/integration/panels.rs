#![warn(clippy::pedantic)]
#![allow(clippy::cast_possible_truncation, clippy::format_collect)]
// Shared fixtures/helpers live in `common.rs`; every test here needs a
// handful of them, so a glob import earns its keep over a long explicit list.
#![allow(clippy::wildcard_imports)]

use crate::common::*;

#[test]
fn calendar_left_right_pages_months() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.calendar");
    assert!(app.show_calendar, "Calendar opens on the current month");
    let start = app.calendar.shown_month();
    assert!(
        app.calendar.grid().today.is_some(),
        "today shows in the current month"
    );

    // Ctrl pages months; Ctrl+Right forward, Ctrl+Left back to where we started.
    app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL));
    assert_ne!(
        app.calendar.shown_month(),
        start,
        "Ctrl+Right advances the month"
    );
    app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL));
    assert_eq!(
        app.calendar.shown_month(),
        start,
        "Ctrl+Left returns to the start month"
    );

    // Plain arrows move the selected day.
    let sel = app.calendar.selected();
    app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert_ne!(app.calendar.selected(), sel, "Right moves the selected day");

    // Esc closes the box.
    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!app.show_calendar, "Esc closes the calendar");
}

#[test]
fn calendar_nav_arrows_change_the_month() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let mut app = app_at(Path::new("."));
    app.run_action("tools.calendar");
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    let cal = app.layout.calendar;
    let title = app.calendar.title();

    // The nav arrows sit on the month-header row (row 0): ◀ at col 0, ▶ at col 20.
    app.on_mouse(click(cal.x + 20, cal.y)); // ▶ next month
    assert_ne!(app.calendar.title(), title, "▶ advanced to the next month");
    app.on_mouse(click(cal.x, cal.y)); // ◀ previous month
    assert_eq!(
        app.calendar.title(),
        title,
        "◀ returned to the original month"
    );
    assert!(app.show_calendar, "an arrow click keeps the calendar open");
}

#[test]
fn calendar_click_inserts_into_editor() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let mut app = app_at(Path::new("."));
    app.run_action("tools.calendar");
    let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    let cal = app.layout.calendar;
    assert!(cal.width > 0, "the calendar rect was recorded");

    // Click a populated day cell (cells are 3 columns wide; the grid's weekday
    // header is row 1 and the week rows start at row 2). Date-time lines moved to
    // the clock box, so the calendar only inserts days now.
    let grid = app.calendar.grid();
    let (wk, col) = grid
        .weeks
        .iter()
        .enumerate()
        .find_map(|(w, week)| week.iter().position(Option::is_some).map(|c| (w, c)))
        .unwrap();
    let before = app.editor.active_tab().unwrap().text().len();
    app.on_mouse(click(cal.x + col as u16 * 3 + 1, cal.y + 2 + wk as u16));
    let after = app.editor.active_tab().unwrap().text();
    assert!(after.len() > before, "clicking a day inserted a date");

    // A click outside the box closes it.
    app.on_mouse(click(0, 23));
    assert!(!app.show_calendar, "an outside click closes the calendar");
}

#[test]
fn explorer_left_arrow_collapses_never_opens() {
    let dir = unique_dir("explorerleft");
    fs::create_dir(dir.join("sub")).unwrap();
    fs::write(dir.join("sub/inner.txt"), "x").unwrap();

    let mut app = app_at(&dir);
    app.focus = Focus::Explorer;
    let has_inner = |app: &App| app.explorer.nodes.iter().any(|n| n.name == "inner.txt");

    // Left on a collapsed folder must NOT open it.
    app.explorer.selected = node_index(&app, "sub");
    app.on_key(keycode(KeyCode::Left));
    assert!(!has_inner(&app), "Left must not expand a collapsed folder");

    // Right opens it; Left then closes it again.
    app.on_key(keycode(KeyCode::Right));
    assert!(has_inner(&app), "Right expands the folder");
    app.explorer.selected = node_index(&app, "sub");
    app.on_key(keycode(KeyCode::Left));
    assert!(!has_inner(&app), "Left collapses the expanded folder");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_copy_paste_into_directory() {
    let dir = unique_dir("clip");
    fs::write(dir.join("a.txt"), "hello").unwrap();
    fs::create_dir(dir.join("sub")).unwrap();

    let mut app = app_at(&dir);
    app.focus = Focus::Explorer;
    app.explorer.selected = node_index(&app, "a.txt");
    app.on_key(ctrl('c')); // copy
    app.explorer.selected = node_index(&app, "sub");
    app.on_key(ctrl('v')); // paste into sub/

    assert!(dir.join("sub/a.txt").exists(), "file copied into sub/");
    assert!(dir.join("a.txt").exists(), "original remains after copy");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_cut_moves_file_and_follows_buffer() {
    let dir = unique_dir("cut");
    fs::write(dir.join("a.txt"), "hello\n").unwrap();
    fs::create_dir(dir.join("sub")).unwrap();

    let mut app = app_at(&dir);
    app.open_initial(&dir.join("a.txt")); // buffer open on the file
    app.focus = Focus::Explorer;
    app.explorer.selected = node_index(&app, "a.txt");
    app.on_key(ctrl('x')); // cut
    app.explorer.selected = node_index(&app, "sub");
    app.on_key(ctrl('v')); // paste/move into sub/

    assert!(dir.join("sub/a.txt").exists(), "file moved");
    assert!(!dir.join("a.txt").exists(), "original gone after move");
    // The open buffer now points at the new location.
    let tab_path = app.editor.active_tab().unwrap().path.clone().unwrap();
    assert!(
        tab_path.ends_with("sub/a.txt"),
        "buffer followed the move: {tab_path:?}"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_delete_closes_buffer() {
    let dir = unique_dir("del");
    fs::write(dir.join("a.txt"), "bye\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&dir.join("a.txt"));
    assert_eq!(app.editor.tabs.len(), 2); // initial empty buffer + a.txt
    app.focus = Focus::Explorer;
    app.explorer.selected = node_index(&app, "a.txt");
    app.on_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE)); // request
    app.on_key(key('y')); // confirm

    assert!(!dir.join("a.txt").exists(), "file deleted");
    // The file's buffer closed; only the empty buffer remains.
    assert_eq!(app.editor.tabs.len(), 1);
    assert!(app.editor.active_tab().unwrap().path.is_none());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_multiselect_collects_paths() {
    let dir = unique_dir("multi");
    fs::write(dir.join("a.txt"), "1").unwrap();
    fs::write(dir.join("b.txt"), "2").unwrap();
    fs::write(dir.join("c.txt"), "3").unwrap();
    let mut app = app_at(&dir);
    app.focus = Focus::Explorer;
    app.explorer.selected = 0;
    app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT));
    app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT));
    assert_eq!(
        app.explorer.selected_paths().len(),
        3,
        "anchor..cursor inclusive"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_navigation_is_bounded() {
    let mut app = app_at(Path::new("."));
    for _ in 0..5 {
        app.explorer.up();
    }
    assert_eq!(app.explorer.selected, 0);
    app.explorer.last();
    let last = app.explorer.selected;
    app.explorer.down();
    assert_eq!(app.explorer.selected, last, "down at end stays put");
}

#[test]
fn ctrl_o_opens_file_browser_and_esc_closes() {
    let mut app = app_at(Path::new("."));
    app.on_key(ctrl('o'));
    assert!(app.file_browser.is_some(), "Ctrl+O opens the file browser");
    app.on_key(esc());
    assert!(app.file_browser.is_none(), "Esc closes the browser");
}

#[test]
fn file_browser_ctrl_o_falls_back_to_the_path_prompt() {
    let mut app = app_at(Path::new("."));
    app.on_key(ctrl('o'));
    assert!(app.file_browser.is_some());
    // Ctrl+O inside the browser swaps to the classic type-a-path prompt.
    app.on_key(ctrl('o'));
    assert!(app.file_browser.is_none(), "browser closed");
    assert!(app.prompt.is_some(), "path prompt opened");
    app.on_key(esc());
    assert!(app.prompt.is_none(), "Esc closes the prompt");
}

#[test]
fn ctrl_b_toggles_explorer() {
    let mut app = app_at(Path::new("."));
    let before = app.show_explorer;
    app.on_key(ctrl('b'));
    assert_ne!(app.show_explorer, before);
    app.on_key(ctrl('b'));
    assert_eq!(app.show_explorer, before);
}

#[test]
fn ctrl_e_toggles_focus_between_editor_and_explorer() {
    let mut app = app_at(Path::new("."));
    assert_eq!(app.focus, Focus::Editor);
    app.on_key(ctrl('e'));
    assert_eq!(app.focus, Focus::Explorer);
    app.on_key(ctrl('e'));
    assert_eq!(app.focus, Focus::Editor);
}

#[test]
fn open_calendar_swallows_editor_clicks() {
    let mut app = app_at(Path::new("."));
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app.focus = Focus::Explorer;
    app.run_action("tools.calendar"); // open the calendar overlay
    assert!(app.show_calendar);
    // A click over the editor must not reach it while the calendar is open.
    app.on_mouse(click(5, 3));
    assert_eq!(
        app.focus,
        Focus::Explorer,
        "calendar open swallows the editor click"
    );
}

#[test]
fn click_explorer_row_focuses_and_selects() {
    let dir = unique_dir("clickexp");
    fs::write(dir.join("a.txt"), "1").unwrap();
    fs::write(dir.join("b.txt"), "2").unwrap();
    let mut app = app_at(&dir);
    app.show_explorer = true;
    app.layout.explorer = Rect::new(0, 0, 30, 20);
    // Rows start one below the top border (explorer.y + 1). Click the SECOND row;
    // the first is already selected, where a click would promote/open the file.
    app.on_mouse(click(5, 2));
    assert_eq!(app.focus, Focus::Explorer);
    assert_eq!(app.explorer.selected, 1, "clicked the second row");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn drag_explorer_right_edge_resizes_left_dock() {
    let mut app = app_at(Path::new("."));
    app.show_explorer = true;
    app.layout.menu = Rect::new(0, 0, 100, 1); // full width 100
    app.layout.explorer = Rect::new(0, 0, 30, 24); // right border at column 29
    let before = app.settings.explorer_width;
    app.on_mouse(click(29, 0)); // grab the right edge
    app.on_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 50, 0));
    assert!(
        app.settings.explorer_width > before,
        "dragging right widens the dock"
    );
    app.on_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 50, 0));
    // After releasing, a drag elsewhere no longer resizes.
    let stable = app.settings.explorer_width;
    app.on_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 20, 0));
    assert_eq!(
        app.settings.explorer_width, stable,
        "release ends the resize"
    );
}
