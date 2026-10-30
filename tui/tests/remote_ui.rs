//! T713/T714: **File → Open Remote…** end to end against a fake `ssh` (a
//! shell script named by explicit path that runs the remote command locally,
//! as `sshd` would): connect in the background, the status-bar host, the
//! trust gate, conflict-checked saves, a lost connection that keeps the
//! buffer, reconnect, and no secret from the settings ever on `ssh`'s argv.
//! No real host is contacted.
//!
//! The authority slot is process-wide, so this lives in its own test binary
//! and runs as one `#[test]`.
#![cfg(all(unix, feature = "tasks"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use vix::app::App;
use vix::settings::Settings;
use vix_authority::ssh::SshOptions;
use vix_input::{KeyCode, KeyEvent};

/// Secrets planted in the settings; none may reach `ssh`'s argv.
const SECRETS: &[&str] = &["hunter2-ai-key-cmd", "hunter2-db-password-cmd"];

/// A fake `ssh` in `dir`: log every argument to `dir/argv`, fail like a
/// dropped connection (status 255) while `dir/down` exists, else skip the
/// options and host and run the remote command with `/bin/sh -c`.
fn fake_ssh(dir: &Path) -> PathBuf {
    let script = format!(
        "#!/bin/sh\n\
         for a in \"$@\"; do printf '%s\\n' \"$a\" >> '{argv}'; done\n\
         if [ -e '{down}' ]; then echo 'ssh: connect to host: Connection refused' >&2; exit 255; fi\n\
         while [ $# -gt 0 ]; do\n\
           case $1 in --) shift; break ;; -o|-p|-l) shift 2 ;; *) shift ;; esac\n\
         done\n\
         shift\n\
         exec /bin/sh -c \"$*\"\n",
        argv = dir.join("argv").display(),
        down = dir.join("down").display(),
    );
    let path = dir.join("fake-ssh");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// Tick until no remote connect is running (at most ten seconds).
fn settle(app: &mut App) {
    let start = Instant::now();
    while app.remote_busy() && start.elapsed() < Duration::from_secs(10) {
        app.tick();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!app.remote_busy(), "the connect finished");
}

fn edit(app: &mut App, text: &str) {
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    assert!(app.editor.insert_str(text, area));
}

#[test]
fn open_remote_connects_saves_survives_a_lost_connection_and_reconnects() {
    let scratch = std::env::temp_dir().join(format!("vix-remote-ui-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let proj = scratch.join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    let scratch = scratch.canonicalize().unwrap();
    let proj = proj.canonicalize().unwrap();
    let main = proj.join("main.rs");
    std::fs::write(&main, "fn main() {}\n").unwrap();

    let mut settings = Settings::default();
    settings.ai_api_key_command = format!("echo {}", SECRETS[0]);
    settings.db_connections.push(vix::settings::Connection {
        password_command: format!("echo {}", SECRETS[1]),
        ssh_host: "example.invalid".into(),
        ..Default::default()
    });

    let options = SshOptions::default()
        .with_program(fake_ssh(&scratch))
        .with_multiplex(false);
    let mut app = App::new(scratch.clone(), settings)
        .with_session_path(scratch.join("session.toml"))
        .with_ssh_options(options);

    // Open: connects in the background, then the folder is the workspace.
    app.open_remote(&format!("ssh://me@example.invalid{}", proj.display()));
    assert!(app.remote_busy());
    settle(&mut app);
    assert_eq!(app.remote_host(), Some("me@example.invalid"));
    assert_eq!(app.root, proj);
    assert_eq!(vix_authority::current().name(), "ssh");
    assert!(app.remote_untrusted(), "a new remote starts untrusted");

    // Edit and save through ssh.
    app.open_initial(&main);
    edit(&mut app, "// a\n");
    app.run_action("file.save");
    assert_eq!(
        std::fs::read_to_string(&main).unwrap(),
        "// a\nfn main() {}\n"
    );

    // Someone else changes the file: the save asks instead of overwriting,
    // and `o` overwrites.
    std::fs::write(&main, "changed elsewhere, and longer\n").unwrap();
    edit(&mut app, "// b\n");
    app.run_action("file.save");
    assert_eq!(
        std::fs::read_to_string(&main).unwrap(),
        "changed elsewhere, and longer\n"
    );
    app.on_key(KeyEvent::from(KeyCode::Char('o')));
    assert_eq!(
        std::fs::read_to_string(&main).unwrap(),
        "// a\n// b\nfn main() {}\n"
    );

    // The connection drops: the save fails, the buffer stays dirty, and
    // later saves don't even try until a reconnect.
    std::fs::write(scratch.join("down"), "").unwrap();
    edit(&mut app, "// c\n");
    app.run_action("file.save");
    assert!(app.remote_connection_lost());
    assert!(app.editor.active_tab().unwrap().dirty);
    let calls = || std::fs::read_to_string(scratch.join("argv")).unwrap_or_default();
    let before = calls();
    app.run_action("file.save");
    assert_eq!(calls(), before, "no ssh while the connection is lost");
    assert!(app.editor.active_tab().unwrap().dirty);

    // Reconnect, then the edit saves.
    std::fs::remove_file(scratch.join("down")).unwrap();
    app.run_action("file.reconnect_remote");
    settle(&mut app);
    assert!(!app.remote_connection_lost());
    app.run_action("file.save");
    assert!(!app.editor.active_tab().unwrap().dirty);
    assert_eq!(
        std::fs::read_to_string(&main).unwrap(),
        "// a\n// b\n// c\nfn main() {}\n"
    );

    // T714: no secret from the settings ever reached ssh's argv, every call
    // ran in BatchMode, and host-key checking was never overridden.
    let argv = calls();
    assert!(argv.contains("BatchMode=yes"));
    for secret in SECRETS {
        assert!(!argv.contains(secret), "{secret} leaked onto ssh's argv");
    }
    assert!(!argv.contains("StrictHostKeyChecking"));
    assert!(!argv.contains("UserKnownHostsFile"));
    assert!(!argv.contains("BatchMode=no"), "the dialog never prompts");

    vix_authority::install(Arc::new(vix_authority::LocalAuthority));
    let _ = std::fs::remove_dir_all(&scratch);
}
