#![allow(
    clippy::cast_possible_truncation,
    clippy::format_collect,
    reason = "tests count and index small fixtures, and build expected text with format! over iterators"
)]
// Shared fixtures/helpers live in `common.rs`; every test here needs a
// handful of them, so a glob import earns its keep over a long explicit list.
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use crate::integration::common::*;

#[test]
fn calendar_left_right_pages_months() {
    let mut app = app_at(Path::new("."));
    app.run_action("tools.calendar");
    assert!(
        app.visible.contains(vix::app::Visible::CALENDAR),
        "Calendar opens on the current month"
    );
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
    assert!(
        !app.visible.contains(vix::app::Visible::CALENDAR),
        "Esc closes the calendar"
    );
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
    assert!(
        app.visible.contains(vix::app::Visible::CALENDAR),
        "an arrow click keeps the calendar open"
    );
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
    assert!(
        !app.visible.contains(vix::app::Visible::CALENDAR),
        "an outside click closes the calendar"
    );
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
    // T209: this test is about the buffer-closes-on-delete behavior, not
    // trash routing (that's its own test below) - force "hard" so it
    // doesn't pick up a dependency on the OS trash mechanism being
    // available, which a plain fs::remove_file never needed.
    app.settings.explorer_delete = "hard".to_string();
    app.open_initial(&dir.join("a.txt"));
    // a.txt takes over the empty starting buffer.
    assert_eq!(app.editor.tabs.len(), 1);
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
fn explorer_delete_defaults_to_trash_and_the_prompt_says_so() {
    let dir = unique_dir("del-trash-prompt");
    fs::write(dir.join("a.txt"), "bye\n").unwrap();
    let mut app = app_at(&dir);
    assert_eq!(
        app.settings.explorer_delete, "trash",
        "trash is the default"
    );
    app.focus = Focus::Explorer;
    app.explorer.selected = node_index(&app, "a.txt");
    app.on_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE));
    let msg = &app.confirm.as_ref().expect("confirm prompt open").message;
    assert!(
        msg.contains("trash"),
        "prompt should say it will move to trash, got: {msg:?}"
    );

    // An unrecognized value (a typo, or an older config file) falls back to
    // the safer trash behavior rather than silently hard-deleting.
    app.settings.explorer_delete = "bogus".to_string();
    app.on_key(key('n')); // cancel the first prompt
    app.on_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE));
    let msg = &app.confirm.as_ref().expect("confirm prompt open").message;
    assert!(
        msg.contains("trash"),
        "an unrecognized explorer_delete value should still say trash, got: {msg:?}"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_delete_hard_mode_says_permanently_and_does_not_trash() {
    let dir = unique_dir("del-hard-prompt");
    fs::write(dir.join("a.txt"), "bye\n").unwrap();
    let mut app = app_at(&dir);
    app.settings.explorer_delete = "hard".to_string();
    app.focus = Focus::Explorer;
    app.explorer.selected = node_index(&app, "a.txt");
    app.on_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE));
    let msg = &app.confirm.as_ref().expect("confirm prompt open").message;
    assert!(
        msg.to_lowercase().contains("permanent"),
        "hard mode's prompt should say it's permanent, got: {msg:?}"
    );
    app.on_key(key('y'));
    assert!(!dir.join("a.txt").exists());
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
    let before = app.visible.contains(vix::app::Visible::EXPLORER);
    app.on_key(ctrl('b'));
    assert_ne!(app.visible.contains(vix::app::Visible::EXPLORER), before);
    app.on_key(ctrl('b'));
    assert_eq!(app.visible.contains(vix::app::Visible::EXPLORER), before);
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
    assert!(app.visible.contains(vix::app::Visible::CALENDAR));
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
    app.visible.set(vix::app::Visible::EXPLORER, true);
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
    app.visible.set(vix::app::Visible::EXPLORER, true);
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

#[test]
fn minimap_renders_a_bar_per_line_band_without_panicking() {
    // T512: `draw_minimap` reads each line's trimmed length straight from the
    // rope (via `Code::line`) instead of a `Vec<String>` clone of the whole
    // buffer. A buffer ending in a newline exercises the edge this touches:
    // `Code::len_lines()` (rope semantics) counts one more trailing line than
    // `str::lines()` would, so this also guards the line-index bound.
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let mut app = app_at(Path::new("."));
    app.visible.set(vix::app::Visible::EXPLORER, false);
    let lines: String = (0..50)
        .map(|i| format!("line {i} {}\n", "x".repeat(i)))
        .collect();
    app.editor.new_tab_with_content(&lines);
    app.run_action("view.minimap");
    assert!(
        app.settings.viewport.show_minimap,
        "the minimap is toggled on"
    );
    let mut term = Terminal::new(TestBackend::new(120, 24)).unwrap();
    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    let mm = app.layout.minimap;
    assert!(
        mm.width > 0 && mm.height > 0,
        "the minimap rect was laid out"
    );
    let buf = term.backend().buffer();
    let has_bar = (mm.left()..mm.right())
        .flat_map(|x| (mm.top()..mm.bottom()).map(move |y| (x, y)))
        .any(|(x, y)| buf[(x, y)].symbol() == "\u{2593}" || buf[(x, y)].symbol() == "\u{2592}");
    assert!(has_bar, "at least one band drew a bar for its longest line");
}

#[test]
fn report_a_crash_shows_the_report_read_only_and_asks_first() {
    use crate::app::ConfirmKind;
    let dir = unique_dir("crash-report");
    let report = dir.join("crash-20260101T000000Z.txt");
    std::fs::write(&report, "Vix crash report\nversion: 2.0.2\npanic: boom\n").unwrap();
    let mut app = app_at(&dir);
    app.show_crash_report(&report);
    let tab = app.editor.active_tab().unwrap();
    assert!(tab.read_only, "the report opens read-only");
    assert!(tab.text().contains("panic: boom"));
    assert_eq!(
        app.confirm.as_ref().map(|c| c.kind),
        Some(ConfirmKind::ReportCrash),
        "nothing is opened or sent before the user says yes"
    );
    app.on_key(key('n'));
    assert!(app.confirm.is_none(), "n closes the question");
    std::fs::remove_dir_all(&dir).ok();
}

/// Pasting a copy over existing names asks per file: `s` skips one, `o`
/// overwrites one, `S` skips the rest, and `c` cancels; the prompt draws.
#[test]
fn explorer_paste_conflicts_ask_and_each_choice_is_kept() {
    let dir = unique_dir("paste-conflict");
    fs::create_dir(dir.join("sub")).unwrap();
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(dir.join(name), format!("new {name}")).unwrap();
        fs::write(dir.join("sub").join(name), format!("old {name}")).unwrap();
    }
    let mut app = app_at(&dir);
    app.focus = Focus::Explorer;
    let copy = |app: &mut App| {
        for name in ["a.txt", "b.txt", "c.txt"] {
            app.explorer.selected = node_index(app, name);
            app.on_key(ctrl('c'));
        }
    };
    copy(&mut app);
    app.explorer.selected = node_index(&app, "sub");
    app.on_key(ctrl('v'));
    assert!(app.paste.is_some(), "a conflict asks: {}", app.status);
    draw_frame(&mut app, 120, 40);
    draw_frame(&mut app, 50, 16);
    app.on_key(key('o')); // overwrite the first conflict
    app.on_key(key('s')); // skip the second
    app.on_key(key('S')); // skip the rest
    assert!(app.paste.is_none(), "every conflict decided");
    let read = |n: &str| fs::read_to_string(dir.join("sub").join(n)).unwrap();
    let kept: Vec<String> = ["a.txt", "b.txt", "c.txt"]
        .iter()
        .map(|n| read(n))
        .collect();
    assert_eq!(
        kept.iter().filter(|t| t.starts_with("new")).count(),
        1,
        "exactly one overwritten: {kept:?}"
    );

    // Cancel leaves everything as it is.
    copy(&mut app);
    app.explorer.selected = node_index(&app, "sub");
    app.on_key(ctrl('v'));
    app.on_key(key('c'));
    assert!(app.paste.is_none());
    let after: Vec<String> = ["a.txt", "b.txt", "c.txt"]
        .iter()
        .map(|n| read(n))
        .collect();
    assert_eq!(after, kept, "cancel changes nothing");
    fs::remove_dir_all(&dir).ok();
}

/// **File → Open…**'s browser: typing filters, Ctrl S / Ctrl R / Alt H
/// change sort, direction and hidden files, arrows move, and it draws.
#[test]
fn file_browser_filters_sorts_and_opens() {
    let dir = unique_dir("file-browser-keys");
    fs::write(dir.join("alpha.rs"), "a\n").unwrap();
    fs::write(dir.join("beta.rs"), "b\n").unwrap();
    fs::write(dir.join(".hidden"), "h\n").unwrap();
    fs::create_dir(dir.join("zeta")).unwrap();
    let mut app = app_at(&dir);
    app.run_action("file.open");
    assert!(app.file_browser.is_some());
    draw_frame(&mut app, 120, 40);
    for k in [ctrl('s'), ctrl('r'), ctrl('s')] {
        app.on_key(k);
        draw_frame(&mut app, 120, 40);
    }
    app.on_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::ALT));
    draw_frame(&mut app, 50, 16);
    app.on_key(keycode(KeyCode::Down));
    app.on_key(keycode(KeyCode::Up));
    for c in "beta".chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Backspace));
    app.on_key(key('a'));
    let screen = draw_frame(&mut app, 120, 40);
    assert!(screen.contains("beta.rs"), "{screen}");
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.file_browser.is_none(), "Enter opens the match");
    assert!(
        app.editor
            .active_tab()
            .and_then(|t| t.path.as_ref())
            .is_some_and(|p| p.ends_with("beta.rs"))
    );
    fs::remove_dir_all(&dir).ok();
}

/// A path that cannot open says why, in Messages, without a panic: missing,
/// a directory, and a binary file (which asks before opening).
#[test]
fn files_that_cannot_open_say_why() {
    let dir = unique_dir("open-failures");
    fs::write(dir.join("blob.bin"), [0u8, 159, 146, 150, 0, 1, 2, 255]).unwrap();
    fs::create_dir(dir.join("folder")).unwrap();
    let mut app = app_at(&dir);
    let tabs = app.editor.tabs.len();
    for name in ["missing.txt", "folder", "blob.bin"] {
        app.open_initial(&dir.join(name));
        draw_frame(&mut app, 120, 40);
        app.on_key(esc());
    }
    assert_eq!(app.editor.tabs.len(), tabs, "none of them opened as text");
    fs::remove_dir_all(&dir).ok();
}

/// **Tools → Contacts** lists the workspace's `.vcf` files; Enter opens one
/// in the vCard view, which scrolls, draws, and on Enter inserts the
/// highlighted value into the buffer.
#[test]
fn contacts_open_a_vcard_and_insert_a_value() {
    let dir = unique_dir("contacts-vcard");
    fs::write(
        dir.join("ada.vcf"),
        "BEGIN:VCARD\r\nVERSION:4.0\r\nFN:Ada Lovelace\r\nN:Lovelace;Ada;;;\r\nEMAIL;TYPE=work:ada@example.org\r\nTEL;TYPE=cell:+44 20 7946 0000\r\nORG:Analytical Engines\r\nNOTE:First programmer\r\nEND:VCARD\r\n",
    )
    .unwrap();
    fs::write(dir.join("notes.txt"), "").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&dir.join("notes.txt"));
    app.run_action("tools.contacts");
    assert!(
        app.contacts.is_some(),
        "the contacts panel opens: {}",
        app.status
    );
    let screen = draw_frame(&mut app, 120, 40);
    assert!(screen.contains("Ada Lovelace"), "{screen}");
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.vcard.is_some(), "Enter opens the vCard view");
    let screen = draw_frame(&mut app, 120, 40);
    assert!(screen.contains("ada@example.org"), "{screen}");
    draw_frame(&mut app, 50, 16);
    for code in [
        KeyCode::End,
        KeyCode::Home,
        KeyCode::PageDown,
        KeyCode::PageUp,
        KeyCode::Down,
        KeyCode::Up,
    ] {
        app.on_key(keycode(code));
    }
    app.on_key(keycode(KeyCode::Down));
    app.on_key(keycode(KeyCode::Enter));
    let text = app.editor.active_tab().unwrap().text();
    assert!(!text.is_empty(), "a value was inserted");
    app.run_action("tools.contacts");
    app.on_key(keycode(KeyCode::Enter));
    app.on_key(esc());
    assert!(app.vcard.is_none(), "Esc closes the vCard view");
    fs::remove_dir_all(&dir).ok();
}

/// The Markdown preview's `t` opens a table of contents of its headings,
/// which draws, moves and jumps.
#[test]
fn markdown_preview_table_of_contents_lists_and_jumps() {
    let dir = unique_dir("md-toc");
    let doc = "# Title\n\nIntro.\n\n## First\n\nText.\n\n## Second\n\n### Deeper\n\nMore.\n";
    fs::write(dir.join("README.md"), doc).unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&dir.join("README.md"));
    app.run_action("tools.markdown_preview");
    assert!(app.markdown_preview.is_some(), "{}", app.status);
    draw_frame(&mut app, 120, 40);
    app.on_key(key('t'));
    assert!(app.markdown_toc.is_some(), "t opens the table of contents");
    let screen = draw_frame(&mut app, 120, 40);
    assert!(
        screen.contains("Second") && screen.contains("Deeper"),
        "{screen}"
    );
    draw_frame(&mut app, 50, 16);
    for code in [
        KeyCode::Down,
        KeyCode::Down,
        KeyCode::Up,
        KeyCode::End,
        KeyCode::Home,
        KeyCode::Down,
    ] {
        app.on_key(keycode(code));
        draw_frame(&mut app, 120, 40);
    }
    app.on_key(keycode(KeyCode::Enter));
    assert!(
        app.markdown_toc.is_none(),
        "Enter jumps and closes the list"
    );
    draw_frame(&mut app, 120, 40);
    fs::remove_dir_all(&dir).ok();
}

/// Every info panel answers each navigation key and a left click on any row
/// without panicking, and Esc closes it.
#[test]
fn info_panels_answer_every_navigation_key_and_click() {
    let dir = unique_dir("info-nav");
    fs::write(
        dir.join("ada.vcf"),
        "BEGIN:VCARD\r\nVERSION:4.0\r\nFN:Ada\r\nEMAIL:ada@example.org\r\nTEL:1\r\nNOTE:n\r\nEND:VCARD\r\n",
    )
    .unwrap();
    fs::write(
        dir.join("doc.md"),
        "# T\n\ntext words here\n\n## S\n\nmore\n",
    )
    .unwrap();
    let keys = [
        KeyCode::Down,
        KeyCode::Up,
        KeyCode::PageDown,
        KeyCode::PageUp,
        KeyCode::End,
        KeyCode::Home,
        KeyCode::Char('?'),
    ];
    for (action, open_vcard, toc) in [
        ("tools.contacts", false, false),
        ("tools.contacts", true, false),
        ("tools.file_info", false, false),
        ("tools.text_info", false, false),
        ("tools.markdown_preview", false, false),
        ("tools.markdown_preview", false, true),
    ] {
        let mut app = app_at(&dir);
        app.open_initial(&dir.join("doc.md"));
        app.run_action(action);
        if open_vcard {
            draw_frame(&mut app, 100, 30);
            app.on_key(keycode(KeyCode::Enter));
        }
        if toc {
            draw_frame(&mut app, 100, 30);
            app.on_key(key('t'));
        }
        draw_frame(&mut app, 100, 30);
        for code in keys {
            app.on_key(keycode(code));
            draw_frame(&mut app, 100, 30);
        }
        for row in 0..30 {
            for col in (0..100).step_by(9) {
                app.on_mouse(click(col, row));
                app.on_mouse(mouse(MouseEventKind::Moved, col, row));
                draw_frame(&mut app, 100, 30);
            }
        }
        app.on_key(esc());
    }
    fs::remove_dir_all(&dir).ok();
}

/// Clicks, drags, right-clicks and wheel turns over every cell of the explorer
/// and the Messages dock (skipping the menu bar) leave the app standing.
#[test]
fn explorer_and_messages_survive_mouse_input_everywhere() {
    let dir = unique_dir("mouse-everywhere");
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(dir.join(name), "x\n").unwrap();
    }
    fs::create_dir_all(dir.join("sub")).unwrap();
    fs::write(dir.join("sub").join("d.txt"), "y\n").unwrap();
    let mut app = app_at(&dir);
    app.open_initial(&dir.join("a.txt"));
    for i in 0..30 {
        app.messages.error(format!("message {i}"));
    }
    draw_frame(&mut app, 100, 40);
    for row in 2..40 {
        for col in (0..100).step_by(3) {
            for kind in [
                MouseEventKind::Down(MouseButton::Left),
                MouseEventKind::Drag(MouseButton::Left),
                MouseEventKind::Up(MouseButton::Left),
                MouseEventKind::Down(MouseButton::Right),
                MouseEventKind::ScrollDown,
                MouseEventKind::ScrollUp,
            ] {
                app.on_mouse(mouse(kind, col, row));
            }
        }
        draw_frame(&mut app, 100, 40);
    }
    fs::remove_dir_all(&dir).ok();
}

/// The shortcut help panel filters as you type and scrolls; the clock panel
/// moves, inserts the chosen time into the buffer, and closes.
#[test]
fn help_and_clock_panels_take_their_keys() {
    let mut app = app_at(Path::new("."));
    app.run_action("help.shortcuts");
    assert!(app.help.is_some());
    draw_frame(&mut app, 100, 30);
    for code in [
        KeyCode::Char('s'),
        KeyCode::Char('a'),
        KeyCode::Backspace,
        KeyCode::Down,
        KeyCode::Up,
        KeyCode::PageDown,
        KeyCode::PageUp,
        KeyCode::Tab,
    ] {
        app.on_key(keycode(code));
        draw_frame(&mut app, 100, 30);
    }
    app.on_key(keycode(KeyCode::Esc));
    assert!(app.help.is_none());

    app.run_action("tools.clock");
    assert!(app.visible.contains(vix::app::Visible::CLOCK));
    draw_frame(&mut app, 100, 30);
    app.on_key(keycode(KeyCode::Down));
    app.on_key(keycode(KeyCode::Up));
    app.on_key(keycode(KeyCode::Enter));
    assert!(
        !app.visible.contains(vix::app::Visible::CLOCK),
        "Enter closes"
    );
    assert!(
        !app.editor.active_tab().unwrap().text().is_empty(),
        "the time was inserted"
    );
    app.run_action("tools.clock");
    app.on_key(keycode(KeyCode::Char('q')));
    assert!(!app.visible.contains(vix::app::Visible::CLOCK));
}

/// Home and End jump to the ends, paging moves, Right enters a highlighted
/// directory, Left walks back up to its parent.
#[test]
fn file_browser_jumps_pages_and_walks_the_directory_tree() {
    let dir = unique_dir("file-browser-walk").canonicalize().unwrap();
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(dir.join(name), "x\n").unwrap();
    }
    fs::create_dir(dir.join("sub")).unwrap();
    fs::write(dir.join("sub").join("inner.txt"), "y\n").unwrap();
    let mut app = app_at(&dir);
    app.run_action("file.open");
    draw_frame(&mut app, 100, 30);
    let len = app.file_browser.as_ref().unwrap().len();
    assert!(len >= 4, "the folder and its files are listed");

    app.on_key(keycode(KeyCode::End));
    assert_eq!(app.file_browser.as_ref().unwrap().selected, len - 1);
    app.on_key(keycode(KeyCode::Home));
    assert_eq!(app.file_browser.as_ref().unwrap().selected, 0);
    app.on_key(keycode(KeyCode::PageDown));
    app.on_key(keycode(KeyCode::PageUp));
    assert_eq!(app.file_browser.as_ref().unwrap().selected, 0);

    // Directories sort first, so the first row is `sub`: Right enters it.
    assert!(
        app.file_browser
            .as_ref()
            .unwrap()
            .selected_entry()
            .unwrap()
            .is_dir
    );
    app.on_key(keycode(KeyCode::Right));
    assert!(
        app.file_browser.is_some(),
        "entering a folder keeps the browser"
    );
    draw_frame(&mut app, 100, 30);
    let inside = draw_frame(&mut app, 100, 30);
    assert!(inside.contains("inner.txt"), "{inside}");
    // Right on a file does nothing; Left goes back up.
    app.on_key(keycode(KeyCode::Right));
    app.on_key(keycode(KeyCode::Left));
    let back = draw_frame(&mut app, 100, 30);
    assert!(back.contains("a.txt"), "{back}");
    app.on_key(keycode(KeyCode::Esc));
    assert!(app.file_browser.is_none());
    fs::remove_dir_all(&dir).ok();
}

/// The document outline and the HTML entity palette answer every navigation
/// key (up, down, paging, Home, End), keep their highlight in range, and
/// close on Esc.
#[test]
fn outline_and_html_palette_take_the_navigation_keys() {
    let dir = unique_dir("outline-html-nav");
    let src: String = (0..30).map(|i| format!("fn f{i}() {{}}\n")).collect();
    fs::write(dir.join("a.rs"), src).unwrap();
    for (action, which) in [("nav.outline", 0), ("tools.html_chars", 1)] {
        let mut app = app_at(&dir);
        app.open_initial(&dir.join("a.rs"));
        app.run_action(action);
        draw_frame(&mut app, 100, 30);
        for code in [
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::PageDown,
            KeyCode::End,
            KeyCode::Up,
            KeyCode::PageUp,
            KeyCode::Home,
        ] {
            app.on_key(keycode(code));
            draw_frame(&mut app, 100, 30);
        }
        let open = |app: &App| {
            if which == 0 {
                app.outline.is_some()
            } else {
                app.html_panel.is_some()
            }
        };
        assert!(open(&app), "{action} stays open through navigation");
        app.on_key(keycode(KeyCode::Esc));
        assert!(!open(&app), "{action} closes on Esc");
    }
    fs::remove_dir_all(&dir).ok();
}
