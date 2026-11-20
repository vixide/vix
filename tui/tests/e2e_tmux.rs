//! End-to-end tests of the real `vix` binary in a real terminal (tmux).
//!
//! Every other test drives `App` in-process on a `TestBackend`, so the
//! binary's own terminal setup, event loop, input decoding and teardown
//! (`tui/src/main.rs`) never run under test, and neither do bugs that only
//! show up in a real terminal. These start `vix` in a detached tmux session
//! on a private server socket (`tmux -L`, so a person's own tmux sessions are
//! never touched) with a throwaway `HOME` and config, send keys with
//! `send-keys`, and read the screen with `capture-pane`.
//!
//! Skipped (with a message) when `tmux` is not installed. Unix only: tmux
//! does not run on Windows.
#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests fail loudly on a broken fixture"
)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

/// One tmux server and session running `vix`, torn down on drop.
struct Session {
    socket: String,
    home: PathBuf,
}

impl Session {
    /// Start `vix args…` in a 120×40 session, in `dir`, or `None` when tmux
    /// is not installed.
    fn start(name: &str, dir: &Path, args: &[&str]) -> Option<Session> {
        if Command::new("tmux").arg("-V").output().is_err() {
            eprintln!("tmux not installed; skipping");
            return None;
        }
        let socket = format!("vix-e2e-{name}-{}", std::process::id());
        let home = std::env::temp_dir().join(format!("{socket}-home"));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("config")).unwrap();
        let mut cmd = vec![env!("CARGO_BIN_EXE_vix").to_string()];
        cmd.extend(args.iter().map(|a| (*a).to_string()));
        let session = Session { socket, home };
        let env = |k: &str, v: &Path| format!("{k}={}", v.display());
        let status = Command::new("tmux")
            .args([
                "-L",
                &session.socket,
                "-f",
                "/dev/null",
                "new-session",
                "-d",
            ])
            .args(["-x", "120", "-y", "40", "-s", "e2e", "-c"])
            .arg(dir)
            .args(["-e", &env("HOME", &session.home)])
            .args(["-e", &env("XDG_CONFIG_HOME", &session.home.join("config"))])
            .args(["-e", &env("XDG_DATA_HOME", &session.home.join("data"))])
            .args(["-e", "TERM=xterm-256color"])
            .args(cmd)
            .status()
            .unwrap();
        assert!(status.success(), "tmux new-session failed");
        Some(session)
    }

    fn tmux(&self, args: &[&str]) -> Output {
        Command::new("tmux")
            .args(["-L", &self.socket])
            .args(args)
            .output()
            .unwrap()
    }

    /// The visible screen, one line per row.
    fn screen(&self) -> String {
        String::from_utf8_lossy(&self.tmux(&["capture-pane", "-p", "-t", "e2e"]).stdout)
            .into_owned()
    }

    /// Send tmux key names (`"C-s"`, `"Escape"`) or literal text.
    fn keys(&self, keys: &[&str]) {
        let mut args = vec!["send-keys", "-t", "e2e"];
        args.extend_from_slice(keys);
        assert!(self.tmux(&args).status.success());
    }

    fn type_text(&self, text: &str) {
        assert!(
            self.tmux(&["send-keys", "-t", "e2e", "-l", text])
                .status
                .success()
        );
    }

    /// Wait until the screen contains `text`, or fail showing the screen.
    fn wait_for(&self, text: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let screen = self.screen();
            if screen.contains(text) {
                return screen;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {text:?}; screen:\n{screen}"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Whether the session (so `vix`) has ended.
    fn ended(&self) -> bool {
        !self.tmux(&["has-session", "-t", "e2e"]).status.success()
    }

    fn wait_ended(&self) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !self.ended() {
            assert!(
                Instant::now() < deadline,
                "vix did not exit; screen:\n{}",
                self.screen()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.tmux(&["kill-server"]);
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vix-e2e-ws-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Open a file, see it on screen, type, save with Ctrl+S, quit with Ctrl+Q:
/// the text is on disk and the terminal is given back.
#[test]
fn edit_save_and_quit_in_a_real_terminal() {
    let dir = workspace("edit");
    std::fs::write(dir.join("notes.txt"), "first line\n").unwrap();
    let Some(s) = Session::start("edit", &dir, &["notes.txt"]) else {
        return;
    };
    let screen = s.wait_for("first line");
    assert!(screen.contains("File"), "the menu bar is drawn:\n{screen}");
    // The first-run welcome may be on top; Esc dismisses it.
    s.keys(&["Escape"]);
    s.keys(&["C-End"]);
    s.type_text("typed in tmux");
    s.wait_for("typed in tmux");
    s.keys(&["C-s"]);
    let deadline = Instant::now() + Duration::from_secs(20);
    while !std::fs::read_to_string(dir.join("notes.txt"))
        .unwrap()
        .contains("typed in tmux")
    {
        assert!(
            Instant::now() < deadline,
            "not saved; screen:\n{}",
            s.screen()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    s.keys(&["C-q"]);
    s.wait_ended();
    let _ = std::fs::remove_dir_all(&dir);
}

/// `vix some-folder` opens that folder as the workspace: its files are in the
/// explorer and nothing reports a failed open; Ctrl+Q then quits.
#[test]
fn a_folder_argument_opens_that_folder_as_the_workspace() {
    let dir = workspace("folder");
    std::fs::create_dir(dir.join("project")).unwrap();
    std::fs::write(dir.join("project").join("proj_file.txt"), "x\n").unwrap();
    let Some(s) = Session::start("folder", &dir, &["project"]) else {
        return;
    };
    let screen = s.wait_for("proj_file.txt");
    assert!(!screen.contains("Open failed"), "{screen}");
    s.keys(&["Escape"]);
    s.keys(&["C-q"]);
    s.wait_ended();
    let _ = std::fs::remove_dir_all(&dir);
}
