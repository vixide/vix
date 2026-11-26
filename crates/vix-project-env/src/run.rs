//! Starting `direnv`/`mise` (behind the `process` feature).
//!
//! This is the only file in the crate that starts a process (it is listed in
//! `tui/tests/spawn_sites.rs`, which audits spawn sites). Owning spec:
//! `crates/vix-project-env/spec/index.md`.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Run `program args` in `dir` with stdin closed, capturing stdout, killed
/// after `timeout`, with `env` added to the inherited environment. `None` on any failure.
pub(crate) fn run_tool(
    program: &str,
    args: &[&str],
    dir: &Path,
    timeout: Duration,
    env: &[(String, String)],
) -> Option<String> {
    let mut child = Command::new(program)
        .args(args)
        .envs(env.iter().map(|(k, v)| (k, v)))
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

/// Replace this process with itself (same executable and arguments), with
/// `PATH` set to `path` and `done_var` set to `1` (see
/// `login_path::reexec_with_login_shell_path`). Returns only if the exec
/// failed, and then changes nothing.
#[cfg(unix)]
pub(crate) fn reexec_with_path(path: &str, done_var: &str) {
    use std::os::unix::process::CommandExt as _;
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let _ = Command::new(exe)
        .args(std::env::args_os().skip(1))
        .env("PATH", path)
        .env(done_var, "1")
        .exec();
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn sh(script: &str, timeout: Duration, env: &[(String, String)]) -> Option<String> {
        run_tool("/bin/sh", &["-c", script], Path::new("/"), timeout, env)
    }

    #[test]
    fn a_successful_tool_returns_its_stdout() {
        let long = Duration::from_secs(10);
        assert_eq!(sh("printf hello", long, &[]).as_deref(), Some("hello"));
        // The extra environment reaches the child.
        let env = [("VIX_RUN_TEST".to_string(), "42".to_string())];
        assert_eq!(
            sh("printf %s \"$VIX_RUN_TEST\"", long, &env).as_deref(),
            Some("42")
        );
        // It runs in `dir`.
        assert_eq!(sh("pwd", long, &[]).as_deref(), Some("/\n"));
    }

    #[test]
    fn a_failing_or_missing_tool_yields_nothing() {
        let long = Duration::from_secs(10);
        assert_eq!(sh("printf partial; exit 3", long, &[]), None);
        assert_eq!(
            run_tool("/nonexistent/tool", &[], Path::new("/"), long, &[]),
            None
        );
    }

    #[test]
    fn a_tool_past_its_timeout_is_killed() {
        // `exec` so the kill reaches `sleep` itself and the pipe closes.
        assert_eq!(sh("exec sleep 30", Duration::from_millis(50), &[]), None);
    }
}
