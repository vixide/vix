//! T631: several integrated terminals as tabs, their scrollback, and mouse
//! wheel scrolling. Real shells on real PTYs (Unix), like the T697 test.
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use crate::integration::common::*;

/// Type `line` and Enter into whatever takes keys.
fn run_line(app: &mut App, line: &str) {
    type_str(app, line);
    app.on_key(keycode(KeyCode::Enter));
}

/// Wait (10 s at most) until the terminal at tab `index` shows `want`.
fn until_tab_shows(app: &mut App, index: usize, want: &str) {
    wait_until(
        app,
        |_| {},
        |a| {
            a.terminal_screen_text_at(index)
                .is_some_and(|t| t.contains(want))
        },
        Duration::from_secs(10),
    );
}

/// Wait until the active terminal's screen has not changed for 300 ms (the
/// shell has drawn its next prompt), so scroll offsets are not bumped by
/// late output mid-assertion.
fn settle(app: &App) {
    let mut last = app.terminal_screen_text();
    let mut still = Instant::now();
    let deadline = Instant::now() + Duration::from_secs(10);
    while still.elapsed() < Duration::from_millis(300) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
        let now = app.terminal_screen_text();
        if now != last {
            last = now;
            still = Instant::now();
        }
    }
}

fn with_mods(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

/// Two terminals: each gets only its own keys; Ctrl+PgUp/PgDn and the
/// `terminal.*` actions switch; Ctrl+] hides without stopping either shell;
/// `terminal.close` stops the active one, and the overlay goes with the last.
#[test]
fn several_terminals_each_get_their_own_keys() {
    let dir = unique_dir("t631-tabs");
    let mut app = app_at(&dir);

    app.run_action("tools.terminal");
    assert_eq!(app.terminal_count(), 1);
    assert!(app.terminal_visible());
    app.run_action("terminal.new");
    assert_eq!(app.terminal_count(), 2);
    assert_eq!(app.terminal_active(), 1, "a new terminal is active");

    // `expr` prints a sum only the shell computes, never the typed echo.
    run_line(&mut app, "expr 111000 + 111");
    until_tab_shows(&mut app, 1, "111111");

    app.on_key(with_mods(KeyCode::PageUp, KeyModifiers::CONTROL));
    assert_eq!(app.terminal_active(), 0);
    run_line(&mut app, "expr 222000 + 222");
    until_tab_shows(&mut app, 0, "222222");
    let first = app.terminal_screen_text_at(0).unwrap();
    let second = app.terminal_screen_text_at(1).unwrap();
    assert!(!first.contains("111111"), "{first}");
    assert!(!second.contains("222222"), "{second}");

    app.on_key(with_mods(KeyCode::PageDown, KeyModifiers::CONTROL));
    assert_eq!(app.terminal_active(), 1);
    app.run_action("terminal.prev");
    assert_eq!(app.terminal_active(), 0);

    // Ctrl+] hides the overlay; both shells keep running.
    app.on_key(ctrl(']'));
    assert!(!app.terminal_visible());
    assert!(app.terminal_running());
    assert_eq!(app.terminal_count(), 2);
    app.run_action("tools.terminal");
    assert!(app.terminal_visible(), "shown again, not a third shell");
    assert_eq!(app.terminal_count(), 2);
    assert_eq!(app.terminal_active(), 0);

    app.run_action("terminal.next");
    app.run_action("terminal.close");
    assert_eq!(app.terminal_count(), 1);
    assert_eq!(app.terminal_active(), 0);
    assert!(
        app.terminal_screen_text()
            .is_some_and(|t| t.contains("222222")),
        "the first terminal is the one left"
    );
    app.run_action("terminal.close");
    assert_eq!(app.terminal_count(), 0);
    assert!(!app.terminal_visible());
    assert!(!app.terminal_running());
    // Nothing to close, switch or copy: no panic.
    app.run_action("terminal.close");
    app.run_action("terminal.next");
    app.run_action("terminal.copy");
    assert_eq!(app.terminal_count(), 0);
}

/// Shift+PgUp/PgDn page the scrollback, Shift+Home stops at the configured
/// cap, Shift+End and typing return to the live screen, and the wheel
/// scrolls three lines.
#[test]
fn the_scrollback_scrolls_and_is_capped() {
    let dir = unique_dir("t631-scroll");
    let mut settings = Settings::default();
    settings.terminal_scrollback = 100;
    let mut app = app_at_with(&dir, settings);
    app.run_action("tools.terminal");
    // Many more lines than the screen and the cap together; the marker is
    // computed, so only the shell's output can contain it.
    run_line(&mut app, "seq 1 400; expr 630000 + 1");
    until_tab_shows(&mut app, 0, "630001");
    settle(&app);
    assert_eq!(app.terminal_scroll_offset(), 0);

    let shift = |code| with_mods(code, KeyModifiers::SHIFT);
    // The PTY is 24 rows here (no frame drawn): a page is 23 lines.
    app.on_key(shift(KeyCode::PageUp));
    assert_eq!(app.terminal_scroll_offset(), 23);
    let back = app.terminal_screen_text().unwrap();
    assert!(
        !back.contains("630001"),
        "scrolled away from the end:\n{back}"
    );
    app.on_key(shift(KeyCode::PageDown));
    assert_eq!(app.terminal_scroll_offset(), 0);

    // The top of the scrollback is the cap, however much scrolled off.
    app.on_key(shift(KeyCode::Home));
    assert_eq!(app.terminal_scroll_offset(), 100);
    app.on_key(shift(KeyCode::End));
    assert_eq!(app.terminal_scroll_offset(), 0);

    // The wheel over the terminal's screen scrolls it.
    app.layout.terminal = Rect::new(1, 1, 78, 22);
    app.on_mouse(mouse(MouseEventKind::ScrollUp, 10, 10));
    assert_eq!(app.terminal_scroll_offset(), 3);
    app.on_mouse(mouse(MouseEventKind::ScrollDown, 10, 10));
    assert_eq!(app.terminal_scroll_offset(), 0);

    // Typing returns to the live screen first.
    app.on_key(shift(KeyCode::PageUp));
    assert!(app.terminal_scroll_offset() > 0);
    app.on_key(key(' '));
    assert_eq!(app.terminal_scroll_offset(), 0);
    app.run_action("terminal.close");
}

/// The (column, row) of the first screen cell where `needle` starts.
fn find_on_screen(app: &App, needle: &str) -> (u16, u16) {
    let screen = app.terminal_screen_text().unwrap_or_default();
    for (row, line) in screen.lines().enumerate() {
        if let Some(byte) = line.find(needle) {
            let col = line[..byte].chars().count();
            return (u16::try_from(col).unwrap(), u16::try_from(row).unwrap());
        }
    }
    panic!("{needle:?} is not on the terminal screen:\n{screen}");
}

/// A Ctrl+click at (`col`, `row`) of the terminal's screen.
fn ctrl_click(app: &mut App, col: u16, row: u16) {
    app.layout.terminal = Rect::new(0, 0, 80, 24);
    let mut event = click(col, row);
    event.modifiers = KeyModifiers::CONTROL;
    app.on_mouse(event);
}

/// Ctrl+click on `path:line:col` in terminal output opens it there, resolved
/// against the shell's working directory, which OSC 7 keeps up to date (T666).
#[test]
fn ctrl_click_opens_a_path_line_resolved_against_the_osc7_directory() {
    let dir = unique_dir("t666-open");
    fs::create_dir_all(dir.join("sub")).unwrap();
    fs::write(dir.join("top.txt"), "one\ntwo\nthree\n").unwrap();
    fs::write(dir.join("sub/inner.txt"), "a\nb\ncc\nd\n").unwrap();
    let mut app = app_at(&dir);
    app.run_action("tools.terminal");

    // A path relative to the starting directory (the workspace root).
    run_line(&mut app, "echo see top.txt:2:1 now");
    until_tab_shows(&mut app, 0, "see top.txt:2:1 now");
    settle(&app);
    let (col, row) = find_on_screen(&app, "top.txt");
    ctrl_click(&mut app, col + 2, row);
    assert!(!app.terminal_visible(), "opening hides the overlay");
    assert!(
        app.editor
            .active_tab()
            .and_then(|t| t.path.as_ref())
            .is_some_and(|p| p.ends_with("top.txt"))
    );
    assert_eq!(app.editor.cursor_line(), 1);

    // After `cd sub` and an OSC 7 report, `inner.txt` resolves under `sub`.
    app.run_action("tools.terminal");
    run_line(&mut app, "cd sub");
    run_line(
        &mut app,
        r#"printf '\033]7;file://localhost%s\007' "$PWD"; echo at inner.txt:3 end"#,
    );
    until_tab_shows(&mut app, 0, "at inner.txt:3 end");
    settle(&app);
    let (col, row) = find_on_screen(&app, "at inner.txt:3 end");
    ctrl_click(&mut app, col + 5, row);
    assert!(
        app.editor
            .active_tab()
            .and_then(|t| t.path.as_ref())
            .is_some_and(|p| p.ends_with("sub/inner.txt")),
        "resolved against the OSC 7 directory"
    );
    assert_eq!(app.editor.cursor_line(), 2);

    // A click on plain words opens nothing and leaves the overlay alone.
    app.run_action("tools.terminal");
    let (col, row) = find_on_screen(&app, "at inner.txt:3 end");
    ctrl_click(&mut app, col, row);
    assert!(app.terminal_visible());
    app.run_action("terminal.close");
}

/// Send to Terminal sends the selection, or the current line, as a command.
#[test]
fn send_selection_runs_the_current_line_in_the_terminal() {
    let dir = unique_dir("t666-send");
    fs::write(dir.join("cmds.sh"), "echo first\nexpr 660000 + 6\n").unwrap();
    let mut app = app_at(&dir);
    app.editor.open(&dir.join("cmds.sh"), false).unwrap();
    app.editor.goto(2, Some(1), Rect::new(0, 0, 80, 24));
    assert_eq!(app.terminal_count(), 0);
    app.run_action("terminal.send_selection");
    assert_eq!(app.terminal_count(), 1, "a terminal is opened for it");
    assert!(app.terminal_visible());
    until_tab_shows(&mut app, 0, "660006");
    app.run_action("terminal.close");

    // Nothing to send without a buffer: no terminal, no panic.
    let mut empty = app_at(&dir);
    empty.run_action("terminal.send_selection");
    assert_eq!(empty.terminal_count(), 0);
}

/// An exited shell keeps its tab; Enter restarts it in place with the old
/// transcript above the new prompt; any other key closes it instead.
#[test]
fn an_exited_terminal_restarts_in_place_keeping_its_transcript() {
    let dir = unique_dir("t666-restart");
    let mut app = app_at(&dir);
    app.run_action("tools.terminal");
    run_line(&mut app, "expr 660000 + 60");
    until_tab_shows(&mut app, 0, "660060");
    run_line(&mut app, "exit");
    wait_until(
        &mut app,
        App::poll_terminal,
        |a| !a.terminal_alive(),
        Duration::from_secs(10),
    );
    assert_eq!(app.terminal_count(), 1, "the exited tab stays");
    assert!(app.terminal_visible());

    app.on_key(keycode(KeyCode::Enter));
    assert!(app.terminal_alive(), "Enter restarted the shell");
    assert_eq!(app.terminal_count(), 1);
    run_line(&mut app, "expr 770000 + 7");
    until_tab_shows(&mut app, 0, "770007");
    let screen = app.terminal_screen_text().unwrap();
    assert!(
        screen.contains("660060"),
        "the transcript is kept:\n{screen}"
    );

    // The explicit action restarts a live shell too.
    app.run_action("terminal.restart");
    assert!(app.terminal_alive());
    assert_eq!(app.terminal_count(), 1);

    // Another key on an exited terminal closes it.
    run_line(&mut app, "exit");
    wait_until(
        &mut app,
        App::poll_terminal,
        |a| !a.terminal_alive(),
        Duration::from_secs(10),
    );
    app.on_key(key('q'));
    assert_eq!(app.terminal_count(), 0);
    // Nothing to restart: no panic.
    app.run_action("terminal.restart");
}

/// With capture off, Ctrl chords stay with the editor instead of reaching the
/// shell; toggling again gives them back (T666).
#[test]
fn key_capture_toggle_decides_who_gets_ctrl_chords() {
    let dir = unique_dir("t666-capture");
    let mut app = app_at(&dir);
    app.run_action("tools.terminal");
    type_str(&mut app, "expr 880000 +");
    app.on_key(ctrl('\\'));
    // Ctrl+A would move the shell's cursor to the line start (and break the
    // command); with capture off it must not get there.
    app.on_key(ctrl('a'));
    app.run_action("terminal.toggle_capture");
    type_str(&mut app, " 8");
    app.on_key(keycode(KeyCode::Enter));
    until_tab_shows(&mut app, 0, "880008");
    app.run_action("terminal.close");
}
