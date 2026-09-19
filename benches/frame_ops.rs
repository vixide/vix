//! Benchmarks for the two per-*frame* buffer scans found and fixed by T510/
//! T511: the editor diff gutter (`vix_git::diff_marks`) and the sticky-scroll
//! header / breadcrumb declaration scan (`vix_palette::symbols`). Both used to
//! run, uncached, on every redraw of a git-tracked or scrolled buffer — the
//! `App`-level fix was to cache each by `(path/tab, buffer revision)`, so
//! these benchmark only the underlying pure functions (what a cache *miss*
//! costs); the cache-hit path is a cheap key comparison, not worth a
//! microbenchmark. A regression here is felt as frame lag, not typing lag —
//! see `benches/editor_ops.rs` for the per-keystroke costs.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use vix::git::diff_marks;
use vix::palette::symbols;

/// A Rust-shaped source buffer of roughly `lines` lines — enough real
/// declarations for `symbols` to have something to find, and enough plain
/// body lines for `diff_marks` to diff.
fn source(lines: usize) -> String {
    const LINES_PER_CHUNK: usize = 5;
    let mut out = String::with_capacity(lines * 40);
    let mut i = 0;
    let mut have = 0;
    while have < lines {
        out.push_str(&format!(
            "/// Item {i}.\npub fn item_{i}(value: &str) -> usize {{\n    let n = value.len();\n    if n > {i} {{ n - {i} }} else {{ 0 }}\n}}\n\n"
        ));
        i += 1;
        have += LINES_PER_CHUNK;
    }
    out
}

/// The diff-gutter path: HEAD's blob vs. the current buffer, with roughly one
/// changed line in twenty (a realistic "still editing this file" spread, not
/// a full rewrite).
fn bench_diff_marks(c: &mut Criterion) {
    let mut group = c.benchmark_group("git/diff_marks");
    for lines in [1_000_usize, 20_000, 100_000] {
        let head = source(lines);
        let mut current = head.clone();
        // Touch every 20th line's body so there's real, scattered work to find.
        let touched: String = current
            .lines()
            .enumerate()
            .map(|(i, l)| {
                if i % 20 == 0 && l.trim_start().starts_with("let n") {
                    "    let n = value.len() + 1;\n".to_string()
                } else {
                    format!("{l}\n")
                }
            })
            .collect();
        current = touched;
        group.bench_with_input(BenchmarkId::from_parameter(lines), &(), |b, ()| {
            b.iter(|| black_box(diff_marks(black_box(&head), black_box(&current))));
        });
    }
    group.finish();
}

/// The sticky-scroll/breadcrumb declaration scan: `symbols` walks every line
/// once, regex-matching for a structural keyword.
fn bench_symbols(c: &mut Criterion) {
    let mut group = c.benchmark_group("palette/symbols");
    for lines in [1_000_usize, 20_000, 100_000] {
        let text = source(lines);
        group.bench_with_input(BenchmarkId::from_parameter(lines), &text, |b, t| {
            b.iter(|| black_box(symbols(black_box(t))));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_diff_marks, bench_symbols);
criterion_main!(benches);
