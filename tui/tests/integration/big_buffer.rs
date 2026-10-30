//! Large buffers (T749): an idle frame does no whole-buffer work, and opening
//! a file costs about its own size in memory, not a multiple of it.
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use std::io::Write as _;

use crate::integration::common::*;

/// `n` log lines of about 90 bytes each.
fn write_log(path: &Path, n: usize) {
    let mut out = std::io::BufWriter::new(fs::File::create(path).unwrap());
    for i in 0..n {
        writeln!(
            out,
            "2026-10-30T12:00:{:02}Z INFO worker={} request id={i} path=/api/v1/items/{} status=200",
            i % 60,
            i % 16,
            i * 7
        )
        .unwrap();
    }
}

/// Draw one frame, as the event loop does after every tick.
fn draw(app: &mut App) {
    use ratatui::{Terminal, backend::TestBackend};
    let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();
    term.draw(|f| vix::ui::draw(app, f)).unwrap();
}

/// The event loop with no input: tick, then draw. Before T749 every frame
/// copied the whole buffer (to size the horizontal scrollbar), which on a
/// 100 MB file was ~7.5% of a core with nothing happening.
#[test]
fn idle_frames_make_no_whole_buffer_pass() {
    use vix::editor_core::code::full_text_scans;
    let dir = unique_dir("t749-idle");
    let path = dir.join("server.log");
    write_log(&path, 5_000);
    let mut app = app_at(&dir);
    app.open_initial(&path);
    assert!(app.editor.active_tab().is_some_and(|t| t.path.is_some()));
    // The first frames may measure the buffer once.
    for _ in 0..2 {
        app.tick();
        draw(&mut app);
    }
    let before = full_text_scans();
    for _ in 0..10 {
        app.tick();
        draw(&mut app);
    }
    assert_eq!(
        full_text_scans(),
        before,
        "an idle frame read the whole buffer"
    );

    // An edit is measured again, once.
    app.on_key(key('x'));
    draw(&mut app);
    let after_edit = full_text_scans();
    for _ in 0..5 {
        app.tick();
        draw(&mut app);
    }
    assert_eq!(full_text_scans(), after_edit);
    let _ = fs::remove_dir_all(&dir);
}

/// This process's resident memory in KiB: the peak (`VmHWM`) where Linux
/// reports it, otherwise the current figure from `ps`. On macOS the two
/// agree here: the allocator keeps freed large blocks resident, which is
/// why transient copies of the text showed up as memory at all.
#[cfg(unix)]
fn resident_kib() -> u64 {
    if let Ok(status) = fs::read_to_string("/proc/self/status")
        && let Some(line) = status.lines().find(|l| l.starts_with("VmHWM:"))
    {
        return line
            .split_whitespace()
            .nth(1)
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
    }
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .unwrap_or(0)
}

const CHILD_ENV: &str = "VIX_T749_MEMORY_CHILD";
const FILE_MB: usize = 20;

/// Run in a process of its own (see the test below): open a ~20 MB log,
/// draw and tick a few frames, and print how much the resident memory grew.
#[cfg(unix)]
#[test]
#[ignore = "run as a child process by opening_a_large_file_costs_about_its_size"]
fn memory_child() {
    if std::env::var_os(CHILD_ENV).is_none() {
        return;
    }
    let dir = unique_dir("t749-memory");
    let path = dir.join("big.log");
    write_log(&path, FILE_MB * 1_000_000 / 90);
    let size = fs::metadata(&path).unwrap().len();
    let mut app = app_at(&dir);
    draw(&mut app);
    let before = resident_kib();
    app.open_initial(&path);
    for _ in 0..5 {
        app.tick();
        draw(&mut app);
    }
    let after = resident_kib();
    assert!(app.editor.active_tab().is_some_and(|t| t.path.is_some()));
    println!("T749 size={size} grew_kib={}", after.saturating_sub(before));
    let _ = fs::remove_dir_all(&dir);
}

/// Opening a file holds its text once (the rope, about the file's size),
/// not several transient copies of it. Before T749 a 100 MB file made Vix
/// ~1.1 GiB resident; the budget here is three times the file's size.
/// Measured in a child process so no other test's allocations count.
#[cfg(unix)]
#[test]
fn opening_a_large_file_costs_about_its_size() {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "integration::big_buffer::memory_child",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ENV, "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let line = stdout
        .lines()
        // The harness may print the test's name on the same line.
        .find_map(|l| l.find("T749 ").map(|i| &l[i..]))
        .unwrap_or_else(|| panic!("no measurement in {stdout}"));
    let field = |name: &str| -> u64 {
        line.split_whitespace()
            .find_map(|w| w.strip_prefix(name))
            .and_then(|v| v.parse().ok())
            .unwrap()
    };
    let size_kib = field("size=") / 1024;
    let grew = field("grew_kib=");
    assert!(
        grew < 3 * size_kib,
        "opening a {size_kib} KiB file grew resident memory by {grew} KiB"
    );
}
