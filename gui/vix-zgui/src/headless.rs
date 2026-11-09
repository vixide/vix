//! End-to-end tests of the window-independent half: zgui key values in
//! (through the same `keys::key_event` the window's listener calls), the Vix
//! `App`'s grid out. zgui 0.1 publishes no headless platform, so no window
//! opens here; `paint` and `view` are exercised by running the binary.

use std::path::{Path, PathBuf};

use vix::app::App;
use vix::settings::Settings;
use vix_input::{KeyModifiers, MouseButton};
use zgui::prelude::{Key, Modifiers, NamedKey};

use crate::keys;
use crate::model::{Metrics, Model};

/// A fresh scratch directory for one test.
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vix-zgui-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// A model over a new `App` rooted at `root`, with its session kept out of
/// the user's real one, fitted to 100 x 30 cells and rendered.
fn model_at(root: &Path) -> Model {
    let app = App::new(root.to_path_buf(), Settings::default())
        .with_session_path(root.join("session.toml"));
    let mut model = Model::new(app, true);
    let m = Metrics::placeholder();
    model.fit(m.cell_w * 100.0, m.cell_h * 30.0, m);
    model.render();
    model
}

/// Press `key` (with `unmodified` as the key without modifiers), then render.
fn press(model: &mut Model, key: &Key, unmodified: &Key, mods: Modifiers) {
    if let Some(event) = keys::key_event(key, unmodified, mods) {
        model.key(event);
    }
    model.render();
}

/// Type `text` one character at a time.
fn type_text(model: &mut Model, text: &str) {
    for c in text.chars() {
        let k = Key::character(c.to_string());
        press(model, &k, &k, Modifiers::NONE);
    }
}

/// Whether any row of the grid contains `needle`.
fn grid_has(model: &Model, needle: &str) -> bool {
    model.grid_lines().iter().any(|l| l.contains(needle))
}

#[test]
fn the_grid_fits_the_window_and_typing_reaches_the_buffer() {
    let dir = scratch("typing");
    let mut model = model_at(&dir);
    assert_eq!(model.grid_size(), (100, 30));
    let esc = Key::Named(NamedKey::Escape);
    // A fresh profile opens on the Welcome overlay.
    press(&mut model, &esc, &esc, Modifiers::NONE);
    type_text(&mut model, "zqxv");
    assert!(grid_has(&model, "zqxv"), "typed text is in the grid");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_chord_opens_an_overlay_and_escape_closes_it() {
    let dir = scratch("chord");
    let mut model = model_at(&dir);
    let esc = Key::Named(NamedKey::Escape);
    press(&mut model, &esc, &esc, Modifiers::NONE);
    let before = model.grid_lines();
    // Ctrl+P: the command palette. The layout's key is a control character.
    press(
        &mut model,
        &Key::character("\u{10}"),
        &Key::character("p"),
        Modifiers::CONTROL,
    );
    assert_ne!(
        before,
        model.grid_lines(),
        "the palette drew over the editor"
    );
    press(&mut model, &esc, &esc, Modifiers::NONE);
    assert_eq!(before, model.grid_lines(), "Escape put the editor back");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn clicking_and_scrolling_reach_the_app_without_quitting() {
    let dir = scratch("mouse");
    let mut model = model_at(&dir);
    let none = KeyModifiers::NONE;
    model.mouse_down(200.0, 100.0, MouseButton::Left, none);
    assert!(
        model.mouse_move(260.0, 100.0, none),
        "a new cell is reported"
    );
    assert!(
        !model.mouse_move(260.5, 100.0, none),
        "the same cell is not"
    );
    model.mouse_up(260.0, 100.0, MouseButton::Left, none);
    assert!(model.scroll_lines(200.0, 100.0, 3.0, none));
    assert!(
        !model.scroll_pixels(200.0, 100.0, 5.0, none),
        "under a line"
    );
    assert!(model.scroll_pixels(200.0, 100.0, -40.0, none));
    model.render();
    assert!(!model.should_quit());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cells_are_found_at_the_windows_scale() {
    let dir = scratch("scale");
    let mut model = model_at(&dir);
    let m = Metrics {
        scale: 2.0,
        cell_w: 16.0,
        cell_h: 36.0,
        baseline: 28.0,
    };
    model.fit(1600.0, 720.0, m);
    assert_eq!(model.grid_size(), (100, 20));
    // CSS pixels: 8 x 18 per cell at scale 2.
    assert_eq!(model.cell_at(8.5, 18.5), (1, 1));
    assert_eq!(model.cell_at(-4.0, 9999.0), (0, 19));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_input_method_commit_types_its_text() {
    let dir = scratch("ime");
    let mut model = model_at(&dir);
    let esc = Key::Named(NamedKey::Escape);
    press(&mut model, &esc, &esc, Modifiers::NONE);
    model.commit("日本\n");
    model.render();
    assert!(grid_has(&model, "日"), "the committed text is in the grid");
    let _ = std::fs::remove_dir_all(&dir);
}

/// What the system dialog does with its answer, without the dialog.
#[test]
fn files_from_the_system_dialog_open_in_tabs() {
    let dir = scratch("dialog");
    let file = dir.join("picked.txt");
    std::fs::write(&file, "picked-by-the-dialog\n").expect("file");
    let mut model = model_at(&dir);
    assert!(!grid_has(&model, "picked-by-the-dialog"));
    model.open_files(std::slice::from_ref(&file));
    model.render();
    assert!(grid_has(&model, "picked-by-the-dialog"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_quit_action_asks_the_window_to_close() {
    let dir = scratch("quit");
    let mut model = model_at(&dir);
    model.run_action("file.quit");
    assert!(model.should_quit(), "nothing unsaved, so it quits");
    let _ = std::fs::remove_dir_all(&dir);
}
