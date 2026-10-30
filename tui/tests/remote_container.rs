//! T718: container workspaces end to end against a fake engine (a shell
//! script named by explicit path that skips the engine's own options and
//! runs the `/bin/sh` meant for the container locally) and a fake
//! `devcontainer` CLI. **File → Open Remote…** with a `docker://` name, and
//! **OC → Reopen in Dev Container**: confirm, `devcontainer up`, connect,
//! trusted. No real engine is started.
//!
//! The authority slot is process-wide, so this lives in its own test binary
//! and runs as one `#[test]`.
#![cfg(all(unix, feature = "oc"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use vix::app::App;
use vix::settings::Settings;
use vix_authority::container::ExecOptions;
use vix_input::{KeyCode, KeyEvent};

fn script(path: &Path, body: &str) -> PathBuf {
    std::fs::write(path, format!("#!/bin/sh\n{body}")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.to_path_buf()
}

/// A fake engine: log the arguments to `dir/engine-argv`, then run what
/// follows `/bin/sh` here.
fn fake_engine(dir: &Path) -> PathBuf {
    script(
        &dir.join("fake-engine"),
        &format!(
            "for a in \"$@\"; do printf '%s\\n' \"$a\" >> '{argv}'; done\n\
             while [ $# -gt 0 ] && [ \"$1\" != /bin/sh ]; do shift; done\n\
             exec \"$@\"\n",
            argv = dir.join("engine-argv").display(),
        ),
    )
}

/// A fake `devcontainer`: log its arguments, chatter on stderr, and report
/// container `abc123` with the workspace at `folder` (no user: the
/// configuration's `remoteUser` is the fallback).
fn fake_devcontainer(dir: &Path, folder: &Path) -> PathBuf {
    script(
        &dir.join("fake-devcontainer"),
        &format!(
            "for a in \"$@\"; do printf '%s\\n' \"$a\" >> '{argv}'; done\n\
             echo '[1 ms] Start: Run: docker build' >&2\n\
             printf '%s\\n' '{{\"outcome\":\"success\",\"containerId\":\"abc123\",\"remoteWorkspaceFolder\":\"{folder}\"}}'\n",
            argv = dir.join("dc-argv").display(),
            folder = folder.display(),
        ),
    )
}

/// Tick until no remote connect is running (at most twenty seconds).
fn settle(app: &mut App) {
    let start = Instant::now();
    while app.remote_busy() && start.elapsed() < Duration::from_secs(20) {
        app.tick();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!app.remote_busy(), "the connect finished");
}

fn lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn containers_open_from_the_dialog_and_from_a_dev_container() {
    let scratch = std::env::temp_dir().join(format!("vix-remote-ctr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let proj = scratch.join("proj");
    let inside = scratch.join("inside");
    std::fs::create_dir_all(proj.join(".devcontainer")).unwrap();
    std::fs::create_dir_all(&inside).unwrap();
    let scratch = scratch.canonicalize().unwrap();
    let proj = proj.canonicalize().unwrap();
    let inside = inside.canonicalize().unwrap();
    std::fs::write(inside.join("a.txt"), "inside\n").unwrap();
    std::fs::write(
        proj.join(".devcontainer/devcontainer.json"),
        "{\n  // JSONC\n  \"name\": \"Demo\",\n  \"remoteUser\": \"dev\",\n}\n",
    )
    .unwrap();
    let engine = fake_engine(&scratch);
    let exec = ExecOptions::default().with_program(&engine);

    // File → Open Remote… with a container name.
    let mut app = App::new(proj.clone(), Settings::default())
        .with_session_path(scratch.join("session.toml"))
        .with_exec_options(exec.clone());
    app.open_remote(&format!("docker://web{}", inside.display()));
    assert!(app.remote_busy());
    settle(&mut app);
    assert_eq!(app.remote_host(), Some("docker:web"));
    assert_eq!(app.root, inside);
    assert_eq!(vix_authority::current().name(), "docker");
    assert!(
        app.remote_untrusted(),
        "a container workspace starts untrusted"
    );
    let argv = lines(&scratch.join("engine-argv"));
    assert_eq!(&argv[..3], ["exec", "-i", "web"], "{argv:?}");
    // A bad name never reaches the engine.
    app.open_remote("docker://-it/x");
    assert!(!app.remote_busy());

    // OC → Reopen in Dev Container: asks, builds, connects, trusted.
    let mut settings = Settings::default();
    settings.oc_engine = "docker".into();
    let mut app = App::new(proj.clone(), settings)
        .with_session_path(scratch.join("session2.toml"))
        .with_exec_options(exec)
        .with_devcontainer_cli(fake_devcontainer(&scratch, &inside));
    app.run_action("oc.devcontainer_reopen");
    assert!(!app.remote_busy(), "it asks before building");
    assert!(!scratch.join("dc-argv").exists(), "nothing ran yet");
    std::thread::sleep(Duration::from_millis(300));
    app.on_key(KeyEvent::from(KeyCode::Char('y')));
    assert!(app.remote_busy());
    settle(&mut app);
    assert_eq!(
        lines(&scratch.join("dc-argv")),
        ["up", "--workspace-folder", &proj.display().to_string()]
    );
    assert_eq!(app.remote_host(), Some("docker:dev@abc123"));
    assert_eq!(app.root, inside);
    assert!(!app.remote_untrusted(), "the user chose to run its code");
    let argv = lines(&scratch.join("engine-argv"));
    assert!(
        argv.windows(3).any(|w| w == ["-u", "dev", "abc123"]),
        "{argv:?}"
    );
    // Already in a container: no second one, even when answered.
    app.run_action("oc.devcontainer_reopen");
    std::thread::sleep(Duration::from_millis(300));
    app.on_key(KeyEvent::from(KeyCode::Char('y')));
    assert!(!app.remote_busy());
    assert_eq!(lines(&scratch.join("dc-argv")).len(), 3, "ran once");

    // No configuration: nothing to reopen.
    let bare = scratch.join("bare");
    std::fs::create_dir_all(&bare).unwrap();
    vix_authority::install(std::sync::Arc::new(vix_authority::LocalAuthority));
    let mut app = App::new(bare, Settings::default())
        .with_session_path(scratch.join("session3.toml"))
        .with_devcontainer_cli(scratch.join("fake-devcontainer"));
    app.run_action("oc.devcontainer_reopen");
    std::thread::sleep(Duration::from_millis(300));
    app.on_key(KeyEvent::from(KeyCode::Char('y')));
    assert!(!app.remote_busy());
    assert_eq!(lines(&scratch.join("dc-argv")).len(), 3, "never ran");
    let _ = std::fs::remove_dir_all(&scratch);
}
