//! Every action, once (the coverage sweep): each id `App::run_action` can
//! dispatch runs on a fresh App over a small workspace, then a frame is drawn
//! at a wide and a narrow size, Esc is pressed twice and the frame drawn
//! again. In between, [`poke`] hovers, clicks and scrolls across the screen
//! and presses the navigation keys (never Enter), so mouse and key handlers
//! run too. Nothing is asserted beyond "no panic, no hang": the point is that
//! every overlay, panel and prompt an action opens is drawn and handled at
//! least once under test, so a panic in a rarely used one is caught here and
//! its code is measured by coverage (`tarpaulin.toml`).
//!
//! Like the chaos monkey (`tui/examples/chaos_monkey.rs`) the sweep runs in a
//! sandbox: the test re-executes its own binary with a `PATH` holding only a
//! link to `git` (no action can start a shell, a browser or a language
//! server; the workspace is a throwaway repository with no remote), a throwaway
//! `HOME` and config directory, and no `SHELL`. The parent kills a child that
//! takes longer than [`TIMEOUT`] and names the action it was on.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests fail loudly on a broken fixture"
)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use vix::app::{App, AppFlags};
use vix::settings::Settings;
use vix_action_catalog::dispatch_scan::every_dispatchable_action_id;
use vix_input::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

/// Marks the sandboxed child.
const SANDBOX_VAR: &str = "VIX_ACTION_SWEEP_SANDBOX";
/// The file the child writes the action it is on to, for a hang report.
const PROGRESS_VAR: &str = "VIX_ACTION_SWEEP_PROGRESS";
/// The whole sweep's time limit.
const TIMEOUT: Duration = Duration::from_secs(600);

/// Actions the sweep skips: each would block on something no test can
/// answer, or end the process.
const SKIP: &[&str] = &[
    // Suspends the process to the shell (SIGTSTP).
    "app.suspend",
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

fn make_workspace(root: &Path) {
    let files = [
        (
            "main.rs",
            "fn main() {\n    let x = 1;\n    println!(\"{x}\");\n}\n",
        ),
        ("notes.txt", "hello world\nsecond line\n"),
        (
            "README.md",
            "# Title\n\nSome *text* and a [link](x).\n\n- one\n- two\n",
        ),
        (
            "todo.org",
            "#+TITLE: Todo\n* TODO Write tests :work:\nSCHEDULED: <2026-11-20 Thu>\n** DONE Sub\n:PROPERTIES:\n:ID: a\n:END:\n- [ ] one\n- [X] two\n| a | b |\n|---+---|\n| 1 | 2 |\n* Projects\n** Vix [/]\n* Archive\n",
        ),
        ("data.csv", "name,qty\napple,3\npear,5\n"),
        // A task for the task chooser (never run: the sweep presses no Enter,
        // and PATH is empty).
        (
            "tasks.toml",
            "[[task]]\nname = \"build\"\ncommand = \"make\"\n",
        ),
        ("data.json", "{\"a\": [1, 2], \"b\": {\"c\": true}}\n"),
        ("query.sql", "select 1;\n"),
        ("sub/nested.txt", "nested file\n"),
    ];
    for (name, body) in files {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }
}

/// The program `name` on this process's `PATH`, if any.
fn find_on_path(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

/// Make the workspace a repository the Git actions have something to show:
/// two commits, a second branch, a stash, and an uncommitted change. Skipped
/// (the Git actions then report "not a repository") when `git` is missing.
fn make_git_history(root: &Path) {
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(root)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    if !git(&["init", "-q"]) {
        return;
    }
    git(&["add", "-A"]);
    git(&["commit", "-qm", "first"]);
    std::fs::write(root.join("notes.txt"), "hello world\nsecond line\nthird\n").unwrap();
    git(&["commit", "-qam", "second"]);
    git(&["branch", "feature"]);
    std::fs::write(root.join("notes.txt"), "stashed\n").unwrap();
    git(&["stash", "-q"]);
    std::fs::write(root.join("main.rs"), "fn main() {\n    let y = 2;\n}\n").unwrap();
}

fn new_app(root: &Path, session: &Path) -> App {
    // AI on, through the CLI provider only: its `sh -c` cannot start with
    // `git` alone on PATH, so every AI action builds its request and then
    // reports the failed command, and no HTTP provider is ever chosen.
    let mut settings = Settings::default();
    settings.ai = "on".into();
    settings.ai_provider = "cli".into();
    let mut app = App::new(root.to_path_buf(), settings).with_session_path(session.to_path_buf());
    app.layout.editor = Rect::new(0, 0, 100, 30);
    app.open_initial(&root.join("notes.txt"));
    app.open_initial(&root.join("main.rs"));
    app
}

fn draw(terminal: &mut Terminal<TestBackend>, app: &mut App) {
    terminal
        .draw(|frame| vix::ui::draw(app, frame))
        .expect("draw a frame");
}

fn resize(terminal: &mut Terminal<TestBackend>, w: u16, h: u16) {
    terminal.backend_mut().resize(w, h);
    terminal.resize(Rect::new(0, 0, w, h)).expect("resize");
}

/// Poke whatever an action opened, the way a person exploring it would:
/// hover, click and scroll across a grid of points (the menu bar row, the
/// body, the docks, the status bar), then the navigation keys, drawing after
/// each. Never Enter, so nothing is confirmed blindly.
fn poke(terminal: &mut Terminal<TestBackend>, app: &mut App) {
    let mouse = |kind, column, row| MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    for &row in &[0u16, 1, 6, 20, 34, 39] {
        for &column in &[3u16, 40, 80, 117] {
            app.on_mouse(mouse(MouseEventKind::Moved, column, row));
            app.on_mouse(mouse(MouseEventKind::Down(MouseButton::Left), column, row));
            app.on_mouse(mouse(MouseEventKind::Up(MouseButton::Left), column, row));
            if app.flags.contains(AppFlags::SHOULD_QUIT) {
                return;
            }
        }
        app.on_mouse(mouse(MouseEventKind::ScrollDown, 60, row));
        app.on_mouse(mouse(MouseEventKind::ScrollUp, 60, row));
        draw(terminal, app);
    }
    for code in [
        KeyCode::Down,
        KeyCode::Down,
        KeyCode::Up,
        KeyCode::Tab,
        KeyCode::Right,
        KeyCode::Left,
        KeyCode::PageDown,
        KeyCode::End,
        KeyCode::Home,
    ] {
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
        app.poll_command();
        if app.flags.contains(AppFlags::SHOULD_QUIT) {
            return;
        }
    }
    draw(terminal, app);
}

/// A saved macro for **Play Saved Macro…**, written to the macros file only
/// when that file is inside the sandbox's throwaway `HOME` (never a real
/// config directory).
fn seed_saved_macro() {
    let (Some(path), Some(home)) = (Settings::macros_path(), std::env::var_os("HOME")) else {
        return;
    };
    if !path.starts_with(&home) || std::env::var_os(SANDBOX_VAR).is_none() {
        return;
    }
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        "[[macro]]\nname = \"greet\"\nkeys = [\"h\", \"i\"]\n",
    )
    .unwrap();
}

/// The sweep itself, run in the sandboxed child.
fn sweep(progress: &Path) {
    let base = std::env::temp_dir().join(format!("vix-action-sweep-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let root = base.join("ws");
    make_workspace(&root);
    make_git_history(&root);
    let session = base.join("session.toml");
    let ids = every_dispatchable_action_id(&workspace_root());
    assert!(
        ids.len() > 100,
        "the dispatch scan found only {} ids",
        ids.len()
    );
    seed_saved_macro();
    let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("test backend");
    for id in ids.iter().filter(|id| !SKIP.contains(&id.as_str())) {
        std::fs::write(progress, id).unwrap();
        let mut app = new_app(&root, &session);
        // Org actions act on an Org buffer, with the cursor inside a subtree
        // (refile, archive, promote and the rest need one).
        if id.starts_with("org.") {
            app.open_initial(&root.join("todo.org"));
            if let Some(tab) = app.editor.active_tab_mut() {
                tab.editor.set_cursor_line(2);
            }
        }
        resize(&mut terminal, 120, 40);
        draw(&mut terminal, &mut app);
        app.run_action(id);
        app.poll_command();
        if app.flags.contains(AppFlags::SHOULD_QUIT) {
            continue;
        }
        draw(&mut terminal, &mut app);
        poke(&mut terminal, &mut app);
        if app.flags.contains(AppFlags::SHOULD_QUIT) {
            continue;
        }
        resize(&mut terminal, 50, 16);
        draw(&mut terminal, &mut app);
        app.on_key(esc);
        app.on_key(esc);
        resize(&mut terminal, 120, 40);
        draw(&mut terminal, &mut app);
    }
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn every_action_opens_and_draws_without_panicking() {
    if std::env::var_os(SANDBOX_VAR).is_some() {
        let progress = PathBuf::from(std::env::var_os(PROGRESS_VAR).unwrap());
        sweep(&progress);
        return;
    }
    let base =
        std::env::temp_dir().join(format!("vix-action-sweep-sandbox-{}", std::process::id()));
    let empty_path = base.join("empty-path");
    let home = base.join("home");
    let config = base.join("config");
    for dir in [&empty_path, &home, &config] {
        std::fs::create_dir_all(dir).unwrap();
    }
    // `git` is the one program the sandbox may run (a symlink, so nothing
    // else beside it is reachable), so the Git actions work on a real
    // repository; with no remote, nothing can leave the machine.
    #[cfg(unix)]
    if let Some(git) = find_on_path("git") {
        std::os::unix::fs::symlink(git, empty_path.join("git")).unwrap();
    }
    std::fs::write(
        home.join(".gitconfig"),
        "[user]\n\tname = Sweep\n\temail = sweep@example.invalid\n[init]\n\tdefaultBranch = main\n",
    )
    .unwrap();
    let progress = base.join("progress");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "every_action_opens_and_draws_without_panicking",
            "--nocapture",
        ])
        .current_dir(&home)
        .env(SANDBOX_VAR, "1")
        .env(PROGRESS_VAR, &progress)
        .env("PATH", &empty_path)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &config)
        .env("XDG_DATA_HOME", home.join("data"))
        .env_remove("SHELL")
        .env("GIT_CONFIG_GLOBAL", home.join(".gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_EDITOR", "true")
        .env("GIT_TERMINAL_PROMPT", "0")
        // Some actions write terminal escapes (font zoom) to stdout.
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if start.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let last = std::fs::read_to_string(&progress).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&base);
    match status {
        Some(status) => assert!(status.success(), "the sweep failed at action {last:?}"),
        None => panic!("the sweep hung at action {last:?}"),
    }
}
