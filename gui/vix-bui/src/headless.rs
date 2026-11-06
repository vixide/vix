//! End-to-end tests through BUI's off-screen windows: real BUI events in, the
//! Vix `App`'s grid and BUI's rendered pixels out. They need no display; BUI
//! renders headless windows with its software raster.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use blenderui::prelude::Application;
use vix::app::App;
use vix::settings::Settings;

use crate::fonts::Fonts;
use crate::model::Model;
use crate::view::{self, Shared};

/// A fresh scratch directory for one test.
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vix-bui-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// A shared model over a new `App` rooted at `root`, with its session kept
/// out of the user's real one.
fn shared_at(root: &Path) -> Rc<Shared> {
    let app = App::new(root.to_path_buf(), Settings::default())
        .with_session_path(root.join("session.toml"));
    Shared::new(Model::new(app, true, Fonts::embedded()))
}

/// Whether any row of the grid contains `needle`.
fn grid_has(shared: &Shared, needle: &str) -> bool {
    shared
        .model
        .borrow()
        .grid_lines()
        .iter()
        .any(|l| l.contains(needle))
}

#[test]
fn window_draws_the_app_and_typing_reaches_the_buffer() {
    let dir = scratch("typing");
    let shared = shared_at(&dir);
    Application::headless().run(|cx| {
        let win = view::open(cx, &shared, "Vix".into(), 800.0, 400.0).expect("window");
        let (w, h, before) = cx.draw(win.id);
        assert!(w > 0 && h > 0, "the window rendered");
        // The grid was fitted to the 800x400 window: 100 x 22 cells.
        assert_eq!(shared.model.borrow().grid_size(), (100, 22));
        // The App painted something other than one flat color.
        assert!(before.iter().any(|&p| p != before[0]), "not a blank window");

        // BUI's kit routes a key to the focused node, or else to the one under
        // the pointer: put the pointer over the window first, as a user's is.
        cx.simulate_mouse_move(win.id, 400.0, 200.0);
        // Type a marker through BUI's own key path.
        for ch in "zqxv".chars() {
            let s = ch.to_string();
            cx.simulate_key(win.id, &s, &s);
        }
        cx.notify_all();
        let (_, _, after) = cx.draw(win.id);
        assert!(grid_has(&shared, "zqxv"), "typed text is in the grid");
        assert_ne!(before, after, "the pixels changed");
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn chord_opens_an_overlay_and_escape_closes_it() {
    let dir = scratch("chord");
    let shared = shared_at(&dir);
    Application::headless().run(|cx| {
        let win = view::open(cx, &shared, "Vix".into(), 900.0, 500.0).expect("window");
        cx.draw(win.id);
        cx.simulate_mouse_move(win.id, 400.0, 250.0);
        let before = shared.model.borrow().grid_lines();
        cx.simulate_key_with(win.id, "", "p", true, false); // Ctrl+P: the palette
        cx.notify_all();
        cx.draw(win.id);
        let open = shared.model.borrow().grid_lines();
        assert_ne!(before, open, "the command palette drew over the editor");
        cx.simulate_key(win.id, "", "Escape");
        cx.notify_all();
        cx.draw(win.id);
        assert_eq!(
            before,
            shared.model.borrow().grid_lines(),
            "Escape put the editor back"
        );
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn clicking_and_scrolling_do_not_disturb_the_window() {
    let dir = scratch("mouse");
    let shared = shared_at(&dir);
    Application::headless().run(|cx| {
        let win = view::open(cx, &shared, "Vix".into(), 800.0, 400.0).expect("window");
        cx.draw(win.id);
        cx.simulate_click(win.id, 200.0, 100.0);
        cx.simulate_scroll(win.id, 200.0, 100.0, 3);
        cx.simulate_scroll(win.id, 200.0, 100.0, -3);
        cx.notify_all();
        let (w, h, px) = cx.draw(win.id);
        assert!(w > 0 && h > 0 && !px.is_empty());
        assert!(!shared.model.borrow_mut().should_quit());
    });
    let _ = std::fs::remove_dir_all(&dir);
}

/// The real path with whatever monospace font this machine has: a real font,
/// a styled span and a fallback icon over the canvas must neither disturb the
/// grid's text nor stop input from reaching the window. With no font installed
/// (a bare CI image) this is the embedded path again, and still must hold.
#[test]
fn a_monospace_window_keeps_input_working_over_styled_text() {
    let dir = scratch("mono");
    let file = dir.join("note.txt");
    std::fs::write(&file, "# heading\n\u{f07b} icon\n").expect("file");
    let mut app =
        App::new(dir.clone(), Settings::default()).with_session_path(dir.join("session.toml"));
    app.open_initial(&file);
    let shared = Shared::new(Model::new(app, true, Fonts::load()));
    Application::headless().run(|cx| {
        let win = view::open(cx, &shared, "Vix".into(), 800.0, 400.0).expect("window");
        let (_, _, pixels) = cx.draw(win.id);
        assert!(pixels.iter().any(|&p| p != pixels[0]), "not a blank window");
        assert!(grid_has(&shared, "heading"));
        let m = shared.model.borrow().metrics();
        // Over the opened text, where a styled element may sit.
        cx.simulate_mouse_move(win.id, m.cell_w * 30.0, m.cell_h * 3.0);
        cx.simulate_key(win.id, "z", "z");
        cx.simulate_scroll(win.id, m.cell_w * 30.0, m.cell_h * 3.0, 1);
        cx.notify_all();
        cx.draw(win.id);
        assert!(grid_has(&shared, "z"), "the key reached the App");
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn quit_action_ends_the_loop_request() {
    let dir = scratch("quit");
    let shared = shared_at(&dir);
    Application::headless().run(|cx| {
        let win = view::open(cx, &shared, "Vix".into(), 600.0, 300.0).expect("window");
        cx.draw(win.id);
        shared.model.borrow_mut().run_action("file.quit");
        assert!(
            shared.model.borrow_mut().should_quit(),
            "nothing unsaved, so it quits"
        );
    });
    let _ = std::fs::remove_dir_all(&dir);
}
