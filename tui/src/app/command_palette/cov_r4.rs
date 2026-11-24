//! Round 4: the palette's remaining reachable branches -- a go-to-line
//! preview that must not move the cursor, a guest's `cowork:` file list, the
//! workspace-symbol scan skipping unreadable/oversized/image files and
//! stopping at its result cap, and the preview target of entries that have
//! no preview. Every app keeps its settings inside its own temp directory.

use super::*;
use crate::app::cov4::{app_at as base_app_at, code, key, temp};
use std::fmt::Write as _;
use vix_input::KeyCode;

fn app_at(d: &Path) -> App {
    let mut app = base_app_at(d);
    app.settings_path = Some(d.join("vix-test-config.toml"));
    app
}

#[test]
fn go_to_line_preview_ignores_non_numbers_zero_and_other_modes() {
    let d = temp("pal4-goto");
    let mut app = app_at(&d);
    app.editor.new_tab_with_content("a\nb\nc\nd\n");
    app.editor.active_tab_mut().unwrap().editor.set_cursor(2);
    let start = app.editor.cursor_line();
    app.open_palette_seeded(":");
    for typed in ["x", "0"] {
        app.palette_key(key(typed.chars().next().unwrap()));
        assert_eq!(
            app.editor.cursor_line(),
            start,
            "{typed:?} is not a line to preview"
        );
        app.palette_key(code(KeyCode::Backspace));
    }
    // A mode other than go-to-line never previews.
    app.open_palette_seeded("2");
    app.preview_goto_line();
    assert_eq!(app.editor.cursor_line(), start);
    // With no palette open there is nothing to preview either.
    app.palette = None;
    app.preview_goto_line();
    assert_eq!(app.editor.cursor_line(), start);
}

#[test]
fn a_guests_remote_file_is_shown_without_the_prefix_and_never_opened_as_a_local_file() {
    let d = temp("pal4-remote");
    let mut app = app_at(&d);
    let remote = PathBuf::from(format!("{}src/guest.rs", crate::editor::REMOTE_PATH_PREFIX));
    app.open_palette_seeded("");
    app.palette_files_override = Some(vec![remote]);
    app.recompute_palette();
    let entries = &app.palette.as_ref().unwrap().entries;
    assert_eq!(entries.len(), 1);
    assert!(
        entries[0].label.contains("src/guest.rs") && !entries[0].label.contains("cowork:"),
        "{}",
        entries[0].label
    );
    let tabs = app.editor.tabs.len();
    app.palette_key(code(KeyCode::Enter));
    assert!(app.palette.is_none(), "accepting closes the palette");
    assert!(app.palette_files_override.is_none());
    assert_eq!(
        app.editor.tabs.len(),
        tabs,
        "no local tab for a remote path"
    );
}

#[test]
fn the_workspace_symbol_scan_skips_images_big_files_and_non_text_and_stops_at_its_cap() {
    let d = temp("pal4-wsyms");
    std::fs::write(d.join("small.rs"), "fn needle_fn() {}\n").unwrap();
    std::fs::write(d.join("pic.png"), "fn needle_png() {}\n").unwrap();
    // Over the 512 KiB scan limit: skipped even though it holds a match.
    let mut big = String::from("fn needle_big() {}\n");
    big.push_str(&"// padding\n".repeat(60_000));
    std::fs::write(d.join("big.rs"), big).unwrap();
    // Not valid UTF-8: unreadable as text, skipped.
    std::fs::write(d.join("binary.rs"), [0xff_u8, 0xfe, 0x00, 0x66, 0x6e]).unwrap();
    // Many matches in one file: the result list is capped at 200.
    let mut many = String::new();
    for n in 0..260 {
        writeln!(many, "fn needle_{n}() {{}}").unwrap();
    }
    std::fs::write(d.join("many.rs"), many).unwrap();
    let mut app = app_at(&d);
    app.build_file_index();

    let entries = app.workspace_symbol_entries("needle");
    assert_eq!(entries.len(), 200, "capped");
    assert!(entries.iter().all(|e| !e.label.contains("needle_png")));
    assert!(entries.iter().all(|e| !e.label.contains("needle_big")));
    assert!(entries.iter().any(|e| e.label.contains("many.rs")));

    // Without the many-match file, the readable small file is found alone.
    std::fs::remove_file(d.join("many.rs")).unwrap();
    app.build_file_index();
    let entries = app.workspace_symbol_entries("needle");
    assert_eq!(
        entries.len(),
        1,
        "{:?}",
        entries.iter().map(|e| &e.label).collect::<Vec<_>>()
    );
    assert!(entries[0].label.contains("small.rs:1"));
}

#[test]
fn command_and_buffer_entries_have_no_preview_target() {
    let d = temp("pal4-target");
    let mut app = app_at(&d);
    app.open_palette_seeded(">");
    assert!(
        app.palette_preview_target().is_none(),
        "commands have no preview"
    );
    assert!(app.palette_preview().is_none());
    app.open_palette_seeded("#");
    assert!(
        app.palette_preview_target().is_none(),
        "buffers have no preview"
    );
    // The history step does nothing without a palette.
    app.palette = None;
    assert!(!app.palette_history_step(true));
    assert!(!app.palette_history_step(false));
}
