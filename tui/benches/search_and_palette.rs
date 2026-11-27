//! Benchmarks for the three per-keystroke search paths.
//!
//! All three run on *every* character typed into their input box, over the
//! whole corpus: the find bar re-scans the buffer, the command palette
//! re-filters the file index, and workspace search re-scans every file under
//! the root. A regression here is felt as typing lag, which is why they are
//! benchmarked rather than merely tested.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use criterion::{BenchmarkId, Criterion, criterion_group};
use regex::Regex;
use std::fs;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use vix::app::App;
use vix::find_panel;
use vix::palette::{fuzzy_match, fuzzy_score};
use vix::settings::Settings;
use vix_input::{KeyCode, KeyEvent, KeyModifiers};

/// A buffer with `lines` lines and a match roughly every tenth line.
fn haystack(lines: usize) -> String {
    (0..lines)
        .map(|i| {
            if i % 10 == 0 {
                format!("let needle_{i} = compute(value, {i});\n")
            } else {
                format!("    let other_{i} = value + {i};\n")
            }
        })
        .collect()
}

/// A file index shaped like a real project's: nested paths, mixed extensions.
fn paths(n: usize) -> Vec<String> {
    (0..n)
        .map(|i| {
            format!(
                "crates/vix-{}/src/{}{}.rs",
                ["editor", "menu", "palette", "db", "org"][i % 5],
                ["lib", "mod", "state", "render", "parse"][i % 5],
                i
            )
        })
        .collect()
}

fn bench_find(c: &mut Criterion) {
    let mut group = c.benchmark_group("find/matches");
    // Built once: the find bar compiles its pattern when the query changes, not
    // per match, so compiling here would measure the wrong thing.
    let plain = Regex::new("needle").expect("valid regex");
    let complex = Regex::new(r"let\s+\w+_\d+\s*=").expect("valid regex");
    for lines in [1_000_usize, 20_000] {
        let text = haystack(lines);
        group.bench_with_input(BenchmarkId::new("plain", lines), &text, |b, t| {
            b.iter(|| black_box(find_panel::matches(black_box(t), &plain)));
        });
        group.bench_with_input(BenchmarkId::new("regex", lines), &text, |b, t| {
            b.iter(|| black_box(find_panel::matches(black_box(t), &complex)));
        });
    }
    group.finish();
}

/// `n` small source files (a match in roughly one in ten) under a fresh temp
/// directory, nested a few directories deep like a real project so the
/// workspace walk isn't just one flat listing. Real files on disk, not an
/// in-memory fixture — `run_workspace_search` reads unopened files from disk.
fn workspace_tree(n: usize) -> PathBuf {
    let dir = PathBuf::from("/tmp").join(format!("vix-bench-workspace-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    for i in 0..n {
        let sub = dir
            .join(["alpha", "beta", "gamma", "delta"][i % 4])
            .join(format!("mod{}", i / 100));
        fs::create_dir_all(&sub).expect("create fixture subdirectory");
        let body = if i % 10 == 0 {
            format!("fn needle_{i}() -> usize {{ compute({i}) }}\n")
        } else {
            format!("fn other_{i}() -> usize {{ {i} }}\n")
        };
        fs::write(sub.join(format!("item{i}.rs")), body).expect("write fixture file");
    }
    dir
}

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

fn backspace() -> KeyEvent {
    KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)
}

/// Plan T006's "workspace search over a generated 10k-file tree". The file
/// index (`App::build_file_index`, an `ignore::WalkBuilder` walk) is built
/// once in setup, matching `bench_find`/`bench_palette` below: what's timed
/// is the per-keystroke rescan (`run_workspace_search`), not the one-time
/// index build. Each iteration types one character (completing the query,
/// triggering a scan) then backspaces it (reverting to the baseline query,
/// triggering a second scan) — symmetric, so the query never drifts and
/// every iteration measures the same work.
fn bench_workspace_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("workspace_search/rescan");
    group.sample_size(10);
    for n in [1_000_usize, 10_000] {
        let root = workspace_tree(n);
        let mut app = App::new(root, Settings::default());
        app.run_action("search.workspace");
        for ch in "need".chars() {
            app.on_key(key(ch));
        }
        group.bench_with_input(BenchmarkId::from_parameter(n), &(), |b, ()| {
            b.iter(|| {
                app.on_key(key('l'));
                app.on_key(backspace());
            });
        });
    }
    group.finish();
}

fn bench_palette(c: &mut Criterion) {
    let mut group = c.benchmark_group("palette/fuzzy");
    for count in [1_000_usize, 20_000] {
        let index = paths(count);
        group.bench_with_input(BenchmarkId::new("filter", count), &index, |b, idx| {
            b.iter(|| {
                let hits = idx
                    .iter()
                    .filter(|p| fuzzy_match(p, black_box("edtparse")))
                    .count();
                black_box(hits)
            });
        });
        group.bench_with_input(BenchmarkId::new("rank", count), &index, |b, idx| {
            b.iter(|| {
                let mut scored: Vec<_> = idx
                    .iter()
                    .filter_map(|p| fuzzy_score(p, black_box("vixorg")).map(|s| (s, p)))
                    .collect();
                scored.sort_unstable_by_key(|a| std::cmp::Reverse(a.0));
                black_box(scored.len())
            });
        });
    }
    group.finish();
}

/// A checkout of `n` empty files, 100 to a directory, three levels deep
/// (`open/<a>/<b>/item<i>.rs`), for Quick Open (T845).
fn checkout_tree(n: usize) -> PathBuf {
    let dir = PathBuf::from("/tmp").join(format!("vix-bench-open-{n}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    for i in 0..n {
        let sub = dir
            .join("open")
            .join(format!("pkg{}", i / 10_000))
            .join(format!("mod{}", i / 100));
        if i % 100 == 0 {
            fs::create_dir_all(&sub).expect("create fixture subdirectory");
        }
        fs::write(sub.join(format!("item{i}.rs")), "").expect("write fixture file");
    }
    dir
}

/// Open Quick Open on `app` and type `query`.
fn quick_open(app: &mut App, query: &str) {
    app.run_action("tools.palette");
    for ch in query.chars() {
        app.on_key(key(ch));
    }
}

/// Tick until the background file index has landed.
fn wait_for_index(app: &mut App) {
    let start = std::time::Instant::now();
    while app.file_index_busy() {
        assert!(start.elapsed().as_secs() < 120, "file index never finished");
        std::thread::sleep(std::time::Duration::from_millis(5));
        app.tick();
    }
}

/// T845: Quick Open in a 20,000- and a 100,000-file checkout. `first_results`
/// is a fresh App opening Quick Open (the synchronous first pass) until it
/// lists files; `keystroke` types one character and backspaces it once the
/// whole index is in (two re-filters of every file per iteration). Setup
/// checks that every file is indexed and that files the first pass could not
/// hold are found by name.
fn bench_quick_open(c: &mut Criterion) {
    let mut group = c.benchmark_group("palette/quick_open");
    group.sample_size(10);
    for n in [20_000_usize, 100_000] {
        let root = checkout_tree(n);
        group.bench_with_input(BenchmarkId::new("first_results", n), &(), |b, ()| {
            b.iter_batched(
                || App::new(root.clone(), Settings::default()),
                |mut app| {
                    app.run_action("tools.palette");
                    assert!(!app.palette.as_ref().unwrap().entries.is_empty());
                    app
                },
                criterion::BatchSize::PerIteration,
            );
        });
        let mut app = App::new(root.clone(), Settings::default());
        quick_open(&mut app, "");
        wait_for_index(&mut app);
        assert_eq!(app.file_index_len(), n, "every file indexed");
        for probe in [0, n / 2, n - 1] {
            app.run_action("tools.palette");
            for ch in format!("item{probe}.rs").chars() {
                app.on_key(key(ch));
            }
            let wanted = format!("item{probe}.rs");
            assert!(
                app.palette
                    .as_ref()
                    .unwrap()
                    .entries
                    .iter()
                    .any(|e| e.label.ends_with(&wanted)),
                "{wanted} is findable"
            );
            app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        }
        quick_open(&mut app, "pkgmod");
        group.bench_with_input(BenchmarkId::new("keystroke", n), &(), |b, ()| {
            b.iter(|| {
                app.on_key(key('i'));
                app.on_key(backspace());
            });
        });
        drop(app);
        let _ = fs::remove_dir_all(&root);
    }
    group.finish();
}

/// A checkout of `n` small source files (four lines each, 100 to a
/// directory, three levels deep) for the T830 large workspace search: the
/// literal `marmalade` is on line 3 of every 20,000th file, so a search
/// for it matches a handful of files out of all of them.
fn search_checkout_tree(n: usize) -> PathBuf {
    let dir = PathBuf::from("/tmp").join(format!("vix-bench-search-{n}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    for i in 0..n {
        let sub = dir
            .join("src")
            .join(format!("pkg{}", i / 10_000))
            .join(format!("mod{}", i / 100));
        if i % 100 == 0 {
            fs::create_dir_all(&sub).expect("create fixture subdirectory");
        }
        let marker = if i % 20_000 == 7 {
            "marmalade"
        } else {
            "other"
        };
        let body = format!(
            "//! Item {i}.\nuse crate::prelude::*;\nfn item_{i}() -> &'static str {{ \"{marker}\" }}\n// end of item {i}\n"
        );
        fs::write(sub.join(format!("item{i}.rs")), body).expect("write fixture file");
    }
    dir
}

/// The median of `samples` (sorted in place).
fn median(samples: &mut [Duration]) -> Duration {
    samples.sort();
    samples.get(samples.len() / 2).copied().unwrap_or_default()
}

/// T830: one workspace search over a 100,000-file checkout, timed by hand
/// (a Criterion loop of second-long searches would take minutes). The query
/// is `marmalade`, which five files hold.
///
/// - `first_pass`: a fresh App's search the moment the panel opens, before
///   the background file index lands (the first 5,000 files).
/// - `cold`: the first search over the whole index (the query less its last
///   character, which the same five files hold), nothing remembered (every file read; the files were just written, so the OS cache is
///   warm — a cold disk is slower). Any new query that does not extend the
///   last one costs this.
/// - `narrowing`: typing the last character again, median of 7: a query
///   extending the last one skips the files the last scan found no match
///   in, with one `lstat` each.
/// - `widening`: the backspace before it, median of 7: a shorter query reads
///   every file again, like `cold`.
///
/// Run with `cargo bench --bench search_and_palette -- workspace_search/large`;
/// `VIX_BENCH_SEARCH_FILES` changes the size.
fn bench_workspace_search_large() {
    let n: usize = std::env::var("VIX_BENCH_SEARCH_FILES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000);
    let root = search_checkout_tree(n);
    let query = "marmalade";
    let (head, last) = query.split_at(query.len() - 1);
    let last = last.chars().next().unwrap();
    let mut app = App::new(root.clone(), Settings::default());
    app.run_action("search.workspace");
    for ch in head.chars() {
        app.on_key(key(ch));
    }
    let start = Instant::now();
    app.on_key(key(last));
    let first_pass = start.elapsed();
    wait_for_index(&mut app);
    assert_eq!(app.file_index_len(), n, "every file indexed");
    // The racy-file window (2 s) has to pass before a scan remembers a miss.
    std::thread::sleep(Duration::from_secs(2));
    let start = Instant::now();
    app.on_key(backspace());
    let cold = start.elapsed();
    let expected = n.div_ceil(20_000);
    assert_eq!(app.workspace_search_hits().len(), expected);
    app.on_key(key(last));
    assert_eq!(app.workspace_search_hits().len(), expected);
    let (mut narrowing, mut widening) = (Vec::new(), Vec::new());
    for _ in 0..7 {
        let start = Instant::now();
        app.on_key(backspace());
        widening.push(start.elapsed());
        let start = Instant::now();
        app.on_key(key(last));
        narrowing.push(start.elapsed());
        assert_eq!(app.workspace_search_hits().len(), expected);
    }
    println!(
        "workspace_search/large/{n}: first_pass {first_pass:.2?}, cold {cold:.2?}, narrowing median {:.2?}, widening median {:.2?} (7 runs each), {expected} hits",
        median(&mut narrowing),
        median(&mut widening),
    );
    drop(app);
    let _ = fs::remove_dir_all(&root);
}

criterion_group!(
    benches,
    bench_find,
    bench_workspace_search,
    bench_palette,
    bench_quick_open
);

fn main() {
    benches();
    Criterion::default().configure_from_args().final_summary();
    // `cargo test --benches` runs bench targets without `--bench`: only the
    // Criterion smoke run then, never the multi-second large search.
    if !std::env::args().any(|a| a == "--bench") {
        return;
    }
    let name = "workspace_search/large";
    let filter = std::env::args().skip(1).find(|a| !a.starts_with('-'));
    if filter
        .as_deref()
        .is_none_or(|f| name.contains(f) || f.contains(name))
    {
        bench_workspace_search_large();
    }
}
