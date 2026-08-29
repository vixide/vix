//! Benchmarks for the pure text transforms (`vix-textops`).
//!
//! These run on every keystroke of the commands that use them (transpose, delete
//! by unit, wrap, toggle), and they all rebuild their unit ranges from scratch —
//! so their cost is a function of *buffer* size, not of the edit. That is the
//! thing to watch: a transform that is O(buffer) per keystroke is fine at 200
//! lines and not fine at 20,000.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use vix::textops;

/// A prose buffer of `lines` lines: sentences, paragraphs, and section breaks,
/// so the sentence/paragraph/section unit builders all have real work to do.
fn prose(lines: usize) -> String {
    let mut out = String::with_capacity(lines * 60);
    for i in 0..lines {
        match i % 12 {
            11 => out.push('\n'),      // blank line: paragraph break
            5 => out.push_str("\n\n"), // double blank: section break
            _ => {
                out.push_str("The quick brown fox jumps over the lazy dog. ");
                out.push_str("Pack my box with five dozen liquor jugs.\n");
            }
        }
    }
    out
}

fn bench_cursor_rewrites(c: &mut Criterion) {
    let mut group = c.benchmark_group("textops/cursor_rewrites");
    for lines in [100_usize, 2_000] {
        let text = prose(lines);
        let cursor = text.chars().count() / 2;
        group.bench_with_input(BenchmarkId::new("transpose_words", lines), &text, |b, t| {
            b.iter(|| black_box(textops::transpose_words_at(black_box(t), cursor)));
        });
        group.bench_with_input(
            BenchmarkId::new("transpose_sentences", lines),
            &text,
            |b, t| {
                b.iter(|| black_box(textops::transpose_sentences_at(black_box(t), cursor)));
            },
        );
        group.bench_with_input(BenchmarkId::new("delete_word", lines), &text, |b, t| {
            b.iter(|| black_box(textops::delete_word_at(black_box(t), cursor)));
        });
        group.bench_with_input(
            BenchmarkId::new("delete_paragraph", lines),
            &text,
            |b, t| {
                b.iter(|| black_box(textops::delete_paragraph_at(black_box(t), cursor)));
            },
        );
        group.bench_with_input(BenchmarkId::new("smart_toggle", lines), &text, |b, t| {
            b.iter(|| black_box(textops::smart_toggle_at(black_box(t), cursor)));
        });
    }
    group.finish();
}

fn bench_whole_text(c: &mut Criterion) {
    let mut group = c.benchmark_group("textops/whole_text");
    let text = prose(2_000);
    group.bench_function("wrap_80", |b| {
        b.iter(|| black_box(textops::wrap(black_box(&text), 80)));
    });
    group.bench_function("to_crlf", |b| {
        b.iter(|| black_box(textops::to_crlf(black_box(&text))));
    });
    group.bench_function("squeeze_blank_lines", |b| {
        b.iter(|| black_box(textops::squeeze_blank_lines(black_box(&text))));
    });
    group.bench_function("sentence_starts", |b| {
        b.iter(|| black_box(textops::sentence_starts(black_box(&text))));
    });
    group.finish();
}

criterion_group!(benches, bench_cursor_rewrites, bench_whole_text);
criterion_main!(benches);
