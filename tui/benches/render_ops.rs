//! Benchmarks for drawing: the two costs between a keystroke and the pixels.
//!
//! - `render/highlight_viewport` is the per-viewport syntax highlight
//!   (`Editor::highlight_interval`), which `editor_ops` deliberately leaves out
//!   because opening a file never pays it. Each iteration clears the editor's
//!   highlight cache first, so it measures a cache *miss*: what a scroll, or
//!   the first frame after an edit, costs. Highlighting walks the Tree-sitter
//!   tree for one screenful, so the number to watch is that the cost stays flat
//!   as the file grows and as the viewport moves down it.
//! - `render/frame` is a whole `ui::draw` of the `App` into a 200×50 terminal:
//!   menu bar, tab bar, editor, gutter and status line. `steady` redraws an
//!   unchanged screen (caches hot, what an idle tick or a cursor blink costs);
//!   `after_keystroke` types and deletes a character before each draw, so the
//!   buffer revision changes and the frame's caches miss.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::{Color, Style};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::Duration;
use vix::app::App;
use vix::editor_core::editor::Editor;
use vix::settings::Settings;
use vix_input::{KeyCode, KeyEvent, KeyModifiers};

/// Lines in one viewport: a tall terminal's editor area.
const VIEWPORT_LINES: usize = 60;

/// A Rust-shaped source buffer of roughly `lines` lines (the same shape as
/// `editor_ops`), five lines per item, all ASCII so char and byte offsets
/// agree.
fn source(lines: usize) -> String {
    const LINES_PER_CHUNK: usize = 5;
    let mut out = String::with_capacity(lines * 40);
    let mut i = 0;
    let mut have = 0;
    while have < lines {
        write!(
            out,
            "/// Item {i}.\npub fn item_{i}(value: &str) -> usize {{\n    let n = value.len();\n    if n > \"{i}\".len() {{ n - {i} }} else {{ 0 }}\n}}\n\n"
        )
        .expect("writing to a String cannot fail");
        i += 1;
        have += LINES_PER_CHUNK;
    }
    out
}

/// The char offset of the start of each line of `text`.
fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect()
}

/// A syntax theme over the common capture names, so every token is styled.
fn theme() -> HashMap<String, Style> {
    [
        "keyword",
        "function",
        "type",
        "string",
        "number",
        "comment",
        "variable",
        "operator",
        "punctuation",
        "constant",
    ]
    .iter()
    .enumerate()
    .map(|(i, name)| {
        let i = u8::try_from(i).expect("ten names");
        ((*name).to_string(), Style::default().fg(Color::Indexed(i)))
    })
    .collect()
}

/// An editor over `text` with its Tree-sitter parse finished (large buffers
/// parse on a background worker).
fn parsed_editor(text: &str) -> Editor {
    let mut ed = Editor::new("rust", text, Vec::new()).expect("the rust grammar loads");
    while ed.parse_pending() {
        ed.poll_parse();
        std::thread::sleep(Duration::from_millis(1));
    }
    ed
}

/// One viewport's highlight, cache cold, at the top, middle and end of files
/// from 1,000 to 125,000 lines.
fn bench_highlight_viewport(c: &mut Criterion) {
    let mut group = c.benchmark_group("render/highlight_viewport");
    let theme = theme();
    for lines in [1_000_usize, 20_000, 125_000] {
        let text = source(lines);
        let starts = line_starts(&text);
        let ed = parsed_editor(&text);
        let last = starts.len() - VIEWPORT_LINES - 1;
        for (place, first) in [("top", 0), ("middle", last / 2), ("end", last)] {
            let (start, end) = (starts[first], starts[first + VIEWPORT_LINES]);
            group.bench_with_input(
                BenchmarkId::new(place, lines),
                &(start, end),
                |b, &(s, e)| {
                    b.iter(|| {
                        ed.reset_highlight_cache();
                        black_box(ed.highlight_interval(black_box(s), black_box(e), &theme))
                    });
                },
            );
        }
    }
    group.finish();
}

/// A scratch workspace holding one `lines`-line Rust file, opened in an `App`
/// whose parse has finished.
fn app_with_file(lines: usize) -> App {
    let dir = std::env::temp_dir().join(format!("vix-bench-render-{lines}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir: PathBuf = std::fs::canonicalize(&dir).unwrap();
    let file = dir.join("main.rs");
    std::fs::write(&file, source(lines)).unwrap();
    let mut app = App::new(dir, Settings::default());
    app.open_initial(&file);
    while app.parse_busy() {
        app.poll_parse();
        std::thread::sleep(Duration::from_millis(1));
    }
    app
}

/// A whole frame, drawn into a 200×50 test terminal.
fn bench_frame(c: &mut Criterion) {
    let mut group = c.benchmark_group("render/frame");
    let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
    for lines in [1_000_usize, 20_000, 500_000] {
        let mut app = app_with_file(lines);
        let mut term = Terminal::new(TestBackend::new(200, 50)).unwrap();
        // One untimed frame, so `steady` starts from a drawn screen.
        term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
        group.bench_function(BenchmarkId::new("steady", lines), |b| {
            b.iter(|| {
                term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
            });
        });
        group.bench_function(BenchmarkId::new("after_keystroke", lines), |b| {
            b.iter(|| {
                app.on_key(key(KeyCode::Char('x')));
                app.on_key(key(KeyCode::Backspace));
                term.draw(|f| vix::ui::draw(&mut app, f)).unwrap();
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_highlight_viewport, bench_frame);
criterion_main!(benches);
