//! The process-spawning half of the Prettier integration (T683), behind the
//! `process` feature so a minimal build never starts a child process.

use std::io::{Read, Write};
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use crate::prettier::PrettierError;

/// Run `binary --stdin-filepath <file>` on `input` and return the formatted
/// text. The working directory is `file`'s directory so config lookup
/// matches what the user gets on the command line.
///
/// # Errors
/// [`PrettierError::Io`] if it cannot be spawned, [`PrettierError::TimedOut`]
/// after `timeout`, [`PrettierError::Failed`] on a non-zero exit, and
/// [`PrettierError::BadOutput`] if the output is not UTF-8.
pub fn format(
    binary: &Path,
    file: &Path,
    input: &str,
    timeout: Duration,
) -> Result<String, PrettierError> {
    // Built by the workspace authority (T710): Prettier runs where the
    // file is.
    let auth = vix_authority::current();
    let mut spec = vix_authority::ProcessSpec::new(binary)
        .arg("--stdin-filepath")
        .arg(file);
    if let Some(dir) = file.parent().filter(|d| auth.is_dir(d)) {
        spec = spec.current_dir(dir);
    }
    let mut cmd = auth.command(&spec).map_err(PrettierError::Io)?;
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(PrettierError::Io)?;
    let mut stdin = child.stdin.take();
    let text = input.to_owned();
    // Feed stdin and drain the pipes on threads so a large buffer cannot
    // deadlock against a full pipe while we poll for the timeout.
    let writer = std::thread::spawn(move || {
        if let Some(s) = stdin.as_mut() {
            let _ = s.write_all(text.as_bytes());
        }
        drop(stdin);
    });
    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    let out = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = out_pipe.as_mut() {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    });
    let err = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = err_pipe.as_mut() {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait().map_err(PrettierError::Io)? {
            Some(s) => break s,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(PrettierError::TimedOut);
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    };
    let _ = writer.join();
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    if !status.success() {
        let msg = String::from_utf8_lossy(&stderr).trim().to_owned();
        return Err(PrettierError::Failed(msg));
    }
    String::from_utf8(stdout).map_err(|_| PrettierError::BadOutput)
}
