//! Measure the UI-thread pause of a hot-exit backup of a large buffer
//! (T859): open a generated file of the given size, type one character so
//! the buffer is dirty, wait out the idle-pass interval, then time the
//! `App::poll_hot_exit` call that backs it up, how long until the backup
//! file is on disk, and how much resident memory the pass raised.
//!
//! Run with:
//! `cargo run --profile bench --example hot_exit_cost -p vix-tui -- [MiB]`
//!
//! Default: 64 MiB. Before T859 the whole backup (copy, JSON, write) ran
//! inside the timed call; since, the call only snapshots the rope and a
//! worker thread writes it (`docs/performance`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]
#![warn(clippy::pedantic)]

use std::fmt::Write as _;
use std::path::Path;
use std::time::{Duration, Instant};

use vix::app::App;
use vix::settings::Settings;
use vix_input::{KeyCode, KeyEvent, KeyModifiers};

fn mib(bytes: u64) -> f64 {
    #[allow(clippy::cast_precision_loss, reason = "a printed figure")]
    let b = bytes as f64;
    b / f64::from(1u32 << 20)
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

fn rss() -> u64 {
    vix_system_information_panel::resident_memory().unwrap_or(0)
}

/// Whether a finished backup (a `.json` file, not a temp) is in `dir`.
fn backup_written(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries
            .filter_map(Result::ok)
            .any(|e| e.path().extension().is_some_and(|x| x == "json"))
    })
}

fn main() {
    let size_mib: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(64);
    let bytes = size_mib << 20;
    let mut text = String::with_capacity(bytes + 200);
    let mut i = 0usize;
    while text.len() < bytes {
        writeln!(
            text,
            "line {i} of a large plain text file, nothing to parse"
        )
        .unwrap();
        i += 1;
    }
    let dir = std::env::temp_dir().join(format!("vix-hot-exit-cost-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("big.txt");
    std::fs::write(&file, &text).unwrap();
    drop(text);
    let backup = dir.join("backup");
    let mut settings = Settings::default();
    settings.editor_behavior.large_file_mb = 0;
    let mut app = App::new(dir.clone(), settings)
        .with_session_path(dir.join("session.toml"))
        .with_backup_dir(&backup);
    app.open_initial(&file);
    app.on_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    // The first idle pass is due at once, and a large buffer never backed
    // up is due too: this call backs the buffer up.
    let rss0 = rss();
    let start = Instant::now();
    app.poll_hot_exit();
    let paused = start.elapsed();
    let rss1 = rss();
    while !backup_written(&backup) {
        std::thread::sleep(Duration::from_millis(1));
    }
    let on_disk = start.elapsed();
    let rss2 = rss();
    println!(
        "{size_mib} MiB: UI thread paused {:.2} ms (resident +{:.1} MiB), backup on disk after {:.1} ms (resident +{:.1} MiB)",
        ms(paused),
        mib(rss1.saturating_sub(rss0)),
        ms(on_disk),
        mib(rss2.saturating_sub(rss0))
    );
    let _ = std::fs::remove_dir_all(&dir);
}
