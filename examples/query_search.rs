//! Search a directory of files for a pattern, the same engine Workspace
//! Search's "to dock" mode uses (`Ctrl+P` -> `search.workspace_dock`).
//!
//! Improvement plan T503 (`tasks.md`) names this example `query_search
//! (vix-query over a directory)` -- but the crate that name refers to
//! doesn't do that: T151 folded the *original* `vix-query` crate into
//! `src/app.rs`, and it turns out to have never been about searching a
//! directory at all -- it was `Decision`, the y/n/!/q confirm-each-match
//! choice for interactive query-*replace* (see the doc comment on
//! `vix::app::Decision`). The actual "search a directory of files" engine is
//! `crates/vix-workspace-search` plus `App::search_workspace_to_dock`, which
//! is what this example drives instead.
//!
//! Run with: `cargo run --example query_search -- <dir> <pattern>`

#![warn(clippy::pedantic)]

use std::path::PathBuf;

use vix::app::App;
use vix::settings::Settings;

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(dir), Some(pattern)) = (args.next(), args.next()) else {
        eprintln!("usage: query_search <dir> <pattern>");
        std::process::exit(2);
    };

    let mut app = App::new(PathBuf::from(dir), Settings::default());

    // The same prompt Ctrl+P -> "search.workspace_dock" opens, then typing
    // the pattern and pressing Enter -- driven through the real `on_key`
    // dispatch, not a private shortcut.
    app.run_action("search.workspace_dock");
    for c in pattern.chars() {
        app.on_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(c),
            crossterm::event::KeyModifiers::NONE,
        ));
    }
    app.on_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    ));

    // Results (plus a "$ search ..." header line) land in the bottom dock,
    // the same place they'd show up on screen.
    for line in &app.bottom_dock.lines {
        println!("{line}");
    }
}
