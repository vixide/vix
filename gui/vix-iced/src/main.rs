//! `vix-iced`: the Vix desktop GUI on iced.
//!
//! The *whole* Vix `App` — every panel, overlay, picker and dock — in a native
//! window on macOS, Windows and Linux, with all of its own GUI code (no
//! library shared with the other front ends): the App's unchanged `ui::draw`
//! renders into an in-memory cell grid, which this front end paints on an
//! iced `canvas` (`paint`); keystrokes, IME commits, the mouse and (on macOS)
//! the native menu bar feed back into the same `App::on_key` /
//! `App::on_mouse` / `App::run_action` the terminal uses. See
//! `gui/spec/index.md`.

// A release build on Windows is a GUI app: no console window opens beside it.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

// `t!` is used only by `menu_tree`.
#[cfg_attr(any(target_os = "macos", test), macro_use)]
extern crate vix_i18n;
vix_i18n::surface!();

mod font_choice;
mod fonts;
mod grid;
mod ime;
mod key_rules;
mod keys;
// Only the macOS native menu bar reads the menu tree (its tests run everywhere).
#[cfg(any(target_os = "macos", test))]
mod menu_tree;
#[cfg(target_os = "macos")]
mod menus;
mod paint;
mod render;
#[cfg(feature = "screenshot")]
mod screenshot;
mod startup;
mod view;

use std::cell::Cell;
use std::path::PathBuf;

use clap::Parser;
use vix::cli::Cli;

use crate::view::VixIced;

fn main() -> iced::Result {
    let cli = Cli::parse();
    let loaded = startup::load(&cli);
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let title = startup::window_title(&root);
    // iced's boot function is `Fn`, but the command line and settings are
    // used once: hand them over through a cell the first call empties (a
    // second call, which iced never makes, would read them afresh).
    let pending = Cell::new(Some((cli, loaded)));
    iced::application(
        move || {
            let (cli, loaded) = pending.take().unwrap_or_else(|| {
                let cli = Cli::parse();
                let loaded = startup::load(&cli);
                (cli, loaded)
            });
            VixIced::boot(cli, loaded, title.clone())
        },
        VixIced::update,
        VixIced::view,
    )
    .title(VixIced::title)
    .theme(VixIced::theme)
    .subscription(VixIced::subscription)
    .window_size((1200.0, 800.0))
    // The close button runs Vix's own quit flow instead (see `view`).
    .exit_on_close_request(false)
    .run()
}
