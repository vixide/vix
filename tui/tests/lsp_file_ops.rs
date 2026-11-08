//! End-to-end tests for T623 against a tiny Python mock language server over
//! local pipes (skips if `python3` is absent): `will/didCreateFiles`,
//! `will/didRenameFiles`, `will/didDeleteFiles` with file and folder filters,
//! the explorer's delete and move, and recursive `didChangeWatchedFiles`.

#![cfg(feature = "lsp")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests fail loudly on a broken fixture"
)]

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use vix::app::App;
use vix::lsp::{FileEdits, Lsp, LspEvent};
use vix::lsp_core::file_ops::{FileChangeType, FileOp, FileOpItem};
use vix::settings::{LspServer, Settings};

fn tool_available(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Wants every file operation for `*.rs` files and for folders named `pkg*`,
/// registers recursive `Cargo.toml` and `*.cfg` watchers dynamically, answers
/// each `will*Files` with one edit to `argv[2]/imports.rs` (`argv[2]` is the
/// root's `file://` URI), and logs every file
/// message it receives to `argv[1]`.
const MOCK_SERVER_FILE_OPS: &str = include_str!("fixtures/mock_lsp_file_ops.py");

/// A scratch workspace with the mock server script, its log and `imports.rs`.
struct Fixture {
    root: PathBuf,
    log: PathBuf,
    server: LspServer,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("vix-lsp-fileops-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let mock = root.join("mock_lsp.py");
        std::fs::write(&mock, MOCK_SERVER_FILE_OPS).unwrap();
        let log = root.join("log.txt");
        std::fs::write(root.join("imports.rs"), "abc\n").unwrap();
        let server = LspServer {
            language_id: "rust".into(),
            extensions: vec!["rs".into()],
            command: vec![
                "python3".into(),
                mock.to_string_lossy().into_owned(),
                log.to_string_lossy().into_owned(),
                vix::lsp::path_to_uri(&root),
            ],
        };
        Self { root, log, server }
    }

    fn logged(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    /// Wait until the log contains `needle` (or 15 s pass), running `step`
    /// between checks.
    fn wait_for(&self, needle: &str, mut step: impl FnMut()) -> bool {
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            if self.logged().contains(needle) {
                return true;
            }
            step();
            std::thread::sleep(Duration::from_millis(20));
        }
        self.logged().contains(needle)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Open `file` and poll until the mock server has answered (it publishes a
/// diagnostic on `didOpen`, after its capabilities and watchers are known).
fn open_and_wait(lsp: &mut Lsp, file: &std::path::Path) {
    lsp.did_open(file, "abc\n");
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline && lsp.diagnostics_for(file).is_empty() {
        let _ = lsp.poll();
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Poll `lsp` until a `WillFileOperations` event arrives.
fn next_will_reply(lsp: &mut Lsp) -> (Vec<FileEdits>, bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        for ev in lsp.poll() {
            if let LspEvent::WillFileOperations { edits, last } = ev {
                return (edits, last);
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("no will*Files reply");
}

/// Each operation is sent only the paths the server's filters match (files
/// and, with the `folder` kind, directories), as a request before and a
/// notification after, and the edits come back.
#[test]
fn will_and_did_file_operations_honor_file_and_folder_filters() {
    if !tool_available("python3") {
        eprintln!("python3 not available; skipping");
        return;
    }
    let fx = Fixture::new("filters");
    let file = fx.root.join("a.rs");
    std::fs::write(&file, "abc\n").unwrap();
    std::fs::create_dir_all(fx.root.join("pkg")).unwrap();
    std::fs::write(fx.root.join("notes.txt"), "n\n").unwrap();

    let mut lsp = Lsp::new(
        true,
        vec![fx.server.clone()],
        std::slice::from_ref(&fx.root),
    );
    open_and_wait(&mut lsp, &file);
    assert!(lsp.watches_files(), "the dynamic registration was kept");

    // Create: only the `.rs` file is wanted, not the `.txt`.
    let create = [(
        FileOp::Create,
        vec![
            FileOpItem::single(&fx.root.join("new.rs"), false),
            FileOpItem::single(&fx.root.join("new.txt"), false),
        ],
    )];
    assert!(lsp.will_file_operations(&create));
    let (edits, last) = next_will_reply(&mut lsp);
    assert!(last);
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].0, fx.root.join("imports.rs"));
    assert!(fx.logged().contains("will Create file://"));
    assert!(fx.logged().contains("new.rs"));
    assert!(!fx.logged().contains("new.txt"), "{}", fx.logged());

    // Delete: a folder named `pkg` matches the folder filter, a file called
    // `pkg` does not.
    let as_file = [(
        FileOp::Delete,
        vec![FileOpItem::single(&fx.root.join("pkg"), false)],
    )];
    assert!(
        !lsp.will_file_operations(&as_file),
        "a file is not a folder"
    );
    let delete = [(
        FileOp::Delete,
        vec![
            FileOpItem::single(&fx.root.join("pkg"), true),
            FileOpItem::single(&fx.root.join("notes.txt"), false),
        ],
    )];
    assert!(lsp.will_file_operations(&delete));
    let (_, last) = next_will_reply(&mut lsp);
    assert!(last);
    assert!(fx.logged().contains("will Delete file://"));
    assert!(!fx.logged().contains("notes.txt"), "{}", fx.logged());

    // A folder rename is announced by the real directory on disk.
    assert!(lsp.will_rename_files(&fx.root.join("pkg"), &fx.root.join("pkg2")));
    let (_, last) = next_will_reply(&mut lsp);
    assert!(last);

    // After the fact: `did*` notifications under the same filters.
    let changes = lsp.did_file_operations(&[
        (
            FileOp::Create,
            vec![
                FileOpItem::single(&fx.root.join("new.rs"), false),
                FileOpItem::single(&fx.root.join("new.txt"), false),
            ],
        ),
        (
            FileOp::Delete,
            vec![FileOpItem::single(&fx.root.join("pkg"), true)],
        ),
        (
            FileOp::Rename,
            vec![FileOpItem::renamed(
                &fx.root.join("pkg"),
                &fx.root.join("pkg2"),
                true,
            )],
        ),
    ]);
    // Only files count as watched-file events; the folder ones are left to
    // the host's scan.
    assert_eq!(
        changes,
        vec![
            (fx.root.join("new.rs"), FileChangeType::Created),
            (fx.root.join("new.txt"), FileChangeType::Created),
        ]
    );
    assert!(fx.wait_for("did Rename file://", || {
        let _ = lsp.poll();
    }));
    let text = fx.logged();
    assert!(text.contains("did Create file://"), "{text}");
    assert!(text.contains("did Delete file://"), "{text}");
    assert!(text.contains("pkg2"), "{text}");
    assert!(
        !text
            .lines()
            .any(|l| l.starts_with("did Create") && l.contains("new.txt")),
        "{text}"
    );
    lsp.shutdown();
}

/// A rename still asks for edits, then tells the server, and a registered
/// `Cargo.toml` watcher gets `didChangeWatchedFiles` while an unwatched file
/// does not.
#[test]
fn mock_server_gets_rename_and_watched_file_notifications() {
    if !tool_available("python3") {
        eprintln!("python3 not available; skipping");
        return;
    }
    let fx = Fixture::new("rename");
    let file = fx.root.join("a.rs");
    std::fs::write(&file, "abc\n").unwrap();
    let renamed = fx.root.join("b.rs");

    let mut lsp = Lsp::new(
        true,
        vec![fx.server.clone()],
        std::slice::from_ref(&fx.root),
    );
    open_and_wait(&mut lsp, &file);
    assert!(lsp.will_rename_files(&file, &renamed));
    let (edits, last) = next_will_reply(&mut lsp);
    assert!(last);
    assert_eq!(edits[0].0, fx.root.join("imports.rs"));

    lsp.did_rename_files(&file, &renamed);
    lsp.did_change_watched_files(&[
        (fx.root.join("Cargo.toml"), FileChangeType::Changed),
        (fx.root.join("notes.txt"), FileChangeType::Changed),
    ]);
    assert!(fx.wait_for("Cargo.toml", || {
        let _ = lsp.poll();
    }));
    let text = fx.logged();
    lsp.shutdown();
    assert!(text.contains("will Rename file://"), "{text}");
    assert!(text.contains("did Rename file://"), "{text}");
    assert!(text.contains("watch 2 file://"), "{text}");
    assert!(!text.contains("notes.txt"), "{text}");
    // The rename is also a watched-file create/delete, but these watchers only
    // match `Cargo.toml` and `*.cfg`.
    assert!(!text.contains("watch 1"), "{text}");
}

fn app_with_server(fx: &Fixture) -> App {
    let mut settings = Settings::default();
    settings.lsp_servers.push(fx.server.clone());
    settings.explorer_delete = "hard".to_string();
    let mut app =
        App::new(fx.root.clone(), settings).with_backup_dir(fx.root.join(".vix-test-backup"));
    app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
    app
}

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

/// A registered glob watcher sees files nobody has open, in subdirectories:
/// the bounded scan reports a created, a changed and a deleted `.cfg` file,
/// and nothing for files that were already there at the start.
#[test]
fn recursive_scan_reports_nested_watched_files() {
    if !tool_available("python3") {
        eprintln!("python3 not available; skipping");
        return;
    }
    let fx = Fixture::new("scan");
    let file = fx.root.join("a.rs");
    std::fs::write(&file, "abc\n").unwrap();
    std::fs::create_dir_all(fx.root.join("deep/er")).unwrap();
    std::fs::write(fx.root.join("deep/pre.cfg"), "1").unwrap();
    std::fs::write(fx.root.join("deep/gone.cfg"), "1").unwrap();

    let mut app = app_with_server(&fx);
    app.open_initial(&file);
    wait_until(&mut app, "the server to be ready", |app| {
        app.lsp.watches_files() && !app.lsp.diagnostics_for(&file).is_empty()
    });
    // Let at least one scan record the baseline.
    let settle = Instant::now() + Duration::from_secs(5);
    while Instant::now() < settle {
        app.tick();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!fx.logged().contains("pre.cfg"), "baseline is silent");

    std::fs::write(fx.root.join("deep/er/new.cfg"), "1").unwrap();
    std::fs::write(fx.root.join("deep/pre.cfg"), "changed, longer").unwrap();
    std::fs::remove_file(fx.root.join("deep/gone.cfg")).unwrap();
    assert!(fx.wait_for("watch 3 ", || app.tick()), "{}", fx.logged());
    assert!(fx.wait_for("watch 1 ", || app.tick()), "{}", fx.logged());
    assert!(fx.wait_for("watch 2 ", || app.tick()), "{}", fx.logged());
    let text = fx.logged();
    assert!(
        text.lines()
            .any(|l| l.starts_with("watch 1 ") && l.ends_with("deep/er/new.cfg")),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|l| l.starts_with("watch 2 ") && l.ends_with("deep/pre.cfg")),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|l| l.starts_with("watch 3 ") && l.ends_with("deep/gone.cfg")),
        "{text}"
    );
    app.lsp.shutdown();
}
