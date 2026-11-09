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

fn git_available() -> bool {
    vix_fileops::which_on_path("git").is_some()
}

/// `git` in the local `dir`, quietly, for fixtures.
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

    // Git: status and the HEAD blob a diff is taken against run `git`
    // through the authority, in the mapped directory.
    if git_available() {
        git(&backing, &["init", "-q"]);
        git(&backing, &["add", "."]);
        git(&backing, &["commit", "-q", "-m", "seed"]);
        std::fs::write(backing.join("src/b.rs"), "changed\n").unwrap();
        fake.clear_calls();
        let rows = vix::git::status(&root);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].path, "src/b.rs");
        let head = vix::git::head_blob(&root, "src/b.rs").unwrap();
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

    vix_authority::install(previous);
    let _ = std::fs::remove_dir_all(&dir);
}

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
