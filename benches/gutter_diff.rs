//! Benchmark for the per-buffer git gutter diff (`vix_git::diff_marks`, T574).
//!
//! It runs on the UI thread every time git state refreshes, against `HEAD`'s
//! text. Myers is `O((N+M)·D)`, so the cost depends on how much the buffer
//! differs, not just how big it is. The regression to watch is the heavily
//! rewritten file: the 40 ms deadline must keep it bounded (look at the
//! `rewritten` rows staying near that budget as the line count grows) and the
//! 50,000-line cap must keep a giant buffer to the cost of counting its lines.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::fmt::Write as _;
use std::hint::black_box;
use vix::git::diff_marks;

/// `lines` distinct source-like lines.
fn source(lines: usize) -> String {
    (0..lines).fold(String::new(), |mut out, i| {
        writeln!(out, "    let value_{i} = compute({i}) + {};", i % 97)
            .expect("writing to a String cannot fail");
        out
    })
}

/// `text` with about one line in `every` changed.
fn lightly_edited(text: &str, every: usize) -> String {
    text.lines()
        .enumerate()
        .fold(String::new(), |mut out, (i, line)| {
            if i % every == 0 {
                out.push_str("    // edited\n");
            } else {
                out.push_str(line);
                out.push('\n');
            }
            out
        })
}

/// The worst case for Myers: a buffer made of a few dozen repeated lines in a
/// scrambled order, so there are very many partial matches and a long edit
/// script (a distinct-lines rewrite is *easy*, with nothing to match).
fn rewritten(lines: usize) -> String {
    (0..lines).fold(String::new(), |mut out, i| {
        writeln!(out, "    repeated_{} += 1;", (i * 7919 + i / 3) % 48)
            .expect("writing to a String cannot fail");
        out
    })
}

/// The same few dozen repeated lines in a different order (for the head side).
fn repetitive(lines: usize) -> String {
    (0..lines).fold(String::new(), |mut out, i| {
        writeln!(out, "    repeated_{} += 1;", (i * 31 + i / 5) % 48)
            .expect("writing to a String cannot fail");
        out
    })
}

fn bench_gutter_diff(c: &mut Criterion) {
    let mut group = c.benchmark_group("git/diff_marks");
    for lines in [1_000_usize, 10_000, 40_000] {
        let head = source(lines);
        let unchanged = head.clone();
        let edited = lightly_edited(&head, 50);
        let repetitive_head = repetitive(lines);
        let rewrite = rewritten(lines);
        group.bench_with_input(BenchmarkId::new("unchanged", lines), &lines, |b, _| {
            b.iter(|| black_box(diff_marks(black_box(&head), black_box(&unchanged))));
        });
        group.bench_with_input(BenchmarkId::new("lightly_edited", lines), &lines, |b, _| {
            b.iter(|| black_box(diff_marks(black_box(&head), black_box(&edited))));
        });
        group.bench_with_input(BenchmarkId::new("rewritten", lines), &lines, |b, _| {
            b.iter(|| black_box(diff_marks(black_box(&repetitive_head), black_box(&rewrite))));
        });
    }
    // Over the cap: returns immediately without diffing.
    let giant = source(60_000);
    group.bench_function("over_the_line_cap", |b| {
        b.iter(|| black_box(diff_marks(black_box(&giant), black_box(&giant))));
    });
    group.finish();
}

criterion_group!(benches, bench_gutter_diff);
criterion_main!(benches);
