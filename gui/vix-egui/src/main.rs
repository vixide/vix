//! `vix-egui`: the Vix desktop GUI on egui (eframe).
//!
//! The *whole* Vix `App` — every panel, overlay, picker and dock — in a native
//! window on macOS, Windows and Linux, over the same toolkit-agnostic core as
//! `vix-gpui` (`vix_gui_common`): the App's unchanged `ui::draw` renders into
//! the shared cell grid, which this front end paints with egui (`view`);
//! keystrokes, IME commits, the mouse and (on macOS) the native menu bar feed
//! back into the same `App::on_key` / `App::on_mouse` / `App::run_action` the
//! terminal uses. See `gui/spec/index.md`.

// A release build on Windows is a GUI app: no console window opens beside it.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod fonts;
mod keys;
#[cfg(target_os = "macos")]
mod menus;
#[cfg(feature = "screenshot")]
mod screenshot;
mod view;

use std::path::PathBuf;

use clap::Parser;
use eframe::egui::{Theme, ViewportBuilder};
use vix::cli::Cli;
use vix_gui_common::startup;

use crate::view::VixEgui;

fn main() -> eframe::Result {
    let cli = Cli::parse();
    let loaded = startup::load(&cli);
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title(startup::window_title(&root)),
        // Vix binds Ctrl/Cmd+X/C/V itself (and keeps its own clipboard
        // history), so they must arrive as plain key presses, not as egui's
        // Cut/Copy/Paste events.
        clipboard_shortcuts: false,
        ..eframe::NativeOptions::default()
    };
    eframe::run_native(
        "Vix",
        options,
        Box::new(move |cc| {
            let dark = cc.egui_ctx.system_theme().unwrap_or(Theme::Dark) == Theme::Dark;
            let (app, _) = startup::start(&cli, loaded, dark)?;
            let mut window = VixEgui::new(cc, app, dark);
            // Same deferral as the terminal binary (T122): the git
            // subprocesses run once the window exists.
            window.refresh_git();
            Ok(Box::new(window))
        }),
    )
}
