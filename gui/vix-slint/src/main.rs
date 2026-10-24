//! `vix-slint`: the Vix desktop GUI on Slint.
//!
//! The *whole* Vix `App` — every panel, overlay, picker and dock — in a native
//! window on macOS, Windows and Linux, with all of its own GUI code (no
//! library shared with the other front ends): the App's unchanged `ui::draw`
//! renders into an in-memory cell grid, whose rows become the models of a
//! Slint window (`paint`, `ui/window.slint`) compiled at startup by Slint's
//! interpreter; keystrokes, the mouse and the native menu bar feed back into
//! the same `App::on_key` / `App::on_mouse` / `App::run_action` the terminal
//! uses (`view`). See `gui/spec/index.md`.

// A release build on Windows is a GUI app: no console window opens beside it.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[macro_use]
extern crate vix_i18n;
vix_i18n::surface!();

mod font_choice;
mod fonts;
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
mod window;

use std::cell::RefCell;
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::Duration;

use clap::Parser;
use slint::ComponentHandle;
use slint_interpreter::Value;
use vix::cli::Cli;

use crate::view::State;

/// The running window's state and background timer, once the App started.
type Started = Rc<RefCell<Option<(Rc<RefCell<State>>, Rc<slint::Timer>)>>>;

fn main() -> ExitCode {
    let cli = Cli::parse();
    // Settings and the locale first: the menu bar is translated as it is built.
    let loaded = startup::load(&cli);
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // The winit backend: Option-as-Meta needs its raw key events.
    let _ = slint::BackendSelector::new()
        .backend_name("winit".into())
        .select();
    #[cfg(feature = "screenshot")]
    let at = std::time::Instant::now();
    let ui = match window::create(&startup::window_title(&root)) {
        Ok(ui) => {
            #[cfg(feature = "screenshot")]
            eprintln!(
                "vix-slint: compiling the window and its menu bar took {:?}",
                at.elapsed()
            );
            ui
        }
        Err(e) => {
            eprintln!("vix-slint: {e}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = ui.show() {
        eprintln!("vix-slint: {e}");
        return ExitCode::FAILURE;
    }

    // Start the App once the event loop runs: only then is the window up and
    // the OS appearance known, which `theme = "auto"` and the default colors
    // follow.
    let started: Started = Rc::default();
    let failed = Rc::new(RefCell::new(None));
    {
        let started = Rc::clone(&started);
        let failed = Rc::clone(&failed);
        let weak = ui.as_weak();
        let mut once = Some((cli, loaded));
        slint::Timer::single_shot(Duration::ZERO, move || {
            let (Some(ui), Some((cli, loaded))) = (weak.upgrade(), once.take()) else {
                return;
            };
            let dark = !matches!(ui.get_property("dark"), Ok(Value::Bool(false)));
            match startup::start(&cli, loaded, dark) {
                Ok((app, _)) => {
                    let state = Rc::new(RefCell::new(State::new(app, dark)));
                    if let Ok(s) = state.try_borrow() {
                        let _ = ui.set_property("family", Value::String(s.family().into()));
                        let _ = ui.set_property("grid-background", s.background());
                        let _ = ui.set_property("rows", s.rows());
                    }
                    let timer = view::attach(&ui, &state);
                    // Same deferral as the terminal binary (T122): the git
                    // subprocesses run once the window exists.
                    if let Ok(mut s) = state.try_borrow_mut() {
                        s.refresh_git();
                    }
                    *started.borrow_mut() = Some((state, timer));
                }
                Err(e) => {
                    *failed.borrow_mut() = Some(e.to_string());
                    let _ = slint::quit_event_loop();
                }
            }
        });
    }

    if let Err(e) = slint::run_event_loop() {
        eprintln!("vix-slint: {e}");
    }
    let _ = ui.hide();
    if let Some((state, _timer)) = started.borrow_mut().take()
        && let Ok(mut s) = state.try_borrow_mut()
    {
        s.on_exit();
    }
    if let Some(e) = failed.borrow_mut().take() {
        eprintln!("vix-slint: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
