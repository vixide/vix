//! Benchmarks for the editor widget's hot paths (`vix-editor-core`).
//!
//! Everything here happens while the user is holding a key down: opening a file
//! (parse + first highlight), typing a character into a large buffer, pasting a
//! block, undoing, and the line transforms that rewrite the whole buffer. The
//! numbers that matter are the *large-buffer* ones — an editor is judged on the
//! 20,000-line file, not the 200-line one.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use vix::editor_core::actions::InsertText;
use vix::editor_core::editor::Editor;

/// A Rust-shaped source buffer of roughly `lines` lines, so Tree-sitter has real
/// structure to parse and highlight rather than one long comment.
fn source(lines: usize) -> String {
    let mut out = String::with_capacity(lines * 40);
    let mut i = 0;
    while out.lines().count() < lines {
        out.push_str(&format!(
            "/// Item {i}.\npub fn item_{i}(value: &str) -> usize {{\n    let n = value.len();\n    if n > {i} {{ n - {i} }} else {{ 0 }}\n}}\n\n"
        ));
        i += 1;
    }
    out
}

/// An editor over `text`, parsed as Rust with no syntax theme.
fn editor(text: &str) -> Editor {
    Editor::new("rust", text, Vec::new()).expect("the text grammar always loads")
}

fn bench_open(c: &mut Criterion) {
    let mut group = c.benchmark_group("editor/open");
    for lines in [200_usize, 5_000] {
        let text = source(lines);
        group.bench_with_input(BenchmarkId::from_parameter(lines), &text, |b, t| {
            b.iter(|| black_box(editor(black_box(t))));
        });
    }
    group.finish();
}

fn bench_typing(c: &mut Criterion) {
    let mut group = c.benchmark_group("editor/typing");
    for lines in [200_usize, 5_000] {
        let text = source(lines);
        group.bench_with_input(BenchmarkId::from_parameter(lines), &text, |b, t| {
            let mut ed = editor(t);
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
    bench_typing,
    bench_paste_and_undo,
    bench_line_transforms
);
criterion_main!(benches);
