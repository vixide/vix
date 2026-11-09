//! T623: the explorer's delete and move reach language servers as
//! `will/didDeleteFiles`, `will/didRenameFiles` and `didCreateFiles` (folders
//! included, honoring the server's filters). A Python mock server over local
//! pipes; skipped when `python3` is absent. The same mock drives the
//! protocol-level tests in `tests/lsp_file_ops.rs`.
#![cfg(feature = "lsp")]

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::app::{App, Focus};
use crate::integration::common::{ctrl, key, keycode};
use crate::settings::{LspServer, Settings};
use vix_input::KeyCode;

/// The mock server shared with `tests/lsp_file_ops.rs`.
const MOCK: &str = include_str!("../fixtures/mock_lsp_file_ops.py");

fn python_available() -> bool {
    Command::new("python3")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn node_index(app: &App, name: &str) -> usize {
    app.explorer
        .nodes
        .iter()
        .position(|n| n.name == name)
        .unwrap_or_else(|| panic!("no explorer node named {name}"))
}

/// Tick `app` until `done`, up to 15 s.
fn wait_until(app: &mut App, what: &str, mut done: impl FnMut(&mut App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if done(app) {
            return;
        }
        app.tick();
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out waiting for {what}");
}

#[test]
fn explorer_delete_move_and_copy_reach_the_server() {
    if !python_available() {
        eprintln!("python3 not available; skipping");
        return;
    }
    let root =
        std::env::temp_dir().join(format!("vix-lsp-explorer-fileops-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let mock = root.join("mock_lsp.py");
    std::fs::write(&mock, MOCK).unwrap();
    let log: PathBuf = root.join("log.txt");
    let logged = || std::fs::read_to_string(&log).unwrap_or_default();
    let file = root.join("a.rs");
    std::fs::write(&file, "abc\n").unwrap();
    std::fs::write(root.join("imports.rs"), "abc\n").unwrap();
    std::fs::write(root.join("m.rs"), "m\n").unwrap();
    std::fs::create_dir_all(root.join("pkg")).unwrap();
    std::fs::create_dir_all(root.join("dest")).unwrap();

    let mut settings = Settings::default();
    settings.explorer_delete = "hard".to_string();
    settings.lsp_servers.push(LspServer {
        language_id: "rust".into(),
        extensions: vec!["rs".into()],
        command: vec![
            "python3".into(),
            mock.to_string_lossy().into_owned(),
            log.to_string_lossy().into_owned(),
            vix::lsp::path_to_uri(&root),
        ],
    });
    let mut app = App::new(root.clone(), settings).with_backup_dir(root.join(".vix-test-backup"));
    app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
    app.open_initial(&file);
    wait_until(&mut app, "the server to be ready", |app| {
        !app.lsp.diagnostics_for(&file).is_empty()
    });

    // Delete the `pkg` folder: the server's folder filter matches it.
    app.focus = Focus::Explorer;
    app.explorer.selected = node_index(&app, "pkg");
    app.on_key(keycode(KeyCode::Delete));
    app.on_key(key('y'));
    wait_until(&mut app, "pkg to be deleted", |_| {
        !root.join("pkg").exists()
    });
    assert_eq!(
        std::fs::read_to_string(root.join("imports.rs")).unwrap(),
        "use\n",
        "the server's willDeleteFiles edit was applied before the delete"
    );
    wait_until(&mut app, "didDeleteFiles", |_| {
        logged().contains("did Delete file://")
    });
    assert!(logged().contains("will Delete file://"));

    // Move `m.rs` into `dest/` (cut and paste): a rename.
    app.explorer.selected = node_index(&app, "m.rs");
    app.on_key(ctrl('x'));
    app.explorer.selected = node_index(&app, "dest");
    app.on_key(ctrl('v'));
    wait_until(&mut app, "m.rs to move", |_| {
        root.join("dest/m.rs").exists()
    });
    wait_until(&mut app, "didRenameFiles", |_| {
        logged().contains("did Rename file://")
    });
    let text = logged();
    assert!(text.contains("will Rename file://"), "{text}");
    assert!(
        text.lines()
            .any(|l| l.starts_with("did Rename") && l.ends_with("dest/m.rs")),
        "{text}"
    );

    // Copy `a.rs` into `dest/` (copy and paste): a create.
    app.explorer.selected = node_index(&app, "a.rs");
    app.on_key(ctrl('c'));
    app.explorer.selected = node_index(&app, "dest");
    app.on_key(ctrl('v'));
    wait_until(&mut app, "didCreateFiles", |_| {
        logged().contains("did Create file://")
    });
    assert!(root.join("dest/a.rs").exists());

    app.lsp.shutdown();
    let _ = std::fs::remove_dir_all(&root);
}
