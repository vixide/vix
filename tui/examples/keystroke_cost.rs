//! Measure what a keystroke costs in a large buffer (T858): open a
//! generated file of the given size, put the cursor in its middle, type
//! characters through the real `App::on_key` path with a tick and a frame
//! into a `TestBackend` after each, and print the time per key, the
//! whole-buffer passes it made and how much resident memory grew.
//!
//! Run with:
//! `cargo run --profile bench --example keystroke_cost -p vix-tui -- [MiB] [keys] [rs|txt] [pause-ms] [git]`
//!
//! Defaults: 64 MiB, 5 keys, `.rs`, no pause. `pause-ms` waits that long
//! before each key (work deferred until typing pauses, such as the 250 ms
//! widest-line remeasure, then lands on the key); `git` commits the file in
//! a fresh repository first, so the change gutter has a base to diff.
//! `/usr/bin/sample keystroke_cost 8 -wait -mayDie` beside a run with many keys
//! finds where the time goes (`docs/performance`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]
#![warn(clippy::pedantic)]

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use ratatui::{Terminal, backend::TestBackend};
use vix::app::App;
use vix::editor_core::code::full_text_scans;
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

fn main() {
    let mut args = std::env::args().skip(1);
    let size_mib: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(64);
    let keys: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(5);
    let ext = args.next().unwrap_or_else(|| "rs".to_string());
    let pause_ms: u64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(0);
    let git = args.next().is_some_and(|a| a == "git");
    let bytes = size_mib << 20;
    let mut text = String::with_capacity(bytes + 200);
    let mut i = 0usize;
    while text.len() < bytes {
        write!(
            text,
            "/// Item {i}.\npub fn item_{i}(value: &str) -> usize {{\n    let n = value.len();\n    n + {i}\n}}\n\n"
        )
        .unwrap();
        i += 1;
    }
    let dir = std::env::temp_dir().join(format!("vix-keystroke-cost-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("big.{ext}"));
    std::fs::write(&file, &text).unwrap();
    let middle = text.chars().count() / 2;
    drop(text);
    if git {
        for cmd in [
            &["init", "-q"][..],
            &["add", "."],
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-qm",
                "x",
            ],
        ] {
            std::process::Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(cmd)
                .status()
                .unwrap();
        }
    }
    let mut settings = Settings::default();
    // No "open anyway?" question for the big file.
    settings.editor_behavior.large_file_mb = 0;
    let mut app = App::new(dir.clone(), settings)
        .with_session_path(dir.join("session.toml"))
        .with_backup_dir(dir.join("backup"));
    app.layout.editor = ratatui::layout::Rect::new(0, 0, 120, 40);
    let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();
    app.open_initial(&file);
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_cursor(middle);
    for _ in 0..3 {
        app.tick();
        term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
    }
    let rss0 = rss();
    let scans0 = full_text_scans();
    println!(
        "{size_mib} MiB .{ext}, resident before typing {:.1} MiB",
        mib(rss0)
    );
    for k in 0..keys {
        if pause_ms > 0 {
            std::thread::sleep(Duration::from_millis(pause_ms));
        }
        let c = char::from(b'a' + u8::try_from(k % 26).unwrap());
        let scans = full_text_scans();
        let start = Instant::now();
        app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        let handled = start.elapsed();
        app.tick();
        let ticked = start.elapsed();
        term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
        let total = start.elapsed();
        let now = rss();
        println!(
            "key {k}: {:.2} ms (on_key {:.2}, tick {:.2}, frame {:.2}), {} whole-buffer passes, resident +{:.1} MiB",
            ms(total),
            ms(handled),
            ms(ticked.saturating_sub(handled)),
            ms(total.saturating_sub(ticked)),
            full_text_scans() - scans,
            mib(now.saturating_sub(rss0))
        );
    }
    println!(
        "whole-buffer passes while typing: {}",
        full_text_scans() - scans0
    );
    let _ = std::fs::remove_dir_all(&dir);
}
