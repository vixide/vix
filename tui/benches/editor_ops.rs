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

/// Highlight Word (`App::refresh_word_highlight`, T851) as the event loop
/// runs it after a cursor move: the cursor steps between `compute_something`
/// and `alpha` (each once in the buffer, so the search reads to the end)
/// and, for `common`, between two `the`s in the filler (500 marks, the cap,
/// are found early). Since T852 both read only the lines on and around the
/// view and the cursor (at most 3,000 lines past the view each way), so
/// `rare` no longer reads to the end of the buffer.
fn bench_word_highlight(c: &mut Criterion) {
    let mut group = c.benchmark_group("editor/word_highlight");
    group.sample_size(20);
    for (label, bytes) in [("4KiB", 4 << 10), ("512KiB", 512 << 10), ("8MiB", 8 << 20)] {
        let (mut app, cursor) = vi_app(&format!("word-highlight-{label}"), bytes);
        app.settings.editor_visual.highlight_word = true;
        let line_start = cursor - 16;
        for (name, a, b) in [
            ("rare", cursor, line_start + 34),
            ("common", line_start - 61, line_start - 61 + 31),
        ] {
            group.bench_function(BenchmarkId::new(name, label), |bench| {
                bench.iter(|| {
                    for at in [a, b] {
                        app.editor.active_tab_mut().unwrap().editor.set_cursor(at);
                        app.refresh_word_highlight();
                    }
                });
            });
        }
    }
    group.finish();
}

/// Go → Word → Next, Previous, Start and End, then Go → Sentence → Start
/// and End (`nav.word_*`, `nav.sentence_start`/`_end` through
/// `App::run_action`, T851), around the same line in prose: each should
/// read the words or sentences it crosses, not the whole buffer.
fn bench_go_words(c: &mut Criterion) {
    let mut group = c.benchmark_group("editor/go_words");
    group.sample_size(20);
    for (label, bytes) in [("4KiB", 4 << 10), ("512KiB", 512 << 10), ("8MiB", 8 << 20)] {
        let (mut app, cursor) = vi_app_filled(
            &format!("go-words-{label}"),
            bytes,
            "The quick brown fox jumps. Over the lazy dog! Again and again?
",
        );
        let actions: [(&str, &[&str]); 3] = [
            ("word_next_prev", &["nav.word_next", "nav.word_prev"]),
            ("word_start_end", &["nav.word_start", "nav.word_end"]),
            (
                "sentence_start_end",
                &["nav.sentence_start", "nav.sentence_end"],
            ),
        ];
        for (name, ids) in actions {
            group.bench_function(BenchmarkId::new(name, label), |b| {
                b.iter(|| {
                    for id in ids {
                        app.run_action(id);
                    }
                    app.editor
                        .active_tab_mut()
                        .unwrap()
                        .editor
                        .set_cursor(cursor + 3);
                });
            });
        }
        // Go → Word → Number and Go → Sentence → Number, to the 10th: they
        // count from the start of the buffer, so they read up to it.
        let area = app.layout.editor;
        group.bench_function(BenchmarkId::new("goto_word_and_sentence_10", label), |b| {
            b.iter(|| {
                app.editor.goto_word(10, area);
                app.editor.goto_sentence(10, area);
            });
        });
    }
    group.finish();
}

/// Go → Matching Tag (`nav.matching_tag`, T851) there and back, between the
/// `<li>` and `</li>` of one line of HTML in the middle of the buffer.
fn bench_matching_tag(c: &mut Criterion) {
    const FILLER: &str = "<li><a href=\"page.html\">An item</a> and <b>more</b></li>\n";
    let mut group = c.benchmark_group("editor/matching_tag");
    group.sample_size(20);
    for (label, bytes) in [("4KiB", 4 << 10), ("512KiB", 512 << 10), ("8MiB", 8 << 20)] {
        let (mut app, cursor) = vi_app_filled(&format!("matching-tag-{label}"), bytes, FILLER);
        let li = cursor - 16 - FILLER.len() + 1;
        group.bench_function(BenchmarkId::new("there_and_back", label), |b| {
            b.iter(|| {
                app.editor.active_tab_mut().unwrap().editor.set_cursor(li);
                app.run_action("nav.matching_tag");
                app.run_action("nav.matching_tag");
            });
        });
    }
    group.finish();
}

/// One keystroke and the frame after it, through the real `App` key path
/// with the default keymap (T858): type `x` and draw, then Backspace and
/// draw, in the middle of a plain-text buffer of Rust-shaped lines (so the
/// sticky-scroll header has declarations to find). Before T858 the frame
/// after an edit scanned the whole buffer for declarations, about 7 ms per
/// MiB; it should not depend on the size at all.
fn bench_keystroke_frame(c: &mut Criterion) {
    use vix::settings::Settings;
    let mut group = c.benchmark_group("editor/keystroke");
    group.sample_size(10);
    for (label, bytes) in [
        ("4KiB", 4 << 10),
        ("512KiB", 512 << 10),
        ("8MiB", 8 << 20),
        ("64MiB", 64 << 20),
    ] {
        let mut text = source(bytes / 16);
        // `source` is ASCII, so any byte length is a character boundary.
        text.truncate(bytes);
        let dir = std::env::temp_dir().join(format!(
            "vix-bench-keystroke-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("big.txt");
        std::fs::write(&file, &text).unwrap();
        let mut settings = Settings::default();
        settings.editor_behavior.large_file_mb = 0;
        let mut app = vix::app::App::new(dir.clone(), settings)
            .with_session_path(dir.join("session.toml"))
            .with_backup_dir(dir.join("backup"));
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 120, 40);
        app.open_initial(&file);
        let middle = text.chars().count() / 2;
        drop(text);
        app.editor
            .active_tab_mut()
            .unwrap()
            .editor
            .set_cursor(middle);
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
        term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
        let backspace =
            vix_input::KeyEvent::new(vix_input::KeyCode::Backspace, vix_input::KeyModifiers::NONE);
        group.bench_function(
            BenchmarkId::new("type_and_backspace_with_frames", label),
            |b| {
                b.iter(|| {
                    for k in [vi_key('x'), backspace] {
                        app.on_key(k);
                        term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
                    }
                });
            },
        );
        let _ = std::fs::remove_dir_all(&dir);
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
    bench_go_sentences,
    bench_word_highlight,
    bench_go_words,
    bench_matching_tag,
    bench_keystroke_frame
);
criterion_main!(benches);
