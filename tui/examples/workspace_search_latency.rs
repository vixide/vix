//! Measure workspace search latency through the real key path (T869):
//! generate a tree of small files, open Search in Workspace and, for each
//! keystroke, record how long `App::on_key` took to return, how long until
//! the first hits were posted and how long until the search completed.
//!
//! Run with:
//! `cargo run --profile bench --example workspace_search_latency -p vix-tui -- [files] [samples]`
//!
//! Defaults: 10,000 files, 30 samples. A tenth of the files hold `needle`.
//! "Cold" samples each open a fresh App (no scan memo: every file is read;
//! the operating system's file cache stays warm, as it is for a project
//! being worked on) and type `ne`; "warm" samples type `l` after `need`
//! in one App, so the scan memo skips files the last scan found no
//! candidate in, as typing a query further does. The figures are printed
//! as percentiles (`docs/performance`, "Workspace search latency").
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]
#![warn(clippy::pedantic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use vix::app::App;
use vix::settings::Settings;
use vix_input::{KeyCode, KeyEvent, KeyModifiers};

/// `n` small files under a fresh temp directory, four top folders deep.
fn tree(n: usize) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vix-search-latency-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    for i in 0..n {
        let sub = dir
            .join(["alpha", "beta", "gamma", "delta"][i % 4])
            .join(format!("mod{}", i / 100));
        if i < 4 || i % 100 < 4 {
            fs::create_dir_all(&sub).unwrap();
        }
        let body = if i % 10 == 0 {
            format!("fn needle_{i}() -> usize {{ compute({i}) }}\n")
        } else {
            format!("fn other_{i}() -> usize {{ {i} }}\n")
        };
        fs::write(sub.join(format!("item{i}.rs")), body).unwrap();
    }
    dir
}

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

/// An App over `root` with the search panel open and the whole index in.
fn open(root: &Path) -> App {
    let mut app = App::new(root.to_path_buf(), Settings::default());
    app.run_action("search.workspace");
    while app.file_index_busy() {
        std::thread::sleep(Duration::from_millis(2));
        app.tick();
    }
    app
}

/// One keystroke's handler, first-result and completion times.
fn sample(app: &mut App, k: KeyEvent) -> [Duration; 3] {
    let start = Instant::now();
    app.on_key(k);
    let handler = start.elapsed();
    app.wait_for_workspace_search();
    let t = app.workspace_search_timing().expect("a search completed");
    [handler, t.first_result, t.complete]
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

/// p50, p90, p99 and max of `v`, in milliseconds.
fn line(label: &str, mut v: Vec<Duration>) -> String {
    v.sort_unstable();
    let at = |q: usize| v[(v.len() - 1) * q / 100];
    format!(
        "{label:<14} p50 {:>8.3}  p90 {:>8.3}  p99 {:>8.3}  max {:>8.3} ms",
        ms(at(50)),
        ms(at(90)),
        ms(at(99)),
        ms(*v.last().unwrap())
    )
}

fn report(name: &str, samples: &[[Duration; 3]]) {
    println!("{name} ({} samples):", samples.len());
    for (i, label) in ["key-to-return", "first result", "complete"]
        .iter()
        .enumerate()
    {
        println!("  {}", line(label, samples.iter().map(|s| s[i]).collect()));
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let files: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(10_000);
    let samples: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(30);
    let root = tree(files);

    let mut cold = Vec::new();
    for _ in 0..samples {
        let mut app = open(&root);
        app.on_key(key('n'));
        cold.push(sample(&mut app, key('e')));
        assert_eq!(app.workspace_search_hits().len(), (files / 10).min(5_000));
    }

    let mut app = open(&root);
    for c in "need".chars() {
        app.on_key(key(c));
    }
    app.wait_for_workspace_search();
    let mut warm = Vec::new();
    for _ in 0..samples {
        warm.push(sample(&mut app, key('l')));
        let back = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        app.on_key(back);
        app.wait_for_workspace_search();
    }

    println!("workspace search, {files} files");
    report("cold (fresh App, `ne`)", &cold);
    report("warm (`need` + `l`)", &warm);
    let _ = fs::remove_dir_all(&root);
}
