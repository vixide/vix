//! Sort, dedupe, and case-convert a file from the CLI -- the same
//! whole-buffer editor operations Edit -> Lines and Tools -> Convert -> Case
//! run interactively, chained as a pipeline over a real file (improvement
//! plan T502). Prints the result to stdout; the source file is untouched.
//!
//! Run with: `cargo run --example textops_pipeline -- <file>`

#![warn(clippy::pedantic)]

use std::path::PathBuf;

use vix::app::App;
use vix::settings::Settings;

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: textops_pipeline <file>");
        std::process::exit(2);
    };

    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut app = App::new(root, Settings::default());
    app.open_initial(&PathBuf::from(&path));

    let tab = app.editor.active_tab_mut().expect("file opened");

    // These two are real `Editor` methods (`crates/vix-editor-core/src/
    // lines.rs`) -- with no selection active, both act on the whole buffer.
    tab.editor.remove_duplicate_lines();
    tab.editor.sort_lines();

    // `vix_case` (re-exported as `vix::case`) is the pure-function half of
    // Tools -> Convert -> Case: it just transforms a `&str`, no editor
    // needed, so it slots into the end of this pipeline as plain text.
    let sorted_and_deduped = tab.text();
    let upper = vix::case::upper(&sorted_and_deduped);

    print!("{upper}");
}
