//! Parse a `macros.toml` file and replay one of its saved macros onto a
//! fresh scratch buffer -- the same `vix_macros::load`/`decode` + `on_key`
//! path the real macro player uses (improvement plan T502).
//!
//! Run with: `cargo run --example macro_replay`

#![warn(clippy::pedantic)]

use std::path::PathBuf;

use vix::app::App;
use vix::settings::Settings;

fn main() {
    // A real `macros.toml`: one macro that types "Hi, " then a name typed
    // separately below, matching the `[[macro]]` schema
    // `crates/vix-macros/spec/index.md` documents.
    let toml = r#"
[[macro]]
name = "greet"
keys = ["H", "i", ",", "Space"]
"#;
    let dir = std::env::temp_dir().join(format!("vix-macro-replay-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    let macros_path = dir.join("macros.toml");
    std::fs::write(&macros_path, toml).expect("write macros.toml");

    // Parse it back with the real loader.
    let macros = vix::macros::load(&macros_path);
    let greet = macros
        .iter()
        .find(|m| m.name == "greet")
        .expect("the macro we just wrote");
    println!("loaded macro {:?}: {:?}", greet.name, greet.keys);

    // Decode its tokens into real `KeyEvent`s and replay them through a real
    // `App`, the exact same call `App::on_key` makes for every live
    // keystroke -- the macro player is not a separate code path.
    let mut app = App::new(
        std::env::current_dir().unwrap_or(PathBuf::from(".")),
        Settings::default(),
    );
    app.open_stdin_buffer("");
    for key in vix::macros::decode(&greet.keys) {
        app.on_key(key);
    }
    // Type the rest by hand, same as a user would after a macro sets up a
    // common prefix.
    for c in "World".chars() {
        app.on_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(c),
            crossterm::event::KeyModifiers::NONE,
        ));
    }

    let result = app.editor.active_tab().expect("buffer open").text();
    println!("replayed buffer: {result:?}");
    assert_eq!(result, "Hi, World");
}
