//! Benchmarks for the editor widget's hot paths (`vix-editor-core`).
//!
//! Everything here happens while the user is holding a key down: opening a
//! file (parsing it — highlighting is a separate, always-lazy, per-viewport
//! cost, not measured by opening one), typing a character into a large
//! buffer, pasting a block, undoing, and the line transforms that rewrite the
//! whole buffer. The numbers that matter are the *large-buffer* ones — an
//! editor is judged on the 20,000-line file, not the 200-line one.

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::{Duration, Instant};
use vix::editor_core::actions::{Delete, InsertText};
use vix::editor_core::editor::Editor;

/// A Rust-shaped source buffer of roughly `lines` lines, so Tree-sitter has real
/// structure to parse and highlight rather than one long comment.
///
/// Tracks the line count from the fixed shape of each appended chunk (5 lines)
/// rather than rescanning the accumulated string on every iteration — at the
/// 2,500,000-line (~100 MB) size that `bench_open` now reaches, an
/// `out.lines().count()` guard would make this function itself O(n²).
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

/// An editor over `text`, parsed as Rust with no syntax theme.
fn editor(text: &str) -> Editor {
    Editor::new("rust", text, Vec::new()).expect("the text grammar always loads")
}

/// `Editor::new` returning is what actually blocks the user — real
/// highlighting is always computed lazily per visible viewport
/// (`Code::highlight_interval`, called only from the render path), never for
/// the whole buffer up front, at any size. Below `ASYNC_PARSE_THRESHOLD`
/// (`code.rs`, 50 KB) the initial Tree-sitter parse is also part of this
/// synchronous return; at or above it (the 125,000-line/~5 MB and
/// 2,500,000-line/~100 MB points — plan T006's "syntax-highlight a 5 MB Rust
/// file" and "open/parse a 100 MB synthetic file") the parse itself now runs
/// on the background worker (T121), so this measures only the fast part: a
/// rope allocation, a compiled-query cache lookup, and handing the buffer to
/// that worker. `iter_custom` is used instead of a plain `b.iter` so the
/// (now background, not blocking) parse can still be drained between
/// iterations without counting toward the timed measurement — otherwise
/// criterion's own iteration-count calibration, seeing this as newly cheap,
/// would fire off many concurrent 100 MB background parses in a tight loop.
/// See `bench_open_until_highlighted` below for the "how long until it's
/// actually fully parsed" number this one no longer measures for large
/// files.
fn bench_open(c: &mut Criterion) {
    let mut group = c.benchmark_group("editor/open");
    group.sample_size(10);
    for lines in [200_usize, 5_000, 20_000, 125_000, 2_500_000] {
        let text = source(lines);
        group.bench_with_input(BenchmarkId::from_parameter(lines), &text, |b, t| {
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;
                for _ in 0..iters {
                    let start = Instant::now();
                    let mut ed = black_box(editor(black_box(t)));
                    total += start.elapsed();
                    while ed.parse_pending() {
                        ed.poll_parse();
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
                total
            });
        });
    }
    group.finish();
}

/// Companion to `bench_open`: for the two sizes whose initial parse now runs
/// on the background worker (T121), how long until `Editor::new` *and* that
/// parse have both finished — the number `bench_open` measured before this
/// task, kept so the underlying parse cost (unchanged by T121; only *when*
/// it happens moved) stays visible. Below the async threshold this would be
/// identical to `bench_open`, so only the two async-triggering sizes are
/// covered here.
fn bench_open_until_highlighted(c: &mut Criterion) {
    let mut group = c.benchmark_group("editor/open_until_highlighted");
    group.sample_size(10);
    for lines in [125_000_usize, 2_500_000] {
        let text = source(lines);
        group.bench_with_input(BenchmarkId::from_parameter(lines), &text, |b, t| {
            b.iter(|| {
                let mut ed = editor(black_box(t));
                while ed.parse_pending() {
                    ed.poll_parse();
                    std::thread::sleep(Duration::from_millis(1));
                }
                black_box(ed)
            });
        });
    }
    group.finish();
}

/// Plan T006's "10k random inserts and deletes": one burst of 10,000
/// alternating inserts/backspace-deletes at random positions across a large
/// buffer, timed as a single unit — this is a *content-editing* workload
/// (structural churn, incremental reparse cost), unlike `bench_typing`
/// below, which measures the steady per-keystroke cost of one fixed spot.
fn bench_random_edits(c: &mut Criterion) {
    let mut group = c.benchmark_group("editor/random_edits");
    group.sample_size(10);
    let text = source(20_000);
    group.bench_function("10k_inserts_and_deletes", |b| {
        b.iter_batched(
            || editor(&text),
            |mut ed| {
                // A small xorshift64 PRNG — deterministic and dependency-free,
                // which is all a benchmark needs (no cryptographic or
                // statistical-quality requirement).
                let mut state = 0x2545_f491_4f6c_dd1d_u64;
                for _ in 0..10_000 {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    let len = ed.code_ref().len_chars();
                    #[allow(clippy::cast_possible_truncation)]
                    let pos = if len == 0 { 0 } else { (state as usize) % len };
                    ed.set_cursor(pos);
                    if state.is_multiple_of(2) {
                        ed.apply(InsertText {
                            text: "x".to_string(),
                        });
                    } else {
                        ed.apply(Delete);
                    }
                }
                black_box(ed);
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

fn bench_typing(c: &mut Criterion) {
    let mut group = c.benchmark_group("editor/typing");
    // 250,000 lines is ~10 MB at this generator's ~40 bytes/line -- plan
    // T121's "keypress-to-frame < 16 ms at 10 MB" target. This measures the
    // mutation + incremental-reparse-trigger side of a keystroke (the other
    // side, highlight-query execution for the redrawn frame, is already
    // viewport-scoped regardless of buffer size — see
    // `crates/vix-editor-core/spec/syntax-highlighting/index.md` — so it
    // isn't part of what a bigger buffer makes slower here).
    for lines in [200_usize, 5_000, 250_000] {
        let text = source(lines);
        group.bench_with_input(BenchmarkId::from_parameter(lines), &text, |b, t| {
            let mut ed = editor(t);
            // Realistic setup: the user has had the file open for a moment
            // before typing, not mid-way through the initial async parse
            // (T121) a truly fresh open would still be in.
            while ed.parse_pending() {
                ed.poll_parse();
                std::thread::sleep(Duration::from_millis(1));
            }
            ed.set_cursor(t.chars().count() / 2);
            b.iter(|| {
                ed.apply(InsertText {
                    text: "x".to_string(),
                });
            });
        });
    }
    group.finish();
}

fn bench_paste_and_undo(c: &mut Criterion) {
    let mut group = c.benchmark_group("editor/paste_undo");
    let text = source(2_000);
    let block = source(50);
    group.bench_function("paste_50_lines", |b| {
        let mut ed = editor(&text);
        ed.set_cursor(text.chars().count() / 2);
        b.iter(|| ed.paste_text(black_box(&block)));
    });
    group.bench_function("undo", |b| {
        // Each iteration makes an edit and undoes it, so the history depth stays
        // constant instead of growing across the sample.
        let mut ed = editor(&text);
        ed.set_cursor(text.chars().count() / 2);
        b.iter(|| {
            ed.apply(InsertText {
                text: "scratch".to_string(),
            });
            ed.undo();
        });
    });
    group.finish();
}

fn bench_line_transforms(c: &mut Criterion) {
    let mut group = c.benchmark_group("editor/line_transforms");
    let text = source(2_000);
    group.bench_function("sort_lines", |b| {
        let mut ed = editor(&text);
        b.iter(|| ed.sort_lines());
    });
    group.bench_function("remove_duplicate_lines", |b| {
        let mut ed = editor(&text);
        b.iter(|| ed.remove_duplicate_lines());
    });
    group.bench_function("trim_trailing_whitespace", |b| {
        let mut ed = editor(&text);
        b.iter(|| ed.trim_trailing_whitespace());
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_open,
    bench_open_until_highlighted,
    bench_typing,
    bench_random_edits,
    bench_paste_and_undo,
    bench_line_transforms
);
criterion_main!(benches);
