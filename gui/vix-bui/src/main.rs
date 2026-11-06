//! `vix-bui`: the Vix desktop GUI on BUI (Blender's UI as a library).
//!
//! The *whole* Vix `App` — every panel, overlay, picker and dock — in a native
//! window, with all of its own GUI code (no library shared with the other
//! front ends): the App's unchanged `ui::draw` renders into an in-memory cell
//! grid, which this front end paints with BUI's `canvas` (`view`, `scene`);
//! keystrokes, the mouse and (on macOS) the native menu bar feed back into the
//! same `App::on_key` / `App::on_mouse` / `App::run_action` the terminal uses
//! (`keys`, `model`, `menus`). See
//! `gui/spec/index.md` and `gui/vix-bui/spec/index.md`.

// A release build on Windows is a GUI app: no console window opens beside it.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

// `t!` is used only by `menu_tree`.
#[cfg_attr(any(target_os = "macos", test), macro_use)]
extern crate vix_i18n;
vix_i18n::surface!();

mod font_choice;
mod fonts;
mod glyphs;
mod grid;
mod key_rules;
mod keys;
// Only the macOS native menu bar reads the menu tree (its tests run everywhere).
#[cfg(any(target_os = "macos", test))]
mod menu_tree;
#[cfg(target_os = "macos")]
mod menus;
mod model;
mod render;
mod scene;
#[cfg(feature = "screenshot")]
mod screenshot;
mod startup;
mod view;

#[cfg(test)]
mod headless;

use clap::Parser;
use vix::cli::Cli;

use crate::model::Model;

/// Whether the window follows BUI's dark theme: the `BUI_THEME` variable
/// (BUI's own switch) set to `light` selects the light one.
fn dark_appearance() -> bool {
    !std::env::var("BUI_THEME").is_ok_and(|v| v.eq_ignore_ascii_case("light"))
}

/// Parse the command line, load settings and the initial files through
/// `startup::load`, then run the BUI window with the `App` inside `Model`.
///
/// The working directory becomes the workspace root (falling back to `.`).
fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let loaded = startup::load(&cli);
    let dark = dark_appearance();
    let (app, title) = match startup::start(&cli, loaded, dark) {
        Ok(started) => started,
        Err(e) => {
            eprintln!("vix-bui: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let model = Model::new(app, dark, fonts::Fonts::load());

    #[cfg(feature = "screenshot")]
    if let Some(script) = startup::Script::from_env() {
        return match screenshot::run(model, &script) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("vix-bui: could not save {}: {e}", script.path);
                std::process::ExitCode::FAILURE
            }
        };
    }

    view::run(model, title, dark);
    std::process::ExitCode::SUCCESS
}
