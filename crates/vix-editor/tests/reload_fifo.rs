//! T656: reloading a clean buffer whose path a checkout replaced with a FIFO
//! is refused by the open-time guard instead of blocking on the read. Lives
//! here rather than beside the other reload tests in `src/editor.rs` because it
//! runs `mkfifo`, and `tui/tests/spawn_sites.rs` keeps child processes out of
//! `src/`.
#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use std::fs;

use vix_editor::editor::{Editor, Flags};
use vix_fileops::TextReadError;

#[test]
fn reload_refuses_a_fifo_without_blocking() {
    let dir = std::env::temp_dir().join(format!("vix-editor-reload-fifo-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    let path = dir.join("a.txt");
    fs::write(&path, "text\n").unwrap();
    let mut e = Editor::new(Flags::LINE_NUMBERS, "    ".into());
    e.open(&path, false).unwrap();

    fs::remove_file(&path).unwrap();
    let made = std::process::Command::new("mkfifo")
        .arg(&path)
        .status()
        .is_ok_and(|s| s.success());
    if !made {
        eprintln!("mkfifo unavailable; skipping");
        fs::remove_dir_all(&dir).ok();
        return;
    }
    // Without the guard this read would block forever (no writer).
    let out = e.reload_clean_from_disk_checked();
    assert_eq!(out.reloaded, 0);
    assert_eq!(out.refused, vec![(path, TextReadError::NotRegular)]);
    assert_eq!(e.active_tab().unwrap().text(), "text\n");
    fs::remove_dir_all(&dir).ok();
}
