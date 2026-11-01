//! Starting `direnv`/`mise` (behind the `process` feature).

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long `direnv`/`mise` get before they are killed.
const TOOL_TIMEOUT: Duration = Duration::from_secs(5);

/// Run `program args` in `dir` with stdin closed, capturing stdout, killed
/// after [`TOOL_TIMEOUT`]. `None` on any failure.
pub(crate) fn run_tool(program: &str, args: &[&str], dir: &Path) -> Option<String> {
    let mut child = Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut stdout, &mut buf)
            .ok()
            .map(|_| buf)
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let text = reader.join().ok().flatten();
                return if status.success() { text } else { None };
            }
            Ok(None) if start.elapsed() < TOOL_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}
