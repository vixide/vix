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
