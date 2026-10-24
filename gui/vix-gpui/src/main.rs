//! `vix-gpui`: the Vix desktop GUI on GPUI (GPUI Kit).
//!
//! The *whole* Vix `App` — every panel, overlay, picker and dock — in a
//! native, GPU-rendered window on macOS, Windows and Linux. The App's
//! unchanged `ui::draw` renders into the shared in-memory cell grid
//! (`vix_gui_common::grid`), which the window paints with a monospace font
//! (`paint`); keys, mouse and the native menu bar (`menus`) feed straight back
//! into the same `App::on_key` / `App::on_mouse` / `App::run_action` the
//! terminal front end uses. See `gui/spec/index.md`.

mod keys;
mod menus;
mod paint;
mod view;

use clap::Parser;
use gpui_kit::{AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};
use vix::cli::Cli;
use vix_gui_common::startup;

use crate::view::VixView;

fn main() {
    let cli = Cli::parse();
    let loaded = startup::load(&cli);

    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        let dark = matches!(
            cx.window_appearance(),
            gpui_kit::WindowAppearance::Dark | gpui_kit::WindowAppearance::VibrantDark
        );
        let (app, title) = match startup::start(&cli, loaded, dark) {
            Ok(started) => started,
            Err(e) => {
                eprintln!("vix-gpui: {e}");
                cx.quit();
                return;
            }
        };

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
            // Same deferral as the terminal binary (T122): the git
            // subprocesses run after the window is up, not before.
            Ok((_, view)) => view.update(cx, VixView::refresh_git),
            Err(e) => {
                eprintln!("vix-gpui: could not open a window: {e}");
                cx.quit();
            }
        }
        cx.activate(true);
    });
}
