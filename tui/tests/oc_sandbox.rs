//! The OC (containers) panel against a fake `docker`, in a sandbox.
//!
//! The test re-executes its own binary with a `PATH` holding only a fake
//! `docker` (shell built-ins and canned JSON; it runs nothing) and a
//! throwaway `HOME`, then drives the panel: listing every kind, the
//! confirm-then-run flow, the read-only guard, inspect, the port URL,
//! logs, prompts, the compose actions and the mouse. Nothing is asserted
//! about the fake's output beyond what the app reports about it.
#![cfg(all(unix, feature = "oc"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests fail loudly on a broken fixture"
)]

use std::path::Path;
use std::process::Command;

use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use vix::app::App;
use vix::settings::Settings;
use vix_input::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

/// Marks the sandboxed child.
const SANDBOX_VAR: &str = "VIX_OC_SANDBOX";

/// A fake `docker`: canned listings, an echo for everything else.
const FAKE_DOCKER: &str = r#"#!/bin/sh
case "$1 $2" in
  "ps "*) echo '{"ID":"abc123def456","Names":"web","Image":"nginx:latest","State":"running","Status":"Up 2 hours","Ports":"0.0.0.0:8080->80/tcp"}'
          echo '{"ID":"0011223344","Names":"db","Image":"postgres:16","State":"exited","Status":"Exited (0) 1 hour ago"}' ;;
  "images "*) echo '{"ID":"sha256:1111","Repository":"nginx","Tag":"latest","Size":"10MB","CreatedSince":"2 weeks ago"}' ;;
  "volume ls") echo '{"Name":"data","Driver":"local","Mountpoint":"/var/lib/docker/volumes/data"}' ;;
  "network ls") echo '{"ID":"n1","Name":"bridge","Driver":"bridge","Scope":"local"}' ;;
  "pod ps") echo '[]' ;;
  "inspect "*) echo '[{"Id":"abc123def456","State":{"Status":"running"}}]' ;;
  "version "*|"info "*) echo '{"Client":{"Version":"99.0"}}' ;;
  *) echo "fake docker: $*" ;;
esac
"#;

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

fn code(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn click(col: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

/// Draw one frame and return its text.
fn screen(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| vix::ui::draw(app, f)).unwrap();
    let buf = terminal.backend().buffer();
    (0..40)
        .map(|y| (0..120).map(|x| buf[(x, y)].symbol()).collect::<String>() + "\n")
        .collect()
}

fn new_app(root: &Path, mode: &str) -> App {
    let mut settings = Settings::default();
    settings.oc_mode = mode.to_string();
    let mut app = App::new(root.to_path_buf(), settings);
    app.layout.editor = Rect::new(0, 0, 100, 30);
    app
}

fn body() {
    let home = std::env::var_os("HOME").expect("sandbox HOME");
    let root = Path::new(&home).join("work");
    std::fs::create_dir_all(&root).unwrap();

    // ---- engine detection and choice ----------------------------------
    let mut app = new_app(&root, "full");
    app.run_action("oc.engine_detect");
    assert!(app.status.contains("docker"), "status: {}", app.status);
    app.run_action("oc.engine_choose");
    assert!(app.status.contains("docker"), "status: {}", app.status);
    app.run_action("oc.engine_choose");
    assert!(app.status.contains("auto"), "wraps back: {}", app.status);

    // ---- containers: browse, port URL, show-all, kinds -----------------
    app.run_action("oc.containers");
    let text = screen(&mut app);
    assert!(text.contains("web") && text.contains("db"), "{text}");
    app.on_key(code(KeyCode::Down));
    app.on_key(code(KeyCode::Up));
    app.on_key(code(KeyCode::PageDown));
    app.on_key(code(KeyCode::PageUp));
    app.on_key(key('o'));
    assert!(
        app.status.contains("8080"),
        "the port URL is copied: {}",
        app.status
    );
    app.on_key(key('a')); // show all, reloads
    app.on_key(key('r')); // reload
    for _ in 0..5 {
        app.on_key(code(KeyCode::Tab)); // images, volumes, networks, ...
        screen(&mut app);
    }
    for _ in 0..5 {
        app.on_key(code(KeyCode::BackTab));
    }

    // ---- confirm then run: stop the first container --------------------
    app.on_key(key('x'));
    assert!(screen(&mut app).contains("stop"), "the command is shown");
    app.on_key(key('y'));
    assert!(app.status.contains("stop"), "ran: {}", app.status);
    // Any other key cancels a confirm: nothing runs.
    let before = app.status.clone();
    app.on_key(key('x'));
    app.on_key(key('n'));
    assert_eq!(app.status, before, "a declined confirm changes nothing");
    app.on_key(key('d'));
    app.on_key(code(KeyCode::Esc));
    app.on_key(key('D')); // prune
    app.on_key(code(KeyCode::Esc));
    for c in ['s', 't', 'p'] {
        app.on_key(key(c));
        app.on_key(code(KeyCode::Esc));
    }

    mouse_inspect_and_prompts(&mut app);
    compose_and_read_only(&root);
}

/// Mouse selection and inspect, then the inspect/logs/workspace keys and the
/// image prompts.
fn mouse_inspect_and_prompts(app: &mut App) {
    // ---- mouse: first click selects, a second inspects -----------------
    screen(app);
    let r = app.layout.oc;
    assert!(r.height > 3, "the panel is drawn");
    app.on_mouse(click(r.x + 2, r.y + 1));
    app.on_mouse(click(r.x + 2, r.y + 1));
    screen(app);

    // ---- inspect opens a read-only tab; logs stream to the dock --------
    app.run_action("oc.containers");
    app.on_key(key('i'));
    app.run_action("oc.containers");
    app.on_key(key('l'));
    app.run_action("oc.containers");
    app.on_key(code(KeyCode::Enter));
    app.run_action("oc.containers");
    app.on_key(key('w')); // open workspace prompt (running container)
    app.on_key(code(KeyCode::Esc));
    app.on_key(code(KeyCode::Esc));

    // ---- images: run/tag/pull/push prompts ------------------------------
    app.run_action("oc.images");
    for c in ['u', 'g', 'p'] {
        app.on_key(key(c));
        screen(app);
        app.on_key(code(KeyCode::Esc));
    }
    app.on_key(key('P'));
    app.on_key(key('n'));
    app.on_key(code(KeyCode::Esc));
    for action in ["oc.run", "oc.pull", "oc.build", "oc.prune"] {
        app.run_action(action);
        screen(app);
        app.on_key(code(KeyCode::Esc));
        app.on_key(code(KeyCode::Esc));
    }
    for action in ["oc.volumes", "oc.networks", "oc.pods"] {
        app.run_action(action);
        screen(app);
        app.on_key(code(KeyCode::Esc));
    }
}

/// Compose actions with and without a compose file, then read-only mode.
fn compose_and_read_only(root: &Path) {
    let mut app = new_app(root, "full");
    // ---- compose: no file, then with one --------------------------------
    app.run_action("oc.compose_up");
    assert!(
        app.status.to_lowercase().contains("compose"),
        "no compose file: {}",
        app.status
    );
    std::fs::write(root.join("docker-compose.yml"), "services: {}\n").unwrap();
    app.run_action("oc.compose_up");
    assert!(screen(&mut app).contains("compose"), "asks to confirm");
    app.on_key(key('n'));
    app.run_action("oc.compose_down");
    app.on_key(key('n'));
    app.on_key(code(KeyCode::Esc));
    app.run_action("oc.compose_logs");
    app.run_action("oc.open_workspace");
    app.run_action("oc.devcontainer_reopen");

    // ---- read-only mode refuses every change -----------------------------
    let mut ro = new_app(root, "read_only");
    ro.run_action("oc.containers");
    ro.on_key(key('x'));
    assert!(ro.status.to_lowercase().contains("read"), "{}", ro.status);
    ro.status.clear();
    ro.on_key(key('d'));
    assert!(ro.status.to_lowercase().contains("read"), "{}", ro.status);
    ro.status.clear();
    ro.run_action("oc.run");
    assert!(ro.status.to_lowercase().contains("read"), "{}", ro.status);
    ro.run_action("oc.compose_up");
    ro.on_key(code(KeyCode::Esc));
}

#[test]
fn oc_panel_against_a_fake_docker() {
    if std::env::var_os(SANDBOX_VAR).is_some() {
        body();
        return;
    }
    let base = std::env::temp_dir().join(format!("vix-oc-sandbox-{}", std::process::id()));
    let bin = base.join("bin");
    let home = base.join("home");
    let config = base.join("config");
    for dir in [&bin, &home, &config] {
        std::fs::create_dir_all(dir).unwrap();
    }
    {
        use std::os::unix::fs::PermissionsExt;
        let docker = bin.join("docker");
        std::fs::write(&docker, FAKE_DOCKER).unwrap();
        std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "oc_panel_against_a_fake_docker", "--nocapture"])
        .current_dir(&home)
        .env(SANDBOX_VAR, "1")
        .env("PATH", &bin)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &config)
        .env("XDG_DATA_HOME", home.join("data"))
        .env_remove("SHELL")
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&base);
    assert!(
        output.status.success(),
        "the sandboxed OC test failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A fake `docker` that lists containers but fails every other command.
const FAILING_DOCKER: &str = r#"#!/bin/sh
case "$1" in
  "ps") echo '{"ID":"abc123def456","Names":"web","Image":"nginx:latest","State":"running","Status":"Up 2 hours"}' ;;
  *) echo "boom" >&2; exit 1 ;;
esac
"#;

/// The newest message's text, or empty.
fn newest(app: &App) -> String {
    app.messages
        .items
        .last()
        .map_or_else(String::new, |m| m.text.clone())
}

fn failing_body() {
    let home = std::env::var_os("HOME").expect("sandbox HOME");
    let root = Path::new(&home).join("work");
    std::fs::create_dir_all(&root).unwrap();
    let mut app = new_app(&root, "full");

    // The listing works; a confirmed stop fails and says why.
    app.run_action("oc.containers");
    assert!(screen(&mut app).contains("web"));
    app.on_key(key('x'));
    app.on_key(key('y'));
    let msg = newest(&app);
    assert!(msg.contains("boom") && msg.contains("stop"), "{msg}");

    // Inspect fails: the panel stays open and the error is reported.
    let before = app.messages.items.len();
    app.on_key(key('i'));
    assert_eq!(app.messages.items.len(), before + 1);
    assert!(newest(&app).contains("boom"), "{}", newest(&app));
    assert!(
        screen(&mut app).contains("web"),
        "a failed inspect keeps the panel"
    );

    // Switching to a kind whose listing fails reports the failure.
    app.on_key(code(KeyCode::Tab));
    assert!(newest(&app).contains("boom"), "{}", newest(&app));

    // A compose file with a failing engine: confirm, run, report.
    std::fs::write(root.join("compose.yaml"), "services: {}\n").unwrap();
    app.run_action("oc.compose_up");
    app.on_key(key('y'));
    assert!(newest(&app).contains("boom"), "{}", newest(&app));
}

#[test]
fn oc_panel_against_a_failing_docker() {
    if std::env::var_os(SANDBOX_VAR).is_some() {
        failing_body();
        return;
    }
    let base = std::env::temp_dir().join(format!("vix-oc-fail-sandbox-{}", std::process::id()));
    let bin = base.join("bin");
    let home = base.join("home");
    let config = base.join("config");
    for dir in [&bin, &home, &config] {
        std::fs::create_dir_all(dir).unwrap();
    }
    {
        use std::os::unix::fs::PermissionsExt;
        let docker = bin.join("docker");
        std::fs::write(&docker, FAILING_DOCKER).unwrap();
        std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "oc_panel_against_a_failing_docker",
            "--nocapture",
        ])
        .current_dir(&home)
        .env(SANDBOX_VAR, "1")
        .env("PATH", &bin)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &config)
        .env("XDG_DATA_HOME", home.join("data"))
        .env_remove("SHELL")
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&base);
    assert!(
        output.status.success(),
        "the sandboxed OC test failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
