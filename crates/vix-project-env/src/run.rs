//! Starting `direnv`/`mise` (behind the `process` feature).
//!
//! This is the only file in the crate that starts a process (it is listed in
//! `tui/tests/spawn_sites.rs`, which audits spawn sites). Owning spec:
//! `crates/vix-project-env/spec/index.md`.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Run `program args` in `dir` with stdin closed, capturing stdout, killed
/// after `timeout`. `None` on any failure.
pub(crate) fn run_tool(
    program: &str,
    args: &[&str],
    dir: &Path,
    timeout: Duration,
) -> Option<String> {
    let mut child = Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    // Drain stdout on a thread while we poll for exit: otherwise a tool
    // printing more than a pipe buffer would block on write and look hung.
    let reader = std::thread::spawn(move || {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut stdout, &mut buf)
            .ok()
            .map(|_| buf)
    });
    let start = Instant::now();
    // No `wait_timeout` in std, so poll `try_wait` at a short interval.
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let text = reader.join().ok().flatten();
                return if status.success() { text } else { None };
            }
            Ok(None) if start.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(10));
            }
            // Timed out (or `try_wait` failed): kill, reap, and give up. The
            // reader thread ends when the pipe closes with the child.
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}
