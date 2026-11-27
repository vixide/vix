//! Benchmarks for the editor widget's hot paths (`vix-editor-core`).
//!
//! Everything here happens while the user is holding a key down: opening a
//! file (parsing it — highlighting is a separate, always-lazy, per-viewport
//! cost, not measured by opening one), typing a character into a large
//! buffer, pasting a block, undoing, and the line transforms that rewrite the
//! whole buffer. The numbers that matter are the *large-buffer* ones — an
//! editor is judged on the 20,000-line file, not the 200-line one.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use std::fmt::Write as _;
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
        write!(
            out,
            "/// Item {i}.\npub fn item_{i}(value: &str) -> usize {{\n    let n = value.len();\n    if n > {i} {{ n - {i} }} else {{ 0 }}\n}}\n\n"
        )
        .expect("writing to a String cannot fail");
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
                    #[allow(clippy::cast_possible_truncation, reason = "benchmark sizes are small")]
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

/// The Vi (modal-engine) keymap over a buffer of about `bytes` bytes whose
/// middle line is always the same `TEST_LINE` (T843). The rest is unrelated
/// filler, so the cost of a motion on that line should not depend on
/// `bytes` at all: a same-line step reads the line, not the buffer.
fn vi_app(tag: &str, bytes: usize) -> (vix::app::App, usize) {
    vi_app_filled(
        tag,
        bytes,
        "the quick brown fox jumps over the lazy dog, again and again\n",
    )
}

/// [`vi_app`] with `filler` as the unrelated text (T830 uses prose, so
/// `(`/`)` have sentences to stop at).
fn vi_app_filled(tag: &str, bytes: usize, filler: &str) -> (vix::app::App, usize) {
    use vix::settings::Settings;
    const TEST_LINE: &str =
        "    let value = compute_something(alpha, beta, gamma, delta); // note\n";
    let half = bytes / 2 / filler.len();
    let mut text = filler.repeat(half);
    let line_start = text.chars().count();
    text.push_str(TEST_LINE);
    text.push_str(&filler.repeat(half));
    let dir = std::env::temp_dir().join(format!("vix-bench-vi-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("a.txt");
    std::fs::write(&file, &text).unwrap();
    let mut settings = Settings::default();
    settings.keymap = "vi".to_string();
    settings.subsystems.modal_engine = true;
    let mut app = vix::app::App::new(dir.clone(), settings)
        .with_session_path(dir.join("session.toml"))
        .with_backup_dir(dir.join("backup"));
    app.layout.editor = ratatui::layout::Rect::new(0, 0, 120, 40);
    app.open_initial(&file);
    // On `compute_something`: a word with words on both sides.
    let cursor = line_start + 16;
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_cursor(cursor);
    (app, cursor)
}

/// One key press, as the terminal reports a plain character.
fn vi_key(c: char) -> vix_input::KeyEvent {
    vix_input::KeyEvent::new(vix_input::KeyCode::Char(c), vix_input::KeyModifiers::NONE)
}

/// Vi motions and counted operators through the real `App` key path
/// (T843): `l`+`h`, `w`+`b`, `3dw`+`u` and `2dd`+`u`, each pair
/// returning the cursor (and buffer) to where it started, plus `l`+`h`
/// with a frame drawn after each key (key-to-frame, what the
/// `VIX_PROFILE` profiler's "input to frame" phase times). Three buffer
/// sizes around the same line: the numbers should not grow with size.
fn bench_vi_motions(c: &mut Criterion) {
    let mut group = c.benchmark_group("editor/vi");
    group.sample_size(20);
    for (label, bytes) in [("4KiB", 4 << 10), ("512KiB", 512 << 10), ("8MiB", 8 << 20)] {
        let (mut app, cursor) = vi_app(label, bytes);
        let keys: [(&str, &[char]); 4] = [
            ("l_h", &['l', 'h']),
            ("w_b", &['w', 'b']),
            ("3dw_u", &['3', 'd', 'w', 'u']),
            ("2dd_u", &['2', 'd', 'd', 'u']),
        ];
        for (name, seq) in keys {
            group.bench_function(BenchmarkId::new(name, label), |b| {
                b.iter(|| {
                    for &k in seq {
                        app.on_key(vi_key(k));
                    }
                    app.editor
                        .active_tab_mut()
                        .unwrap()
                        .editor
                        .set_cursor(cursor);
                });
            });
        }
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
        group.bench_function(BenchmarkId::new("l_h_with_frames", label), |b| {
            b.iter(|| {
                for k in ['l', 'h'] {
                    app.on_key(vi_key(k));
                    term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
                }
            });
        });
    }
    group.finish();
}

/// `)` + `(` through the real `App` key path (T830), in a buffer of
/// prose (sentences every few words) around the same line: each sentence
/// motion should read the sentences it crosses, not the whole buffer.
fn bench_vi_sentences(c: &mut Criterion) {
    let mut group = c.benchmark_group("editor/vi_sentences");
    group.sample_size(20);
    for (label, bytes) in [("4KiB", 4 << 10), ("512KiB", 512 << 10), ("8MiB", 8 << 20)] {
        let (mut app, cursor) = vi_app_filled(
            &format!("sentences-{label}"),
            bytes,
            "The quick brown fox jumps. Over the lazy dog! Again and again?\n",
        );
        group.bench_function(BenchmarkId::new("rparen_lparen", label), |b| {
            b.iter(|| {
                for k in [')', '('] {
                    app.on_key(vi_key(k));
                }
                app.editor
                    .active_tab_mut()
                    .unwrap()
                    .editor
                    .set_cursor(cursor);
            });
        });
    }
    group.finish();
}

/// Go → Sentence → Next then Previous (`nav.sentence_next` /
/// `nav.sentence_prev` through `App::run_action`, T830) in the same prose
/// buffer as [`bench_vi_sentences`]: each should read the sentences it
/// crosses, not the whole buffer.
fn bench_go_sentences(c: &mut Criterion) {
    let mut group = c.benchmark_group("editor/go_sentences");
    group.sample_size(20);
    for (label, bytes) in [("4KiB", 4 << 10), ("512KiB", 512 << 10), ("8MiB", 8 << 20)] {
        let (mut app, cursor) = vi_app_filled(
            &format!("go-sentences-{label}"),
            bytes,
            "The quick brown fox jumps. Over the lazy dog! Again and again?\n",
        );
        group.bench_function(BenchmarkId::new("next_prev", label), |b| {
            b.iter(|| {
                app.run_action("nav.sentence_next");
                app.run_action("nav.sentence_prev");
                app.editor
                    .active_tab_mut()
                    .unwrap()
                    .editor
                    .set_cursor(cursor);
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_open,
    bench_open_until_highlighted,
    bench_typing,
    bench_random_edits,
    bench_paste_and_undo,
    bench_line_transforms,
    bench_vi_motions,
    bench_vi_sentences,
    bench_go_sentences
);
criterion_main!(benches);
