//! Benchmarks for the three per-keystroke search paths.
//!
//! All three run on *every* character typed into their input box, over the
//! whole corpus: the find bar re-scans the buffer, the command palette
//! re-filters the file index, and workspace search re-scans every file under
//! the root. A regression here is felt as typing lag, which is why they are
//! benchmarked rather than merely tested.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use regex::Regex;
use std::fs;
use std::hint::black_box;
use std::path::PathBuf;
use vix::app::App;
use vix::find_panel;
use vix::palette::{fuzzy_match, fuzzy_score};
use vix::settings::Settings;

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

criterion_group!(benches, bench_find, bench_workspace_search, bench_palette);
criterion_main!(benches);
