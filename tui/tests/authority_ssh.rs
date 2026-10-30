//! T712: the App's explorer, open, save and git status work against an
//! installed `SshAuthority` whose `ssh` is a fake: a shell script, named by
//! explicit path, that runs the remote command locally the way `sshd` would.
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

use vix::app::App;
use vix::settings::Settings;
use vix_authority::ssh::{SshAuthority, SshOptions, SshTarget};

/// A fake `ssh` in `dir`: skip the options, the `--` and the host, then run
/// the remote command with `/bin/sh -c`, counting each call in `dir/calls`.
fn fake_ssh(dir: &Path) -> PathBuf {
    let calls = dir.join("calls");
    let script = format!(
        "#!/bin/sh\n\
         echo x >> '{}'\n\
         while [ $# -gt 0 ]; do\n\
           case $1 in --) shift; break ;; -o|-p|-l) shift 2 ;; *) shift ;; esac\n\
         done\n\
         shift\n\
         exec /bin/sh -c \"$*\"\n",
        calls.display()
    );
    let path = dir.join("fake-ssh");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn the_app_opens_edits_and_saves_through_an_ssh_authority() {
    let scratch =
        std::env::temp_dir().join(format!("vix-authority-ssh-app-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let proj = scratch.join("proj");
    std::fs::create_dir_all(proj.join("src")).unwrap();
    std::fs::write(proj.join("src/main.rs"), "fn main() {}\n").unwrap();
    let options = SshOptions::default()
        .with_program(fake_ssh(&scratch))
        .with_multiplex(false);
    let auth =
        SshAuthority::with_options(SshTarget::new("example.invalid").unwrap(), options).unwrap();
    let previous = vix_authority::install(Arc::new(auth));
    let calls = || std::fs::read_to_string(scratch.join("calls")).map_or(0, |s| s.lines().count());

    let mut app =
        App::new(proj.clone(), Settings::default()).with_session_path(scratch.join("session.toml"));
    let main = proj.join("src/main.rs");
    let before = calls();
    app.open_initial(&main);
    assert!(calls() > before, "the open went through ssh");
    let tab = app.editor.active_tab().unwrap();
    assert_eq!(tab.text(), "fn main() {}\n");

    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    assert!(app.editor.insert_str("// over ssh\n", area));
    let before = calls();
    app.run_action("file.save");
    assert!(calls() > before, "the save went through ssh");
    assert_eq!(
        std::fs::read_to_string(&main).unwrap(),
        "// over ssh\nfn main() {}\n"
    );

    vix_authority::install(previous);
    let _ = std::fs::remove_dir_all(&scratch);
}
