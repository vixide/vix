//! `vix-gui`: the Vix desktop GUI.
//!
//! Phase 1 of the GUI plan (see `gui/spec/index.md`): the *whole* Vix `App` —
//! every panel, overlay, picker and dock — running in a native, GPU-rendered
//! window on macOS, Windows and Linux. The App's unchanged `ui::draw` renders
//! into an in-memory cell grid (`grid`), which the window paints with a
//! monospace font (`paint`); keys, mouse and the native menu bar (`menus`)
//! feed straight back into the same `App::on_key` / `App::on_mouse` /
//! `App::run_action` the terminal front end uses.

#[macro_use]
extern crate vix_i18n;
vix_i18n::surface!();

mod grid;
mod keys;
mod menus;
mod paint;
mod view;

use std::path::PathBuf;

use clap::Parser;
use gpui_kit::{AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};
use vix::app::App as VixApp;
use vix::cli::Cli;
use vix::settings::Settings;

use crate::view::VixView;

fn main() {
    let cli = Cli::parse();

    // Same startup as the terminal binary (`src/main.rs`): the real system
    // clipboard, settings (reporting a load failure once the App exists),
    // and the locale (`--locale` wins, unsaved).
    vix::clipboard::use_system();
    let (settings, settings_load_error) = match Settings::try_load() {
        Ok(s) => (s, None),
        Err(e) => (Settings::default(), Some(e.to_string())),
    };
    let locale = cli
        .locale
        .clone()
        .unwrap_or_else(|| settings.locale.clone());
    rust_i18n::set_locale(&locale);

    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);

        // `theme = "auto"` follows the OS appearance — the window's
        // equivalent of the terminal binary's OSC 11 background query.
        if settings
            .theme
            .eq_ignore_ascii_case(vix::theme_model::background::AUTO)
        {
            use vix::theme_model::background::Background;
            let dark = matches!(
                cx.window_appearance(),
                gpui_kit::WindowAppearance::Dark | gpui_kit::WindowAppearance::VibrantDark
            );
            vix::theme_model::background::set_detected(Some(if dark {
                Background::Dark
            } else {
                Background::Light
            }));
        }

        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let title = format!("Vix — {}", root.display());
        let mut app = VixApp::new(root, settings);
        if let Some(e) = &settings_load_error {
            app.warn_settings_load_failed(e);
        }
        if let Err(e) = app.start(&cli) {
            eprintln!("vix-gui: {e}");
            cx.quit();
            return;
        }

        // Built after `App::new`, which registers the theme names and the
        // build's action filter the menu model reads.
        cx.set_menus(menus::native_menus());

        let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some(title.into()),
                ..TitlebarOptions::default()
            }),
            ..WindowOptions::default()
        };
        let opened = gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| VixView::new(app, window, cx))
        });
        match opened {
            Ok((_, view)) => {
                // Same deferral as the terminal binary (T122): the git
                // subprocesses run after the window is up, not before.
                view.update(cx, VixView::refresh_git);
            }
            Err(e) => {
                eprintln!("vix-gui: could not open a window: {e}");
                cx.quit();
            }
        }
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
}
