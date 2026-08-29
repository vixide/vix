//! Benchmarks for the two per-keystroke search paths.
//!
//! Both run on *every* character typed into their input box, over the whole
//! corpus: the find bar re-scans the buffer, and the command palette re-filters
//! the file index. A regression here is felt as typing lag, which is why they
//! are benchmarked rather than merely tested.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use regex::Regex;
use std::hint::black_box;
use vix::find_panel;
use vix::palette::{fuzzy_match, fuzzy_score};

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

criterion_group!(benches, bench_find, bench_palette);
criterion_main!(benches);
