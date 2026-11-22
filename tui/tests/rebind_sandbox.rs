//! The keybinding editor's rebind and reset writes, in a sandbox.
//!
//! `accept_rebind_key` and `reset_selected_keybinding` persist through
//! `Settings::keybindings_path()`, the real user config directory. So this
//! test re-executes its own binary with a throwaway `HOME`,
//! `XDG_CONFIG_HOME` and `XDG_DATA_HOME` (the pattern of `action_sweep.rs`)
//! and, in the child, refuses to run unless the path really is inside that
//! sandbox.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests fail loudly on a broken fixture"
)]

use std::path::Path;
use std::process::Command;

use vix::app::App;
use vix::settings::Settings;
use vix_input::{KeyCode, KeyEvent, KeyModifiers};

/// Marks the sandboxed child.
const SANDBOX_VAR: &str = "VIX_REBIND_SANDBOX";

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

fn code(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Type `text` into the open prompt and accept it.
fn answer(app: &mut App, text: &str) {
    for c in text.chars() {
        app.on_key(key(c));
    }
    app.on_key(code(KeyCode::Enter));
}

fn has_message(app: &App, needle: &str) -> bool {
    app.messages.items.iter().any(|m| m.text.contains(needle))
}

fn body() {
    let home = std::env::var_os("HOME").expect("sandbox HOME");
    let path = Settings::keybindings_path().expect("a config path");
    let config = std::env::var_os("XDG_CONFIG_HOME").expect("sandbox XDG_CONFIG_HOME");
    assert!(
        path.starts_with(&home) || path.starts_with(&config),
        "{} is outside the sandbox {}",
        path.display(),
        Path::new(&home).display()
    );
    let root = Path::new(&home).join("work");
    std::fs::create_dir_all(&root).unwrap();

    let mut app = App::new(root, Settings::default());
    app.run_action("keybindings.editor");
    // Narrow the list to one row, then rebind it.
    for c in "palette".chars() {
        app.on_key(key(c));
    }
    app.on_key(code(KeyCode::Enter));
    answer(&mut app, "C-g");
    assert!(
        path.exists(),
        "a rebind writes {} under the sandbox; messages: {:?}",
        path.display(),
        app.messages
            .items
            .iter()
            .map(|m| m.text.clone())
            .collect::<Vec<_>>()
    );
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("tools.palette"), "{text}");
    // Delete on a built-in row does nothing; the override stays.
    app.on_key(key('x'));
    app.on_key(code(KeyCode::Delete));
    assert!(std::fs::read_to_string(&path).unwrap().contains("C-g"));
    // Filter to the override and reset it: it leaves the file.
    for _ in 0..12 {
        app.on_key(code(KeyCode::Backspace));
    }
    for c in "ctrl g".chars() {
        app.on_key(key(c));
    }
    app.on_key(code(KeyCode::Delete));
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("tools.palette"), "reset removed it: {text}");
    assert!(has_message(&app, "C-g"), "the rebind is reported");
}

#[test]
fn rebind_and_reset_persist_in_the_sandbox_config() {
    if std::env::var_os(SANDBOX_VAR).is_some() {
        body();
        return;
    }
    let base = std::env::temp_dir().join(format!("vix-rebind-sandbox-{}", std::process::id()));
    let home = base.join("home");
    let config = base.join("config");
    for dir in [&home, &config] {
        std::fs::create_dir_all(dir).unwrap();
    }
    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "rebind_and_reset_persist_in_the_sandbox_config",
            "--nocapture",
        ])
        .current_dir(&home)
        .env(SANDBOX_VAR, "1")
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &config)
        .env("XDG_DATA_HOME", home.join("data"))
        .env("APPDATA", &config)
        .status()
        .unwrap();
    let _ = std::fs::remove_dir_all(&base);
    assert!(status.success(), "the sandboxed rebind test failed");
}
