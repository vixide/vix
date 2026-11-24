//! Terminal overlay behaviour that needs no live shell: the no-terminal paths,
//! sizing and mouse routing. (Anything that starts a PTY child is exercised by
//! the real-shell tests, not here, to keep these hermetic.)

use super::*;
use crate::app::cov4::{app_at, buffer_with, code, key, temp};
use ratatui::layout::Rect;
use vix_input::MouseEvent;

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn a_new_terminal_starts_at_the_editor_size_or_a_conventional_24x80() {
    let mut app = app_at(&temp("term-size"));
    app.layout.editor = Rect::new(0, 0, 0, 0);
    assert_eq!(app.terminal_start_size(), (24, 80));
    app.layout.editor = Rect::new(0, 0, 0, 10);
    assert_eq!(
        app.terminal_start_size(),
        (24, 80),
        "zero width counts as unsized"
    );
    app.layout.editor = Rect::new(2, 3, 100, 30);
    assert_eq!(app.terminal_start_size(), (30, 100));
}

#[test]
fn terminal_commands_without_a_terminal_say_there_is_none() {
    let mut app = app_at(&temp("term-none"));
    let none = t!("status.terminal_none").to_string();
    assert!(!app.terminal_running());
    assert!(!app.terminal_alive());
    assert!(!app.terminal_visible());
    assert_eq!(app.terminal_count(), 0);
    assert_eq!(app.terminal_active(), 0);
    assert_eq!(app.terminal_scroll_offset(), 0);
    assert_eq!(app.terminal_screen_text(), None);
    assert_eq!(app.terminal_screen_text_at(0), None);

    app.terminal_toggle_capture();
    assert_eq!(app.status, none);
    app.status.clear();
    app.terminal_restart();
    assert_eq!(app.status, none);
    app.status.clear();
    app.terminal_copy();
    assert_eq!(app.status, none);
    assert_eq!(app.terminal_location_at(0, 0), None);
    assert!(!app.terminal_open_at(0, 0));
    app.poll_terminal();
    assert_ne!(app.status, t!("status.terminal_exited").to_string());
}

#[test]
fn sending_nothing_to_a_terminal_says_so_and_opens_none() {
    let mut app = app_at(&temp("term-send-empty"));
    buffer_with(&mut app, "   \n", 0);
    app.terminal_send_selection();
    assert_eq!(
        app.status,
        t!("status.terminal_nothing_to_send").to_string()
    );
    assert_eq!(app.terminal_count(), 0);
}

#[test]
fn the_mouse_outside_the_terminal_area_or_without_a_terminal_is_not_taken() {
    let mut app = app_at(&temp("term-mouse"));
    app.layout.terminal = Rect::new(10, 5, 40, 10);
    assert!(!app.terminal_mouse(mouse(MouseEventKind::ScrollUp, 0, 0)));
    // Inside the area but with no terminal open: still not taken.
    assert!(!app.terminal_mouse(mouse(MouseEventKind::ScrollUp, 12, 6)));
    // A Ctrl+click with nothing open under the pointer falls through the same way.
    let mut ctrl_click = mouse(MouseEventKind::Down(MouseButton::Left), 12, 6);
    ctrl_click.modifiers = KeyModifiers::CONTROL;
    assert!(!app.terminal_mouse(ctrl_click));
}

#[test]
fn keys_that_toggle_visibility_work_without_a_shell() {
    let mut app = app_at(&temp("term-keys"));
    // Ctrl+] hides the overlay even when nothing is open.
    assert!(app.terminal_key(crate::app::cov4::ctrl(']')));
    assert!(!app.terminal_visible());
    // With no active terminal an ordinary key is swallowed by the overlay.
    assert!(app.terminal_key(key('x')));
    assert!(app.terminal_key(code(KeyCode::Enter)));
}
