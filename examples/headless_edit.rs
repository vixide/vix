//! Drive Vix's editing logic without a terminal.
//!
//! This opens a temp file, types into it, runs a regex find-and-replace via the
//! same key events the TUI would receive, saves, and prints the result — showing
//! that the whole editing pipeline is usable as a plain library.
//!
//! Run with: `cargo run --example headless_edit`

#![warn(clippy::pedantic)]

use std::fs;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use vix::app::App;
use vix::settings::Settings;

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

fn main() -> std::io::Result<()> {
    let dir = std::env::temp_dir().join("vix-example");
    fs::create_dir_all(&dir)?;
    let file = dir.join("greeting.txt");
    fs::write(&file, "hello world\n")?;

    // Root the app at the temp dir and open the file. A non-zero editor area
    // gives the code editor a viewport to scroll the cursor within.
    let mut app = App::new(dir.clone(), Settings::default());
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app.open_initial(file.clone());
    println!("opened : {:?}", app.editor.active_tab().unwrap().lines());

    // Move to end of line and append "!!!".
    app.on_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
    for c in "!!!".chars() {
        app.on_key(key(c));
    }

    // Regex replace: capitalize-ish swap "hello" -> "HELLO".
    app.run_action("edit.replace");
    for c in "hello".chars() {
        app.on_key(key(c));
    }
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)); // to Replace field
    for c in "HELLO".chars() {
        app.on_key(key(c));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)); // replace all

    println!("edited : {:?}", app.editor.active_tab().unwrap().lines());

    // Save and read back from disk.
    app.run_action("file.save");
    let on_disk = fs::read_to_string(&file)?;
    println!("on disk: {on_disk:?}");

    fs::remove_dir_all(&dir).ok();
    Ok(())
}
