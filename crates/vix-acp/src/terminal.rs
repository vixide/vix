//! A command an ACP agent asked to run (the `terminal/*` methods).
//!
//! This is the second place the crate starts a process (the first is
//! [`crate::process`]). The caller must have established that the workspace is
//! trusted **and** that the user approved this exact command line: nothing in
//! this module asks. There is no shell (`argv` only), no stdin, and the output
//! of stdout and stderr is merged into one buffer capped at a byte limit, so a
//! chatty command cannot grow memory without bound.
//!
//! Dropping a [`Terminal`] kills and reaps the command.
//!
//! Owning spec: `crates/vix-acp/spec/index.md`.

use std::io::{self, Read};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use crate::client::TerminalExit;

/// The most output bytes a terminal keeps when the agent names no limit (1 MiB).
pub const DEFAULT_OUTPUT_LIMIT: usize = 1024 * 1024;

/// The most output bytes a terminal keeps whatever the agent asks for (8 MiB).
pub const MAX_OUTPUT_LIMIT: usize = 8 * 1024 * 1024;

/// The output captured so far.
#[derive(Default)]
struct Captured {
    /// The newest bytes, at most `limit` of them.
    bytes: Vec<u8>,
    /// Whether older bytes were dropped to stay under `limit`.
    truncated: bool,
    /// How many chunks have arrived, so a viewer can skip unchanged output.
    chunks: u64,
}

/// A running (or finished) agent command.
pub struct Terminal {
    /// The command, kept to poll, kill and reap it.
    child: Child,
    /// Shared with the two reader threads.
    captured: Arc<Mutex<Captured>>,
    /// Set once the exit status has been observed.
    exit: Option<TerminalExit>,
}

/// Append `chunk` to `cap`, dropping the oldest bytes beyond `limit`.
fn push_capped(cap: &Mutex<Captured>, chunk: &[u8], limit: usize) {
    let Ok(mut c) = cap.lock() else {
        return;
    };
    c.bytes.extend_from_slice(chunk);
    c.chunks = c.chunks.wrapping_add(1);
    if c.bytes.len() > limit {
        let excess = c.bytes.len() - limit;
        c.bytes.drain(..excess);
        c.truncated = true;
    }
}

/// Read `src` to its end into `cap` on a detached thread.
fn pump(mut src: impl Read + Send + 'static, cap: Arc<Mutex<Captured>>, limit: usize) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match src.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => push_capped(&cap, &buf[..n], limit),
            }
        }
    });
}

impl Terminal {
    /// Start `argv` (program then arguments, no shell) in `cwd` with the extra
    /// environment `env`. `output_limit` bytes of output are kept (clamped to
    /// [`MAX_OUTPUT_LIMIT`]; `None` means [`DEFAULT_OUTPUT_LIMIT`]).
    ///
    /// # Errors
    /// An empty `argv`, or the OS refusing to start the program.
    pub fn spawn(
        argv: &[String],
        cwd: &Path,
        env: &[(String, String)],
        output_limit: Option<u64>,
    ) -> io::Result<Self> {
        let (exe, rest) = argv
            .split_first()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty command"))?;
        let limit = output_limit
            .map_or(DEFAULT_OUTPUT_LIMIT, |l| {
                usize::try_from(l).unwrap_or(MAX_OUTPUT_LIMIT)
            })
            .min(MAX_OUTPUT_LIMIT);
        let mut child = Command::new(exe)
            .args(rest)
            .envs(env.iter().map(|(k, v)| (k, v)))
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let captured = Arc::new(Mutex::new(Captured::default()));
        if let Some(out) = child.stdout.take() {
            pump(out, Arc::clone(&captured), limit);
        }
        if let Some(err) = child.stderr.take() {
            pump(err, Arc::clone(&captured), limit);
        }
        Ok(Self {
            child,
            captured,
            exit: None,
        })
    }

    /// The output kept so far and whether older output was dropped. A cut that
    /// lands inside a UTF-8 character is moved forward to the next boundary.
    #[must_use]
    pub fn output(&self) -> (String, bool) {
        let Ok(c) = self.captured.lock() else {
            return (String::new(), false);
        };
        let mut start = 0;
        if c.truncated {
            // 0b10xx_xxxx is a continuation byte: never begin on one.
            while c.bytes.get(start).is_some_and(|b| b & 0xC0 == 0x80) {
                start += 1;
            }
        }
        (
            String::from_utf8_lossy(&c.bytes[start..]).into_owned(),
            c.truncated,
        )
    }

    /// A counter that changes whenever new output arrives, so a display can
    /// skip copying [`Terminal::output`] when nothing is new.
    #[must_use]
    pub fn output_generation(&self) -> u64 {
        self.captured.lock().map_or(0, |c| c.chunks)
    }

    /// How the command ended, or `None` while it still runs.
    pub fn try_exit(&mut self) -> Option<TerminalExit> {
        if self.exit.is_none()
            && let Ok(Some(status)) = self.child.try_wait()
        {
            self.exit = Some(exit_of(status));
        }
        self.exit.clone()
    }

    /// Kill the command (the terminal and its output stay readable).
    pub fn kill(&mut self) {
        let _ = self.child.kill();
    }
}

/// Translate an OS exit status.
fn exit_of(status: std::process::ExitStatus) -> TerminalExit {
    #[cfg(unix)]
    let signal = {
        use std::os::unix::process::ExitStatusExt;
        status.signal().map(|n| format!("SIG{n}"))
    };
    #[cfg(not(unix))]
    let signal = None;
    TerminalExit {
        code: status.code().and_then(|c| u32::try_from(c).ok()),
        signal,
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(ToString::to_string).collect()
    }

    fn wait(t: &mut Terminal) -> TerminalExit {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(e) = t.try_exit() {
                // Give the reader threads a moment to drain the pipes.
                std::thread::sleep(Duration::from_millis(100));
                return e;
            }
            assert!(Instant::now() < deadline, "the command never exited");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn captures_output_and_exit_code_without_a_shell() {
        let dir = std::env::temp_dir();
        let mut t = Terminal::spawn(&argv(&["echo", "a;b $HOME"]), &dir, &[], None).unwrap();
        let exit = wait(&mut t);
        assert_eq!(exit.code, Some(0));
        // No shell: the metacharacters arrive as plain text.
        assert_eq!(t.output(), ("a;b $HOME\n".to_string(), false));
    }

    #[test]
    fn passes_the_environment_and_merges_stderr() {
        let dir = std::env::temp_dir();
        let env = [("VIX_T635".to_string(), "yes".to_string())];
        let mut t = Terminal::spawn(
            &argv(&["sh", "-c", "echo $VIX_T635; echo oops >&2; exit 3"]),
            &dir,
            &env,
            None,
        )
        .unwrap();
        assert_eq!(wait(&mut t).code, Some(3));
        let (out, _) = t.output();
        assert!(out.contains("yes") && out.contains("oops"), "{out}");
    }

    #[test]
    fn output_is_capped_keeping_the_newest_bytes() {
        let dir = std::env::temp_dir();
        let mut t = Terminal::spawn(
            &argv(&["sh", "-c", "printf 0123456789abcdefghij"]),
            &dir,
            &[],
            Some(5),
        )
        .unwrap();
        wait(&mut t);
        assert_eq!(t.output(), ("fghij".to_string(), true));
        assert!(t.output_generation() > 0, "output bumps the generation");
    }

    #[test]
    fn kill_ends_a_long_command_with_a_signal() {
        let dir = std::env::temp_dir();
        let mut t = Terminal::spawn(&argv(&["sleep", "30"]), &dir, &[], None).unwrap();
        assert!(t.try_exit().is_none());
        t.kill();
        let exit = wait(&mut t);
        assert!(exit.signal.is_some() || exit.code != Some(0), "{exit:?}");
    }

    #[test]
    fn a_missing_program_or_empty_command_is_an_error() {
        let dir = std::env::temp_dir();
        assert!(Terminal::spawn(&[], &dir, &[], None).is_err());
        assert!(Terminal::spawn(&argv(&["/nonexistent/vix-t635"]), &dir, &[], None).is_err());
    }
}
