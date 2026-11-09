//! `vix-zgui`: the Vix desktop GUI on zgui.
//!
//! The *whole* Vix `App` — every panel, overlay, picker and dock — in a native
//! window on macOS, Windows and Linux (Wayland and X11), with all of its own
//! GUI code (no library shared with the other front ends): the App's
//! unchanged `ui::draw` renders into an in-memory cell grid, which one zgui
//! custom element paints as quads and shaped glyph runs (`paint`);
//! keystrokes, IME commits, the mouse and (on macOS) the native menu bar feed
//! back into the same `App::on_key` / `App::on_mouse` / `App::run_action` the
//! terminal uses (`keys`, `model`, `view`, `menus`). See `gui/spec/index.md`
//! and `gui/vix-zgui/spec/index.md`.

// A release build on Windows is a GUI app: no console window opens beside it.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

// `t!` is used only by `menu_tree`.
#[cfg_attr(any(target_os = "macos", test), macro_use)]
extern crate vix_i18n;
vix_i18n::surface!();

mod font_choice;
mod grid;
mod key_rules;
mod keys;
// Only the macOS native menu bar reads the menu tree (its tests run everywhere).
#[cfg(any(target_os = "macos", test))]
mod menu_tree;
#[cfg(target_os = "macos")]
mod menus;
mod model;
mod paint;
mod render;
mod startup;
mod view;

#[cfg(test)]
mod headless;

use clap::Parser;
use vix::cli::Cli;

use crate::model::Model;

/// Whether the window starts dark: `VIX_GUI_THEME=light` selects light.
/// zgui 0.1 reports the desktop's preference only once a window is open,
/// after the App (and its theme) must already exist.
fn dark_appearance() -> bool {
    !std::env::var("VIX_GUI_THEME").is_ok_and(|v| v.eq_ignore_ascii_case("light"))
}

/// Parse the command line, load settings and the initial files through
/// `startup::load`, then run the zgui window with the `App` inside `Model`.
///
/// The working directory becomes the workspace root (falling back to `.`).
fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    // Before anything opens or loads: `--version` prints and exits, as in the
    // terminal binary (the Homebrew formula's test runs it).
    if cli.version {
        println!("vix-zgui {}", env!("CARGO_PKG_VERSION"));
        return std::process::ExitCode::SUCCESS;
    }
    let loaded = startup::load(&cli);
    let dark = dark_appearance();
    let (app, title) = match startup::start(&cli, loaded, dark) {
        Ok(started) => started,
        Err(e) => {
            eprintln!("vix-zgui: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    match view::run(Model::new(app, dark), title) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("vix-zgui: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
