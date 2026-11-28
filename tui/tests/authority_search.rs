//! T715: project-wide search and git run where the workspace is.
//!
//! Under a non-local authority the App's search (search-to-dock, the TODO
//! scan, go-to-definition) runs one `rg`/`grep` through
//! `Authority::command` instead of reading every file, and every `vix-git`
//! runner already builds its `git` through the authority. These tests install
//! a [`FakeAuthority`] (processes run locally with the working directory
//! mapped) and an [`SshAuthority`] over a fake `ssh` named by explicit path;
//! nothing contacts a real host.
//!
//! The authority slot is process-wide, so this is its own test binary and
//! each authority is installed by one `#[test]` at a time (a mutex).
#![cfg(all(unix, feature = "tasks"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

#[path = "support/script.rs"]
mod script;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ratatui::{Terminal, backend::TestBackend};
use vix::app::App;
use vix::settings::Settings;
use vix_authority::ssh::quote::sh_quote;
use vix_authority::ssh::{SshAuthority, SshOptions, SshTarget};
use vix_authority::{Authority, FakeAuthority};
use vix_input::{KeyCode, KeyEvent, KeyModifiers};

/// Serializes the tests: each installs its own authority in the one slot.
static SLOT: Mutex<()> = Mutex::new(());

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vix-t715-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(&dir).unwrap()
}

fn screen(app: &mut App) -> String {
    let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
    term.draw(|f| vix::ui::draw(app, f)).unwrap();
    let buf = term.backend().buffer();
    (0..30)
        .map(|y| (0..120).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Run **Search in Workspace (to dock)** for `query` and return the dock.
fn dock_search(app: &mut App, query: &str) -> String {
    app.bottom_dock.clear();
    app.run_action("search.workspace_dock");
    for c in query.chars() {
        app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    app.bottom_dock.lines.join("\n")
}

/// Whether the fake recorded a `command` for a search tool.
fn ran_search_tool(fake: &FakeAuthority) -> bool {
    fake.calls()
        .iter()
        .any(|c| c.op == "command" && c.path.file_name().is_some_and(|n| n == "rg" || n == "grep"))
}

#[cfg(feature = "git")]
fn git_available() -> bool {
    vix_fileops::which_on_path("git").is_some()
}

/// `git` in the local `dir`, quietly, for fixtures.
#[cfg(feature = "git")]
fn git(dir: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git")
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=t@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?}");
}

#[test]
fn search_and_git_go_through_an_installed_fake_authority() {
    let _slot = SLOT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = scratch("fake");
    let fake = Arc::new(FakeAuthority::new(dir.join("remote")).unwrap());
    let backing = fake.backing().join("proj");
    std::fs::create_dir_all(backing.join("src")).unwrap();
    std::fs::write(
        backing.join("src/a.rs"),
        "fn alpha() {}\n// TODO: remote todo\nlet needle = 1;\n",
    )
    .unwrap();
    std::fs::write(backing.join("src/b.rs"), "fn main() {\n    alpha();\n}\n").unwrap();
    std::fs::write(backing.join("notes.txt"), "Needle in notes\n").unwrap();
    let previous = vix_authority::install(Arc::<FakeAuthority>::clone(&fake));
    assert!(!fake.is_local());

    let root = PathBuf::from("/proj");
    let mut app =
        App::new(root.clone(), Settings::default()).with_session_path(dir.join("session.toml"));

    // Search to dock: a case-insensitive literal finds both files, through
    // one tool process rather than a read per file.
    fake.clear_calls();
    let out = dock_search(&mut app, "needle");
    assert!(out.contains("src/a.rs:3:5: let needle = 1;"), "{out}");
    assert!(out.contains("notes.txt:1:1: Needle in notes"), "{out}");
    assert!(out.contains("[2 matches in 2 files]"), "{out}");
    assert!(ran_search_tool(&fake), "{:?}", fake.calls());
    assert!(
        !fake.calls().iter().any(|c| c.op == "read"),
        "no per-file reads: {:?}",
        fake.calls()
    );

    // An open buffer's unsaved text is searched instead of the file.
    app.open_initial(&root.join("notes.txt"));
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    assert!(app.editor.insert_str("unsaved needle\n", area));
    let out = dock_search(&mut app, "needle");
    assert!(out.contains("notes.txt:1:9: unsaved needle"), "{out}");
    assert!(out.contains("notes.txt:2:1: Needle in notes"), "{out}");
    assert!(out.contains("[3 matches in 2 files]"), "{out}");

    // The TODO scan finds the remote tag.
    fake.clear_calls();
    app.run_action("tools.todo_tree");
    let shown = screen(&mut app);
    assert!(shown.contains("remote todo"), "{shown}");
    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    // Go to definition (no language server): a regex only `rg` runs, else
    // the in-process walk; either way it lands on the remote definition.
    app.open_initial(&root.join("src/b.rs"));
    app.editor.goto(2, Some(6), area);
    app.run_action("nav.goto_definition");
    assert_eq!(
        app.editor.active_tab().unwrap().path.as_deref(),
        Some(root.join("src/a.rs").as_path())
    );

    // The same search under the local authority, on the same files, lists
    // the same lines in the same order.
    let mut fresh =
        App::new(root.clone(), Settings::default()).with_session_path(dir.join("session2.toml"));
    let remote_out = dock_search(&mut fresh, "e");
    vix_authority::install(Arc::new(vix_authority::LocalAuthority));
    let mut local =
        App::new(backing.clone(), Settings::default()).with_session_path(dir.join("session3.toml"));
    let local_out = dock_search(&mut local, "e");
    vix_authority::install(Arc::<FakeAuthority>::clone(&fake));
    assert_eq!(remote_out, local_out);

    git_goes_through_the_authority(&fake, &backing, &root);

    vix_authority::install(previous);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Git under the installed fake: status and the HEAD blob a diff is taken
/// against run `git` through the authority, in the mapped directory.
#[cfg(feature = "git")]
fn git_goes_through_the_authority(fake: &FakeAuthority, backing: &Path, root: &Path) {
    if !git_available() {
        return;
    }
    git(backing, &["init", "-q"]);
    git(backing, &["add", "."]);
    git(backing, &["commit", "-q", "-m", "seed"]);
    std::fs::write(backing.join("src/b.rs"), "changed\n").unwrap();
    fake.clear_calls();
    let rows = vix::git::status(root);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].path, "src/b.rs");
    let head = vix::git::head_blob(root, "src/b.rs").unwrap();
    assert!(head.contains("alpha();"));
    let marks = vix::git::diff_marks(&head, "changed\n");
    assert_ne!(marks.len(), 0);
    assert!(
        fake.calls()
            .iter()
            .any(|c| c.op == "command" && c.path == Path::new("git")),
        "{:?}",
        fake.calls()
    );
}

/// Without the `git` feature the App has no git runners to check.
#[cfg(not(feature = "git"))]
fn git_goes_through_the_authority(_: &FakeAuthority, _: &Path, _: &Path) {}

/// A fake `ssh` that logs one `call` line per invocation to `dir/calls.log`
/// and runs the remote command with `sh -c`, the way `sshd` does.
fn fake_ssh(dir: &Path) -> PathBuf {
    let log = dir.join("calls.log");
    let script = format!(
        "#!/bin/sh\n\
         printf 'call\\n' >> {log}\n\
         while [ $# -gt 0 ]; do\n\
           case $1 in\n\
             --) shift; break ;;\n\
             -o|-p|-l) shift 2 ;;\n\
             -O) exit 0 ;;\n\
             *) shift ;;\n\
           esac\n\
         done\n\
         shift\n\
         exec /bin/sh -c \"$*\"\n",
        log = String::from_utf8(sh_quote(log.to_str().unwrap().as_bytes())).unwrap(),
    );
    let path = dir.join("fake-ssh");
    script::write(&path, &script).unwrap();
    path
}

fn ssh_calls(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join("calls.log"))
        .unwrap_or_default()
        .lines()
        .count()
}

#[test]
fn search_over_ssh_is_one_remote_tool_run_not_a_read_per_file() {
    let _slot = SLOT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = scratch("ssh");
    let root = dir.join("work");
    std::fs::create_dir_all(&root).unwrap();
    for i in 0..40 {
        std::fs::write(root.join(format!("f{i:02}.txt")), format!("line {i}\n")).unwrap();
    }
    std::fs::write(root.join("f17.txt"), "a remote needle\n").unwrap();
    let target = SshTarget::parse(&format!("ssh://me@example.invalid{}", root.display())).unwrap();
    let control = Path::new("/tmp").join(format!(
        "ctl-{}",
        dir.file_name().unwrap().to_string_lossy()
    ));
    let _ = std::fs::remove_dir_all(&control);
    let options = SshOptions::default()
        .with_program(fake_ssh(&dir))
        .with_control_dir(&control);
    let ssh = Arc::new(SshAuthority::with_options(target, options).unwrap());
    let previous = vix_authority::install(ssh);

    let mut app = App::new(root, Settings::default()).with_session_path(dir.join("session.toml"));
    let before = ssh_calls(&dir);
    let out = dock_search(&mut app, "needle");
    let used = ssh_calls(&dir) - before;
    assert!(out.contains("f17.txt:1:10: a remote needle"), "{out}");
    assert!(out.contains("[1 matches in 1 files]"), "{out}");
    // The file index walk, `which rg` (and `which grep` without `rg`), and
    // the tool itself; reading 41 files would take at least 41.
    assert!(used <= 5, "{used} ssh round trips");

    vix_authority::install(previous);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&control);
}

/// T869: a delayed fake authority. The local file system, except that while
/// it is held every text read waits until [`Held::release`], so a test can
/// keep a background scan pending for as long as it likes and prove what the
/// App does meanwhile.
#[derive(Debug, Default)]
struct Held {
    /// Whether reads wait.
    closed: Mutex<bool>,
    /// Signalled on release.
    released: std::sync::Condvar,
    /// Reads that reached the gate.
    reads: std::sync::atomic::AtomicUsize,
}

impl Held {
    fn hold(&self) {
        *self.closed.lock().unwrap() = true;
    }

    fn release(&self) {
        *self.closed.lock().unwrap() = false;
        self.released.notify_all();
    }

    fn is_held(&self) -> bool {
        *self.closed.lock().unwrap()
    }

    fn reads(&self) -> usize {
        self.reads.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Wait while held, counting the read.
    fn gate(&self) {
        self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut held = self.closed.lock().unwrap();
        while *held {
            held = self.released.wait(held).unwrap();
        }
    }

    /// Wait (up to 30 s) until a held read is pending: the scan has
    /// started and is stuck on the gate.
    fn wait_for_a_pending_read(&self) {
        let start = std::time::Instant::now();
        while self.reads() == 0 {
            assert!(start.elapsed().as_secs() < 30, "no scan reached the gate");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
}

/// [`Held`] as an authority over the local file system.
#[derive(Debug, Default)]
struct HeldAuthority {
    gate: Arc<Held>,
}

impl Authority for HeldAuthority {
    fn name(&self) -> &'static str {
        "held"
    }
    fn is_local(&self) -> bool {
        true
    }
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        vix_authority::LocalAuthority.read(path)
    }
    fn read_limited(&self, path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
        vix_authority::LocalAuthority.read_limited(path, limit)
    }
    fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        vix_authority::LocalAuthority.read_to_string(path)
    }
    fn read_text_guarded(
        &self,
        path: &Path,
        max_bytes: u64,
    ) -> std::io::Result<vix_authority::TextFile> {
        self.gate.gate();
        vix_authority::LocalAuthority.read_text_guarded(path, max_bytes)
    }
    fn read_to_string_guarded(&self, path: &Path) -> std::io::Result<String> {
        self.gate.gate();
        vix_authority::LocalAuthority.read_to_string_guarded(path)
    }
    fn write(&self, path: &Path, data: &[u8]) -> std::io::Result<()> {
        vix_authority::LocalAuthority.write(path, data)
    }
    fn write_atomic(&self, path: &Path, data: &[u8]) -> std::io::Result<()> {
        vix_authority::LocalAuthority.write_atomic(path, data)
    }
    fn write_text_in_file_encoding(&self, path: &Path, text: &str) -> std::io::Result<()> {
        vix_authority::LocalAuthority.write_text_in_file_encoding(path, text)
    }
    fn metadata(&self, path: &Path) -> std::io::Result<vix_authority::FileStat> {
        vix_authority::LocalAuthority.metadata(path)
    }
    fn symlink_metadata(&self, path: &Path) -> std::io::Result<vix_authority::FileStat> {
        vix_authority::LocalAuthority.symlink_metadata(path)
    }
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        vix_authority::LocalAuthority.canonicalize(path)
    }
    fn read_dir(&self, dir: &Path) -> std::io::Result<Vec<vix_authority::DirEntry>> {
        vix_authority::LocalAuthority.read_dir(dir)
    }
    fn create_dir_all(&self, dir: &Path) -> std::io::Result<()> {
        vix_authority::LocalAuthority.create_dir_all(dir)
    }
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        vix_authority::LocalAuthority.rename(from, to)
    }
    fn remove_file(&self, path: &Path) -> std::io::Result<()> {
        vix_authority::LocalAuthority.remove_file(path)
    }
    fn remove_path(&self, path: &Path) -> std::io::Result<()> {
        vix_authority::LocalAuthority.remove_path(path)
    }
    fn move_path(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        vix_authority::LocalAuthority.move_path(from, to)
    }
    fn copy_recursive(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        vix_authority::LocalAuthority.copy_recursive(from, to)
    }
    fn trash(&self, path: &Path) -> std::io::Result<()> {
        vix_authority::LocalAuthority.trash(path)
    }
    fn is_unwritable(&self, path: &Path) -> bool {
        vix_authority::LocalAuthority.is_unwritable(path)
    }
    fn walk_files(&self, root: &Path, opts: &vix_authority::WalkOptions) -> Vec<PathBuf> {
        vix_authority::LocalAuthority.walk_files(root, opts)
    }
    fn which(&self, name: &str) -> Option<PathBuf> {
        vix_authority::LocalAuthority.which(name)
    }
    fn command(&self, spec: &vix_authority::ProcessSpec) -> std::io::Result<std::process::Command> {
        vix_authority::LocalAuthority.command(spec)
    }
}

/// A small tree for the held-scan tests: files with `needle`, one with
/// only `nexus`, and filler.
fn held_tree(tag: &str) -> PathBuf {
    let dir = scratch(tag);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/a.rs"), "fn a() {}\nlet needle = 1;\n").unwrap();
    std::fs::write(dir.join("src/b.rs"), "// nexus only\n").unwrap();
    std::fs::write(dir.join("src/c.rs"), "needle one\nneedle two\n").unwrap();
    for i in 0..40 {
        std::fs::write(dir.join(format!("src/f{i}.rs")), format!("filler {i}\n")).unwrap();
    }
    dir
}

fn type_keys(app: &mut App, text: &str) {
    for c in text.chars() {
        app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
}

fn panel_query(app: &App) -> Option<String> {
    app.workspace_search_panel().map(|p| p.query.clone())
}

/// The audit's reproduction through the real key path (PQ-08): with the
/// scan held pending on the authority, every keystroke and Escape returns
/// and takes effect before any file is read.
#[test]
fn a_held_scan_does_not_hold_up_typing_or_escape() {
    let _slot = SLOT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = held_tree("held-keys");
    let auth = Arc::new(HeldAuthority::default());
    let gate = Arc::clone(&auth.gate);
    let previous = vix_authority::install(Arc::<HeldAuthority>::clone(&auth));
    let mut app =
        App::new(dir.clone(), Settings::default()).with_session_path(dir.join("session.toml"));
    app.run_action("search.workspace");
    gate.hold();
    // A key handler that scanned on the spot would wait on the gate for
    // ever; the watchdog opens it, so such a regression fails the
    // `is_held` checks below instead of hanging.
    let watchdog = Arc::clone(&gate);
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(20));
        watchdog.release();
    });

    // The first query starts a scan that blocks on its first read; the key
    // handler has already returned.
    type_keys(&mut app, "ne");
    gate.wait_for_a_pending_read();
    assert!(gate.is_held());
    assert!(app.workspace_search_busy());
    assert_eq!(panel_query(&app).as_deref(), Some("ne"));
    assert!(app.workspace_search_hits().is_empty());
    let shown = screen(&mut app);
    assert!(shown.contains("Searching"), "{shown}");

    // A new query is processed while the first scan is still pending.
    type_keys(&mut app, "x");
    assert!(gate.is_held());
    assert_eq!(panel_query(&app).as_deref(), Some("nex"));
    assert!(app.workspace_search_busy());

    // So is Escape: the panel closes and its scan is dropped.
    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(gate.is_held());
    assert!(app.workspace_search_panel().is_none());
    assert!(!app.workspace_search_busy());

    // The cancelled scans finish reading into nothing.
    gate.release();
    std::thread::sleep(std::time::Duration::from_millis(50));
    app.tick();
    assert!(app.workspace_search_panel().is_none());
    assert!(app.workspace_search_hits().is_empty());

    vix_authority::install(previous);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A slow scan for an earlier query never lands over a newer one's hits,
/// the newest query's hits come in index order, and the unsaved buffers a
/// scan reads are the ones at its keystroke.
#[test]
fn a_stale_scan_is_discarded_and_buffers_are_read_as_of_the_keystroke() {
    let _slot = SLOT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = held_tree("held-stale");
    let auth = Arc::new(HeldAuthority::default());
    let gate = Arc::clone(&auth.gate);
    let previous = vix_authority::install(Arc::<HeldAuthority>::clone(&auth));
    let mut app =
        App::new(dir.clone(), Settings::default()).with_session_path(dir.join("session.toml"));

    // An unsaved edit gives src/b.rs a needle.
    app.open_initial(&dir.join("src/b.rs"));
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    assert!(app.editor.insert_str("unsaved needle\n", area));

    app.run_action("search.workspace");
    gate.hold();
    type_keys(&mut app, "ne"); // matches nexus too
    gate.wait_for_a_pending_read();
    type_keys(&mut app, "ed"); // "need": the "ne" scan is now stale
    // An edit after the keystroke is not what the pending scan reads.
    assert!(app.editor.insert_str("and another needle\n", area));
    gate.release();
    app.wait_for_workspace_search();
    assert!(!app.workspace_search_busy());

    let hits: Vec<(String, usize)> = app
        .workspace_search_hits()
        .iter()
        .map(|h| (h.rel.clone(), h.line))
        .collect();
    assert!(
        !app.workspace_search_hits()
            .iter()
            .any(|h| h.text.contains("nexus")),
        "the nexus line matched only the stale query: {hits:?}"
    );
    let mut b_lines: Vec<usize> = hits
        .iter()
        .filter(|(rel, _)| rel == "src/b.rs")
        .map(|(_, l)| *l)
        .collect();
    b_lines.sort_unstable();
    assert_eq!(b_lines, vec![1], "the buffer as of the keystroke: {hits:?}");
    for want in [("src/a.rs", 2), ("src/c.rs", 1), ("src/c.rs", 2)] {
        assert!(
            hits.iter().any(|(rel, l)| rel == want.0 && *l == want.1),
            "{want:?} in {hits:?}"
        );
    }
    assert_eq!(hits.len(), 4, "{hits:?}");
    let status = &app.workspace_search_panel().unwrap().status;
    assert!(status.contains('4') && status.contains('3'), "{status}");
    let timing = app.workspace_search_timing().unwrap();
    assert!(timing.first_result <= timing.complete);

    // The next keystroke snapshots the buffer again; the same search over
    // the same state lists the same hits in the same order.
    app.on_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    type_keys(&mut app, "d");
    app.wait_for_workspace_search();
    let again: Vec<(String, usize)> = app
        .workspace_search_hits()
        .iter()
        .map(|h| (h.rel.clone(), h.line))
        .collect();
    assert_eq!(again.len(), 5, "both unsaved lines now: {again:?}");
    app.on_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    type_keys(&mut app, "d");
    app.wait_for_workspace_search();
    let third: Vec<(String, usize)> = app
        .workspace_search_hits()
        .iter()
        .map(|h| (h.rel.clone(), h.line))
        .collect();
    assert_eq!(again, third);

    vix_authority::install(previous);
    let _ = std::fs::remove_dir_all(&dir);
}
