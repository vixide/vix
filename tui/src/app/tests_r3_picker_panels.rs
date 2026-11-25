//! Round-3 tests of the list-panel glue: every key and click is inert with
//! no panel open, selecting a color for the theme editor applies it, and
//! the Home/End jumps work.

use ratatui::layout::Rect;
use vix_input::KeyCode;

use crate::app::cov4::{app_at, buffer_with, click, code, content, temp};
use crate::app::info_panel::InfoSlot;
use crate::app::{App, AppFlags};

fn fresh(tag: &str) -> App {
    let mut app = app_at(&temp(tag));
    buffer_with(&mut app, "", 0);
    app.layout.nerd_palette = Rect::new(0, 0, 40, 10);
    app.layout.ascii_panel = Rect::new(0, 0, 40, 10);
    app.layout.x11_panel = Rect::new(0, 0, 40, 10);
    app.layout.media_type_panel = Rect::new(0, 0, 40, 10);
    app.layout.todo_tree = Rect::new(0, 0, 40, 10);
    app.layout.theme_editor = Rect::new(0, 0, 40, 10);
    app
}

#[test]
fn keys_and_clicks_are_inert_while_their_panel_is_closed() {
    let mut app = fresh("pp3-inert");
    let status = app.status.clone();
    let keys = [
        KeyCode::Up,
        KeyCode::Down,
        KeyCode::Left,
        KeyCode::Right,
        KeyCode::Home,
        KeyCode::End,
        KeyCode::Enter,
        KeyCode::Backspace,
        KeyCode::Char('x'),
        KeyCode::Char(' '),
    ];
    for k in keys {
        app.nerd_key(code(k));
        app.ascii_key(code(k));
        app.x11_key(code(k));
        app.media_type_key(code(k));
        app.todo_tree_key(code(k));
        app.theme_editor_key(code(k));
    }
    app.nerd_mouse(click(1, 1));
    app.ascii_mouse(click(1, 1));
    app.x11_mouse(click(1, 1));
    app.media_type_mouse(click(1, 1));
    app.todo_tree_mouse(click(1, 1));
    app.theme_editor_mouse(click(1, 1));
    assert_eq!(app.status, status);
    assert_eq!(content(&mut app), "");
    assert!(app.info.nerd_palette().is_none());
    assert!(app.info.ascii_panel().is_none());
    assert!(app.x11_panel.is_none());
    assert!(app.theme_editor.is_none());
    assert!(app.todo_tree.is_none());
}

#[test]
fn nerd_palette_arrows_move_the_highlight() {
    let mut app = fresh("pp3-nerd");
    app.open_nerd_palette();
    let name = |app: &App| {
        app.info
            .nerd_palette()
            .unwrap()
            .selected_name()
            .map(str::to_string)
    };
    let start = name(&app);
    app.nerd_key(code(KeyCode::Right));
    let right = name(&app);
    assert_ne!(right, start, "Right moves to the next cell");
    app.nerd_key(code(KeyCode::Left));
    assert_eq!(name(&app), start);
    app.nerd_key(code(KeyCode::Down));
    let down = name(&app);
    assert_ne!(down, start, "Down moves a row");
    app.nerd_key(code(KeyCode::Up));
    assert_eq!(name(&app), start);
}

#[test]
fn ascii_home_and_end_jump_to_the_ends() {
    let mut app = fresh("pp3-ascii");
    app.open_ascii_panel();
    app.ascii_key(code(KeyCode::End));
    let last = app.info.ascii_panel().unwrap().selected_char();
    app.ascii_key(code(KeyCode::Home));
    let first = app.info.ascii_panel().unwrap().selected_char();
    assert_ne!(first, last);
    app.ascii_key(code(KeyCode::Enter));
    assert_eq!(content(&mut app), first.to_string());
}

#[test]
fn picking_a_color_for_the_theme_editor_applies_it_and_returns_to_the_editor() {
    let mut app = fresh("pp3-pick");
    app.open_theme_editor();
    assert!(app.theme_editor.is_some());
    let before = crate::theme_model::to_json(&app.theme_editor.as_ref().unwrap().theme).unwrap();
    app.theme_editor_key(code(KeyCode::Enter));
    assert!(app.flags.contains(AppFlags::THEME_EDITOR_PICKING));
    // Move to a color that is surely not the slot's current one, then pick it.
    for _ in 0..7 {
        app.x11_key(code(KeyCode::Down));
    }
    app.x11_key(code(KeyCode::Enter));
    assert!(app.x11_panel.is_none(), "the picker closes after a pick");
    assert!(!app.flags.contains(AppFlags::THEME_EDITOR_PICKING));
    assert!(app.theme_editor.is_some(), "back in the theme editor");
    let after = crate::theme_model::to_json(&app.theme_editor.as_ref().unwrap().theme).unwrap();
    assert_ne!(after, before, "the highlighted slot took the new color");

    // The same through a click on a row of the picker.
    app.theme_editor_mouse(click(1, 2));
    assert!(app.x11_panel.is_some());
    app.x11_mouse(click(1, 3));
    assert!(app.x11_panel.is_none());
    app.close_theme_editor();
    assert!(app.theme_editor.is_none());
}

#[test]
fn a_stray_x11_pick_inserts_the_hex_when_no_theme_editor_asked() {
    let mut app = fresh("pp3-x11");
    app.open_x11_panel();
    app.x11_key(code(KeyCode::Enter));
    let text = content(&mut app);
    assert!(text.starts_with('#') && text.len() == 7, "{text:?}");
    app.x11_key(code(KeyCode::Home));
    app.x11_key(code(KeyCode::End));
    app.x11_key(code(KeyCode::Esc));
    assert!(app.x11_panel.is_none());
}
