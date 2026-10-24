//! `vix-gtk4`: the Vix desktop GUI on GTK 4 (gtk4-rs).
//!
//! The *whole* Vix `App` — every panel, overlay, picker and dock — in a native
//! GTK window on Linux, macOS and Windows, with all of its own GUI code (no
//! library shared with the other front ends): the App's unchanged `ui::draw`
//! renders into an in-memory cell grid, which this front end paints with
//! Cairo and Pango on a `DrawingArea` (`view`, `paint`); keystrokes,
//! input-method commits, the mouse and the native GIO menu bar (`menus`) feed
//! back into the same `App::on_key` / `App::on_mouse` / `App::run_action` the
//! terminal uses. See `gui/spec/index.md`.

// A release build on Windows is a GUI app: no console window opens beside it.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[macro_use]
extern crate vix_i18n;
vix_i18n::surface!();

mod appearance;
mod font_choice;
mod grid;
mod key_rules;
mod keys;
mod menu_tree;
mod menus;
mod paint;
mod render;
#[cfg(feature = "screenshot")]
mod screenshot;
mod startup;
mod view;

use std::cell::RefCell;
use std::rc::Rc;

use clap::Parser;
use gtk4::prelude::*;
use gtk4::{gio, glib};
use vix::cli::Cli;

use crate::view::View;

/// The GTK application id.
const APP_ID: &str = "io.github.vixide.Vix";

fn main() -> glib::ExitCode {
    // Vix's own command line, parsed before GTK sees anything; GTK is then
    // given only argv[0], so it never tries to open the file arguments.
    let cli = Cli::parse();
    let loaded = startup::load(&cli);

    // NON_UNIQUE: each `vix-gtk4` is its own process and window, so two
    // windows on different folders never merge into one instance.
    let gtk_app = gtk4::Application::new(Some(APP_ID), gio::ApplicationFlags::NON_UNIQUE);
    let pending = RefCell::new(Some((cli, loaded)));
    let window: Rc<RefCell<Option<View>>> = Rc::default();

    let slot = Rc::clone(&window);
    gtk_app.connect_activate(move |gtk_app| {
        let Some((cli, loaded)) = pending.borrow_mut().take() else {
            // Activated again (the macOS Dock): show the one window.
            if let Some(v) = slot.borrow().as_ref() {
                v.present();
            }
            return;
        };
        let dark = appearance::dark();
        let (app, title) = match startup::start(&cli, loaded, dark) {
            Ok(started) => started,
            Err(e) => {
                eprintln!("vix-gtk4: {e}");
                gtk_app.quit();
                return;
            }
        };
        // Built after `App::new`, which registers the theme names and the
        // build's action filter the menu model reads.
        gtk_app.set_menubar(Some(&menus::menubar()));
        let view = View::new(gtk_app, app, dark, &title);
        // Same deferral as the terminal binary (T122): the git subprocesses
        // run once the window is up.
        view.state().borrow_mut().refresh_git();
        *slot.borrow_mut() = Some(view);
    });
    // However the application ends, the App saves its session once.
    gtk_app.connect_shutdown(move |_| {
        if let Some(v) = window.borrow().as_ref() {
            v.state().borrow_mut().exit();
        }
    });

    let argv0 = std::env::args().next().unwrap_or_default();
    gtk_app.run_with_args(&[argv0])
}
