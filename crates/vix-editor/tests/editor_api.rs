//! T608: unit tests for `vix_editor::editor`'s public functions — the cursor
//! and selection math (multi-byte, wide CJK and combining text, empty buffers,
//! start/end of line and buffer, past-the-end clamping), the tab strip, the
//! split panes, the line edits, undo/redo interplay, and the save/open error
//! paths. Terminal-independent: every test drives an [`Editor`] and asserts on
//! its state, never on rendering or translated text.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use proptest::prelude::*;
use ratatui::layout::Rect;
use vix_editor::editor::{
    Editor, Flags, REMOTE_PATH_PREFIX, SaveOptions, SplitDir, is_image_path, is_remote_path,
    text_field,
};

/// The editor viewport the scrolling commands keep the cursor inside.
const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

/// No on-save normalization.
const PLAIN: SaveOptions = SaveOptions {
    trim_trailing_whitespace: false,
    ensure_final_newline: false,
};

/// An editor whose single (untitled, clean) buffer holds `text`, cursor at 0.
fn ed(text: &str) -> Editor {
    let mut e = Editor::new(Flags::empty(), "    ".into());
    let t = e.active_tab_mut().unwrap();
    t.editor.set_content(text);
    t.editor.set_cursor(0);
    e
}

/// [`ed`] with the cursor at char offset `at`.
fn ed_at(text: &str, at: usize) -> Editor {
    let mut e = ed(text);
    e.active_tab_mut().unwrap().editor.set_cursor(at);
    e
}

/// The active cursor's char offset.
fn cur(e: &Editor) -> usize {
    e.active_tab().unwrap().editor.get_cursor()
}

/// The active buffer's text.
fn text(e: &Editor) -> String {
    e.active_tab().unwrap().text()
}

/// The active selection as `(start, end)`, if it is non-empty.
fn sel(e: &mut Editor) -> Option<(usize, usize)> {
    e.active_tab_mut()
        .unwrap()
        .editor
        .get_selection()
        .filter(|s| !s.is_empty())
        .map(|s| (s.start, s.end))
}

/// A fresh scratch directory for one file test.
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vix-editor-api-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

// ---- free functions -------------------------------------------------------

#[test]
fn is_image_path_matches_image_extensions_case_insensitively() {
    for (path, want) in [
        ("a.png", true),
        ("a.PNG", true),
        ("dir.d/photo.JpEg", true),
        ("a.jpg", true),
        ("a.gif", true),
        ("a.bmp", true),
        ("a.webp", true),
        ("a.ico", true),
        ("a.tif", true),
        ("a.tiff", true),
        ("a.svg", false),
        ("a.txt", false),
        ("png", false),
        // A dotfile has no extension.
        (".png", false),
        ("a.png.bak", false),
        ("", false),
    ] {
        assert_eq!(is_image_path(Path::new(path)), want, "{path:?}");
    }
}

#[test]
fn is_remote_path_needs_the_prefix_at_the_start() {
    for (path, want) in [
        (format!("{REMOTE_PATH_PREFIX}src/main.rs"), true),
        (REMOTE_PATH_PREFIX.to_string(), true),
        ("src/cowork:main.rs".to_string(), false),
        ("cowork".to_string(), false),
        (String::new(), false),
    ] {
        assert_eq!(is_remote_path(Path::new(&path)), want, "{path:?}");
    }
}

#[test]
fn text_field_holds_its_content_verbatim() {
    for content in ["", "https://example.com", "日本語 ✓", "a\nb"] {
        assert_eq!(text_field(content).get_content(), content);
    }
}

// ---- Tab ------------------------------------------------------------------

#[test]
fn tab_cursor_1based_counts_chars_not_bytes() {
    // Line 2 starts at char 6; 'x' is char 9, the 4th char of line 2.
    let text = "héllo\n日本語x\n";
    for (at, want) in [
        (0, (1, 1)),
        (5, (1, 6)),
        (6, (2, 1)),
        (9, (2, 4)),
        (10, (2, 5)),
        (11, (3, 1)),
    ] {
        let e = ed_at(text, at);
        assert_eq!(e.active_tab().unwrap().cursor_1based(), want, "at {at}");
        assert_eq!(e.cursor_1based(), want, "editor at {at}");
    }
}

#[test]
fn tab_lines_and_line_count() {
    for (text, lines, count) in [
        ("", vec![], 1),
        ("a", vec!["a"], 1),
        ("a\n", vec!["a"], 2),
        ("a\n\nb", vec!["a", "", "b"], 3),
        ("日\n本\n", vec!["日", "本"], 3),
    ] {
        let e = ed(text);
        let tab = e.active_tab().unwrap();
        assert_eq!(tab.lines(), lines, "{text:?}");
        assert_eq!(tab.line_count(), count, "{text:?}");
        assert_eq!(e.active_line_count(), count, "{text:?}");
    }
}

#[test]
fn tab_title_and_display_path() {
    let mut e = ed("");
    let tab = e.active_tab_mut().unwrap();
    assert_eq!(tab.display_path(), "untitled");
    assert!(tab.title().contains("untitled"));
    let clean = tab.title();
    tab.dirty = true;
    assert_ne!(tab.title(), clean, "a dirty tab is flagged");
    tab.path = Some(PathBuf::from("/tmp/dir/main.rs"));
    assert_eq!(tab.display_path(), "/tmp/dir/main.rs");
    assert!(tab.title().contains("main.rs") && !tab.title().contains("dir"));
    assert!(!tab.is_remote() && !tab.is_image());
}

// ---- tabs -----------------------------------------------------------------

#[test]
fn new_tabs_append_and_take_focus() {
    let mut e = Editor::default();
    assert_eq!((e.tabs.len(), e.active), (1, 0));
    e.new_tab();
    assert_eq!((e.tabs.len(), e.active), (2, 1));
    assert!(!e.active_tab().unwrap().dirty);
    e.new_tab_with_content("{\"a\": 1}");
    let tab = e.active_tab().unwrap();
    assert_eq!((e.active, tab.dirty, tab.path.as_deref()), (2, true, None));
    assert_eq!(tab.text(), "{\"a\": 1}");
    e.new_tab_with_content_as("x", Some("response.json"));
    let tab = e.active_tab().unwrap();
    assert_eq!(tab.path, None, "a name hint never becomes a path");
    assert!(tab.title().contains("untitled"));
}

#[test]
fn next_and_prev_tab_wrap() {
    let mut e = Editor::default();
    e.new_tab();
    e.new_tab();
    assert_eq!(e.active, 2);
    e.next_tab();
    assert_eq!(e.active, 0);
    e.prev_tab();
    assert_eq!(e.active, 2);
    e.prev_tab();
    assert_eq!(e.active, 1);
    // With one tab both are no-ops.
    let mut one = Editor::default();
    one.next_tab();
    one.prev_tab();
    assert_eq!(one.active, 0);
}

#[test]
fn close_active_keeps_one_buffer_and_returns_the_path() {
    let mut e = Editor::default();
    assert_eq!(e.close_active(), None);
    assert_eq!(e.tabs.len(), 1, "closing the last tab leaves an empty one");
    e.active_tab_mut().unwrap().path = Some(PathBuf::from("a.txt"));
    e.new_tab();
    e.active_tab_mut().unwrap().path = Some(PathBuf::from("b.txt"));
    // Closing the last tab moves focus to the new last one.
    assert_eq!(e.close_active(), Some(PathBuf::from("b.txt")));
    assert_eq!((e.tabs.len(), e.active), (1, 0));
    assert_eq!(e.close_active(), Some(PathBuf::from("a.txt")));
    assert_eq!(e.tabs.len(), 1);
    assert_eq!(e.active_tab().unwrap().path, None);
}

#[test]
fn close_all_returns_paths_oldest_first() {
    let mut e = Editor::default();
    e.active_tab_mut().unwrap().path = Some(PathBuf::from("a"));
    e.new_tab(); // untitled: not returned
    e.new_tab();
    e.active_tab_mut().unwrap().path = Some(PathBuf::from("c"));
    assert_eq!(e.close_all(), vec![PathBuf::from("a"), PathBuf::from("c")]);
    assert_eq!((e.tabs.len(), e.active), (1, 0));
    assert_eq!(e.close_all(), Vec::<PathBuf>::new());
}

#[test]
fn promote_active_clears_preview() {
    let mut e = Editor::default();
    e.active_tab_mut().unwrap().preview = true;
    e.promote_active();
    assert!(!e.active_tab().unwrap().preview);
}

// ---- go to line / offset / percent / byte ---------------------------------

#[test]
fn goto_clamps_line_and_column() {
    // Lines: "ab" (0..2), "cdef" (3..7), "" (8).
    let text = "ab\ncdef\n";
    for (line, col, want) in [
        (1, None, 0),
        (2, None, 3),
        (2, Some(3), 5),
        (2, Some(5), 7),
        // Column 0 is treated as column 1.
        (2, Some(0), 3),
        // Line 0 clamps to the first line, a huge line to the last.
        (0, None, 0),
        (99, None, 8),
        (99, Some(9), 8),
        // A column past the line's end stays on that line (its end), rather
        // than running on into the following lines.
        (1, Some(99), 2),
        (2, Some(99), 7),
    ] {
        let mut e = ed(text);
        e.goto(line, col, AREA);
        assert_eq!(cur(&e), want, "goto({line}, {col:?})");
    }
    // Columns count chars, so a wide CJK line is addressed per character.
    let mut e = ed("日本語\nx");
    e.goto(1, Some(3), AREA);
    assert_eq!(cur(&e), 2);
    e.goto(1, Some(4), AREA);
    assert_eq!(cur(&e), 3);
    e.goto(1, Some(5), AREA);
    assert_eq!(cur(&e), 3, "past the end of the wide line");
    // An empty buffer has one line and one column.
    let mut empty = ed("");
    empty.goto(5, Some(5), AREA);
    assert_eq!(cur(&empty), 0);
}

#[test]
fn goto_offset_clamps_past_the_end() {
    let mut e = ed("ab\ncd");
    e.goto_offset(4, AREA);
    assert_eq!(cur(&e), 4);
    e.goto_offset(999, AREA);
    assert_eq!(cur(&e), 5);
}

#[test]
fn goto_percent_is_by_line_and_clamped() {
    // Five lines starting at 0, 2, 4, 6, 8.
    let text = "a\nb\nc\nd\ne";
    for (pct, want) in [
        (0, 0),
        (24, 0),
        (25, 2),
        (50, 4),
        (99, 6),
        (100, 8),
        (250, 8),
    ] {
        let mut e = ed_at(text, 3);
        e.goto_percent(pct, AREA);
        assert_eq!(cur(&e), want, "{pct}%");
    }
    let mut empty = ed("");
    empty.goto_percent(100, AREA);
    assert_eq!(cur(&empty), 0);
}

#[test]
fn goto_byte_lands_on_the_char_containing_the_byte() {
    // Bytes: a=0, é=1..3, 日=3..6, b=6; chars a=0 é=1 日=2 b=3.
    let text = "aé日b";
    for (byte, want) in [
        (0, 0),
        (1, 1),
        (2, 1),
        (3, 2),
        (4, 2),
        (5, 2),
        (6, 3),
        (7, 4),
        (999, 4),
    ] {
        let mut e = ed(text);
        e.goto_byte(byte, AREA);
        assert_eq!(cur(&e), want, "byte {byte}");
    }
}

// ---- line / document motions ----------------------------------------------

#[test]
fn smart_home_toggles_between_indent_and_column_zero() {
    for (text, at, want) in [
        ("    foo", 6, 4),
        ("    foo", 4, 0),
        ("    foo", 0, 4),
        ("    foo", 2, 4),
        ("\t\tbar", 4, 2),
        ("x", 1, 0),
        ("x", 0, 0),
        // A blank (whitespace-only) line has no first non-blank: column 0.
        ("   ", 2, 0),
        ("   ", 0, 0),
        ("", 0, 0),
        // Second line of a buffer.
        ("a\n  b", 5, 4),
        ("a\n  b", 4, 2),
        // Indentation is spaces and tabs only, not other whitespace.
        ("\u{3000}字", 2, 0),
    ] {
        let mut e = ed_at(text, at);
        e.cursor_line_home();
        assert_eq!(cur(&e), want, "home on {text:?} from {at}");
    }
}

#[test]
fn line_start_and_end_stay_on_the_line() {
    // Lines: "ab" 0..2, "日本語" 3..6, "" 7.
    let text = "ab\n日本語\n";
    for (at, start, end) in [
        (0, 0, 2),
        (2, 0, 2),
        (3, 3, 6),
        (4, 3, 6),
        (6, 3, 6),
        (7, 7, 7),
    ] {
        let mut e = ed_at(text, at);
        e.cursor_line_start();
        assert_eq!(cur(&e), start, "start from {at}");
        e.cursor_line_end();
        assert_eq!(cur(&e), end, "end from {at}");
    }
}

#[test]
fn document_start_and_end() {
    for (text, end) in [
        ("", 0),
        ("ab", 2),
        ("ab\ncd", 5),
        ("ab\n", 3),
        ("日本\n語", 4),
    ] {
        let mut e = ed_at(text, 1.min(end));
        e.cursor_document_end(AREA);
        assert_eq!(cur(&e), end, "end of {text:?}");
        e.cursor_document_start(AREA);
        assert_eq!(cur(&e), 0, "start of {text:?}");
    }
}

#[test]
fn line_down_and_up_stop_at_the_buffer_edges() {
    let text = "abc\nd\nefg";
    let mut e = ed_at(text, 2);
    e.cursor_line_up(AREA);
    assert_eq!(e.cursor_line(), 0, "up from the first line stays");
    e.cursor_line_down(AREA);
    assert_eq!(e.cursor_line(), 1);
    assert!(cur(&e) <= 5, "clamped into the short line");
    e.cursor_line_down(AREA);
    assert_eq!(e.cursor_line(), 2);
    e.cursor_line_down(AREA);
    assert_eq!(e.cursor_line(), 2, "down from the last line stays");
}

#[test]
fn page_up_and_down_move_whole_lines_and_clamp() {
    let text = (0..10)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let mut e = ed(&text);
    e.page_down(3);
    assert_eq!(e.cursor_line(), 3);
    e.page_down(100);
    assert_eq!(e.cursor_line(), 9);
    // A zero-line page still moves one line.
    e.page_up(0);
    assert_eq!(e.cursor_line(), 8);
    e.page_up(100);
    assert_eq!(e.cursor_line(), 0);
    e.page_up(1);
    assert_eq!(e.cursor_line(), 0);
}

// ---- paragraphs and sections ------------------------------------------------

/// Rows: 0 "a", 1 "b", 2 "", 3 "c", 4 "d", 5 "", 6 "", 7 "e"; row starts at
/// chars 0, 2, 4, 5, 7, 9, 10, 11; length 12. Paragraphs start at rows 0, 3, 7;
/// sections (split by the 2-blank run at rows 5–6) at rows 0 and 7.
const BLOCKS: &str = "a\nb\n\nc\nd\n\n\ne";

#[test]
fn paragraph_start_and_end() {
    for (at, start, end) in [
        (0, 0, 3),
        (3, 0, 3),
        (5, 5, 8),
        (8, 5, 8),
        (11, 11, 12),
        (12, 11, 12),
        // From a blank line, start climbs to the paragraph above and end
        // descends to the paragraph below.
        (4, 0, 8),
        (9, 5, 12),
    ] {
        let mut e = ed_at(BLOCKS, at);
        e.cursor_paragraph_start(AREA);
        assert_eq!(cur(&e), start, "paragraph start from {at}");
        let mut e = ed_at(BLOCKS, at);
        e.cursor_paragraph_end(AREA);
        assert_eq!(cur(&e), end, "paragraph end from {at}");
    }
}

#[test]
fn section_start_and_end() {
    for (at, start, end) in [(0, 0, 8), (4, 0, 8), (8, 0, 8), (11, 11, 12), (12, 11, 12)] {
        let mut e = ed_at(BLOCKS, at);
        e.cursor_section_start(AREA);
        assert_eq!(cur(&e), start, "section start from {at}");
        let mut e = ed_at(BLOCKS, at);
        e.cursor_section_end(AREA);
        assert_eq!(cur(&e), end, "section end from {at}");
    }
}

#[test]
fn paragraph_and_section_next_prev_and_number() {
    // (from, next, prev) paragraph starts; `None` means the cursor stays.
    for (at, next, prev) in [
        (0, Some(5), None),
        (3, Some(5), Some(0)),
        (5, Some(11), Some(0)),
        (11, None, Some(5)),
        (12, None, Some(5)),
    ] {
        let mut e = ed_at(BLOCKS, at);
        e.cursor_paragraph_next(AREA);
        assert_eq!(cur(&e), next.unwrap_or(at), "paragraph next from {at}");
        let mut e = ed_at(BLOCKS, at);
        e.cursor_paragraph_prev(AREA);
        assert_eq!(cur(&e), prev.unwrap_or(at), "paragraph prev from {at}");
    }
    for (n, want) in [(0, 0), (1, 0), (2, 5), (3, 11), (4, 6)] {
        let mut e = ed_at(BLOCKS, 6);
        e.goto_paragraph(n, AREA);
        assert_eq!(cur(&e), want, "paragraph {n}");
    }
    for (at, next, prev) in [(0, 11, 0), (5, 11, 0), (11, 11, 0)] {
        let mut e = ed_at(BLOCKS, at);
        e.cursor_section_next(AREA);
        assert_eq!(cur(&e), next, "section next from {at}");
        let mut e = ed_at(BLOCKS, at);
        e.cursor_section_prev(AREA);
        assert_eq!(cur(&e), prev, "section prev from {at}");
    }
    for (n, want) in [(1, 0), (2, 11), (3, 6)] {
        let mut e = ed_at(BLOCKS, 6);
        e.goto_section(n, AREA);
        assert_eq!(cur(&e), want, "section {n}");
    }
}

#[test]
fn block_motions_on_an_empty_buffer_stay_at_zero() {
    let mut e = ed("");
    e.cursor_paragraph_start(AREA);
    e.cursor_paragraph_end(AREA);
    e.cursor_section_start(AREA);
    e.cursor_section_end(AREA);
    e.cursor_paragraph_next(AREA);
    e.cursor_paragraph_prev(AREA);
    e.goto_paragraph(1, AREA);
    e.cursor_section_next(AREA);
    e.goto_section(1, AREA);
    assert_eq!(cur(&e), 0);
}

// ---- sentences and words ------------------------------------------------------

#[test]
fn sentence_motions() {
    // Sentences start at 0 "One", 5 "Two", 10 "Three", 18 "Four"; length 23.
    let text = "One. Two! Three?  Four.";
    for (at, start, end, next, prev) in [
        (0, 0, 4, 5, 0),
        (2, 0, 4, 5, 0),
        (5, 5, 9, 10, 0),
        (7, 5, 9, 10, 5),
        (17, 10, 16, 18, 10),
        (18, 18, 23, 18, 10),
        (23, 18, 23, 23, 18),
    ] {
        let run = |f: fn(&mut Editor, Rect)| {
            let mut e = ed_at(text, at);
            f(&mut e, AREA);
            cur(&e)
        };
        assert_eq!(run(Editor::cursor_sentence_start), start, "start from {at}");
        assert_eq!(run(Editor::cursor_sentence_end), end, "end from {at}");
        assert_eq!(run(Editor::cursor_sentence_next), next, "next from {at}");
        assert_eq!(run(Editor::cursor_sentence_prev), prev, "prev from {at}");
    }
    for (n, want) in [(0, 0), (1, 0), (4, 18), (5, 7)] {
        let mut e = ed_at(text, 7);
        e.goto_sentence(n, AREA);
        assert_eq!(cur(&e), want, "sentence {n}");
    }
    // Whitespace-only and empty buffers have no sentences.
    for blank in ["", "   "] {
        let mut e = ed(blank);
        e.cursor_sentence_end(AREA);
        assert_eq!(cur(&e), blank.chars().count());
        e.cursor_sentence_start(AREA);
        assert_eq!(cur(&e), 0);
    }
}

#[test]
fn word_motions() {
    // Words start at 0 "foo", 5 "bar_baz", 13 "日本", 16 "x"; length 17.
    let text = "foo  bar_baz 日本 x";
    for (at, start, end, next, prev) in [
        (0, 0, 3, 5, 0),
        (1, 0, 3, 5, 0),
        // Between words, start goes back to the previous word.
        (3, 0, 12, 5, 0),
        (7, 5, 12, 13, 5),
        (13, 13, 15, 16, 5),
        (16, 16, 17, 16, 13),
        (17, 16, 17, 17, 16),
    ] {
        let run = |f: fn(&mut Editor, Rect)| {
            let mut e = ed_at(text, at);
            f(&mut e, AREA);
            cur(&e)
        };
        assert_eq!(run(Editor::cursor_word_start), start, "start from {at}");
        assert_eq!(run(Editor::cursor_word_end), end, "end from {at}");
        assert_eq!(run(Editor::cursor_word_next), next, "next from {at}");
        assert_eq!(run(Editor::cursor_word_prev), prev, "prev from {at}");
    }
    for (n, want) in [(0, 0), (1, 0), (3, 13), (4, 16), (5, 7)] {
        let mut e = ed_at(text, 7);
        e.goto_word(n, AREA);
        assert_eq!(cur(&e), want, "word {n}");
    }
    // Leading non-word chars: word start before the first word is 0.
    let mut e = ed_at("..  ab", 1);
    e.cursor_word_start(AREA);
    assert_eq!(cur(&e), 0);
}

/// The spec defines a word as a run of alphanumerics and underscores, so a
/// combining mark (not alphanumeric) ends a word: `e` + U+0301 is two
/// "chars", and word end stops before the accent.
#[test]
fn word_motions_split_at_combining_marks() {
    let text = "cafe\u{301} x";
    let mut e = ed(text);
    e.cursor_word_end(AREA);
    assert_eq!(cur(&e), 4);
    e.cursor_word_next(AREA);
    assert_eq!(cur(&e), 6);
}

// ---- selections ---------------------------------------------------------------

#[test]
fn select_line_includes_the_newline() {
    for (text, at, want) in [
        ("ab\ncd", 0, Some((0, 3))),
        ("ab\ncd", 2, Some((0, 3))),
        ("ab\ncd", 4, Some((3, 5))),
        ("日本\n", 1, Some((0, 3))),
        // The trailing empty line and an empty buffer select nothing.
        ("ab\n", 3, None),
        ("", 0, None),
    ] {
        let mut e = ed_at(text, at);
        e.select_line(AREA);
        assert_eq!(sel(&mut e), want, "{text:?} at {at}");
        if let Some((_, end)) = want {
            assert_eq!(cur(&e), end, "caret at the selection end");
        }
    }
}

#[test]
fn select_paragraph_and_section() {
    for (at, para, section) in [
        (0, (0, 3), (0, 8)),
        (5, (5, 8), (0, 8)),
        (11, (11, 12), (11, 12)),
        // From the blank row 2, the paragraph selection spans both neighbors.
        (4, (0, 8), (0, 8)),
    ] {
        let mut e = ed_at(BLOCKS, at);
        e.select_paragraph(AREA);
        assert_eq!(sel(&mut e), Some(para), "paragraph from {at}");
        let mut e = ed_at(BLOCKS, at);
        e.select_section(AREA);
        assert_eq!(sel(&mut e), Some(section), "section from {at}");
    }
    let mut empty = ed("");
    empty.select_paragraph(AREA);
    empty.select_section(AREA);
    assert_eq!(sel(&mut empty), None);
}

#[test]
fn select_word_grows_and_shrinks_from_the_anchor() {
    let mut e = ed("foo bar 日本");
    e.select_word(true, AREA);
    assert_eq!((sel(&mut e), cur(&e)), (Some((0, 3)), 3));
    e.select_word(true, AREA);
    assert_eq!((sel(&mut e), cur(&e)), (Some((0, 7)), 7));
    e.select_word(true, AREA);
    assert_eq!((sel(&mut e), cur(&e)), (Some((0, 10)), 10));
    // At the end there is nothing further: unchanged.
    e.select_word(true, AREA);
    assert_eq!((sel(&mut e), cur(&e)), (Some((0, 10)), 10));
    // Shrinking keeps the anchor at 0.
    e.select_word(false, AREA);
    assert_eq!((sel(&mut e), cur(&e)), (Some((0, 8)), 8));
    // Backwards from the end of a fresh buffer.
    let mut e = ed_at("foo bar", 7);
    e.select_word(false, AREA);
    assert_eq!((sel(&mut e), cur(&e)), (Some((4, 7)), 4));
    // Nothing to the left of 0.
    let mut e = ed("foo");
    e.select_word(false, AREA);
    assert_eq!((sel(&mut e), cur(&e)), (None, 0));
}

#[test]
fn select_all_spans_the_buffer() {
    let mut e = ed_at("ab\n日本", 1);
    e.select_all();
    assert_eq!(sel(&mut e), Some((0, 5)));
}

#[test]
fn jump_matching_bracket_and_its_no_op() {
    let mut e = ed("(a [b] c)");
    e.jump_matching_bracket(AREA);
    let partner = cur(&e);
    assert!(partner >= 8, "jumped to the closing paren, got {partner}");
    let mut e = ed_at("abc", 1);
    e.jump_matching_bracket(AREA);
    assert_eq!(cur(&e), 1, "not on a bracket: no-op");
}

// ---- edits ----------------------------------------------------------------------

#[test]
fn insert_str_marks_dirty_promotes_and_undoes() {
    let mut e = ed("");
    e.active_tab_mut().unwrap().preview = true;
    assert!(e.insert_str("héllo 日本", AREA));
    let tab = e.active_tab().unwrap();
    assert_eq!(
        (tab.text(), tab.dirty, tab.preview, cur(&e)),
        ("héllo 日本".to_string(), true, false, 8)
    );
    let t = e.active_tab_mut().unwrap();
    t.editor.undo();
    assert_eq!(t.text(), "");
    t.editor.redo();
    assert_eq!(t.text(), "héllo 日本");
}

#[test]
fn insert_and_paste_refuse_a_read_only_buffer() {
    let mut e = ed("x");
    e.active_tab_mut().unwrap().read_only = true;
    assert!(!e.insert_str("y", AREA));
    assert!(!e.paste_str("y", AREA));
    let tab = e.active_tab().unwrap();
    assert_eq!((tab.text(), tab.dirty), ("x".to_string(), false));
}

#[test]
fn paste_str_replaces_the_selection_as_one_undo_step() {
    let mut e = ed("abcd");
    e.active_tab_mut().unwrap().editor.set_selection_range(1, 3);
    assert!(e.paste_str("X\nY", AREA));
    assert_eq!(text(&e), "aX\nYd");
    assert!(e.active_tab().unwrap().dirty);
    let t = e.active_tab_mut().unwrap();
    t.editor.undo();
    assert_eq!(t.text(), "abcd", "one undo reverts the whole paste");
}

#[test]
fn delete_forward_deletes_the_char_ahead() {
    for (text, at, want, cursor) in [
        ("abc", 0, "bc", 0),
        ("abc", 1, "ac", 1),
        // At the very end there is nothing ahead: no backspace instead.
        ("abc", 3, "abc", 3),
        ("", 0, "", 0),
        // At a line end it joins the next line.
        ("ab\ncd", 2, "abcd", 2),
        ("日本", 0, "本", 0),
        // Inside indentation it removes one char, not the whole indent.
        ("    x", 0, "   x", 0),
        ("    x", 2, "   x", 2),
    ] {
        let mut e = ed_at(text, at);
        e.delete_forward();
        assert_eq!(
            (self::text(&e), cur(&e)),
            (want.to_string(), cursor),
            "{text:?} at {at}"
        );
    }
}

#[test]
fn delete_forward_deletes_a_pending_selection() {
    for (text, anchor, caret, want) in [
        ("abcd", 0, 2, "cd"),
        ("abcd", 2, 0, "cd"),
        // A selection that reaches the end of the buffer.
        ("abcd", 1, 4, "a"),
        ("日本語", 0, 3, ""),
    ] {
        let mut e = ed(text);
        e.active_tab_mut()
            .unwrap()
            .editor
            .set_selection_range(anchor, caret);
        e.delete_forward();
        assert_eq!(self::text(&e), want, "{text:?} [{anchor}, {caret})");
        assert_eq!(sel(&mut e), None);
    }
}

#[test]
fn delete_forward_undoes() {
    let mut e = ed_at("abc", 1);
    e.delete_forward();
    let t = e.active_tab_mut().unwrap();
    assert_eq!(t.text(), "ac");
    t.editor.undo();
    assert_eq!(t.text(), "abc");
    t.editor.redo();
    assert_eq!(t.text(), "ac");
}

#[test]
fn whole_buffer_line_edits() {
    type Op = fn(&mut Editor);
    let cases: [(&str, Op, &str, &str); 6] = [
        ("sort", Editor::sort_lines, "b\na\nc", "a\nb\nc"),
        ("reverse", Editor::reverse_lines, "a\nb\nc", "c\nb\na"),
        ("sort unique", Editor::sort_unique, "b\na\nb", "a\nb"),
        ("dedup", Editor::remove_duplicate_lines, "a\nb\na", "a\nb"),
        ("trim", Editor::trim_trailing_whitespace, "a  \nb\t", "a\nb"),
        ("sort cjk", Editor::sort_lines, "日\nあ\nA", "A\nあ\n日"),
    ];
    for (name, op, input, want) in cases {
        let mut e = ed(input);
        e.active_tab_mut().unwrap().preview = true;
        op(&mut e);
        let tab = e.active_tab().unwrap();
        assert_eq!(tab.text(), want, "{name}");
        assert!(tab.dirty && !tab.preview, "{name} marks dirty and promotes");
        let t = e.active_tab_mut().unwrap();
        t.editor.undo();
        assert_eq!(t.text(), input, "{name} undoes in one step");
    }
}

#[test]
fn shuffle_lines_keeps_every_line() {
    let input = "1\n2\n3\n4\n5\n6\n7\n8";
    let mut e = ed(input);
    e.shuffle_lines();
    let mut lines = e.active_tab().unwrap().lines();
    lines.sort();
    assert_eq!(lines.join("\n"), input);
    assert!(e.active_tab().unwrap().dirty);
}

#[test]
fn duplicate_and_join_lines() {
    let mut e = ed_at("ab\ncd", 1);
    e.duplicate_line();
    assert_eq!(text(&e), "ab\nab\ncd");
    assert!(e.active_tab().unwrap().dirty);
    let mut e = ed("ab\ncd");
    e.join_lines();
    let joined = text(&e);
    assert_eq!(
        joined.lines().count(),
        1,
        "joined into one line: {joined:?}"
    );
    assert!(joined.starts_with("ab") && joined.ends_with("cd"));
    // Joining the last line has nothing to pull up.
    let mut e = ed_at("ab", 1);
    e.join_lines();
    assert_eq!(text(&e), "ab");
}

#[test]
fn move_line_swaps_rows_and_stops_at_the_edges() {
    let mut e = ed("a\nb\nc");
    e.move_line(true, AREA);
    assert_eq!(text(&e), "b\na\nc");
    assert_eq!(e.cursor_line(), 1);
    e.move_line(false, AREA);
    assert_eq!(text(&e), "a\nb\nc");
    assert_eq!(e.cursor_line(), 0);
    e.move_line(false, AREA);
    assert_eq!(text(&e), "a\nb\nc", "the first line can't move up");
    let mut e = ed_at("a\nb\nc", 4);
    e.move_line(true, AREA);
    assert_eq!(text(&e), "a\nb\nc", "the last line can't move down");
}

#[test]
fn set_word_marks_and_queries_without_a_buffer_are_safe() {
    let mut e = ed("abc");
    e.set_word_marks(vec![(0, 3)]);
    assert_eq!(e.top_visible_line(), 0);
    assert!(!e.switch_undo_branch(), "a fresh buffer has one branch");
    // With every tab gone (a host's transient state), queries fall back.
    e.tabs.clear();
    assert_eq!(e.cursor_1based(), (1, 1));
    assert_eq!(e.active_line_count(), 1);
    assert_eq!((e.cursor_line(), e.top_visible_line()), (0, 0));
    assert!(!e.insert_str("x", AREA) && !e.paste_str("x", AREA));
    assert!(!e.poll_parse() && !e.parse_pending());
    e.goto(3, None, AREA);
    e.delete_forward();
    e.cursor_word_next(AREA);
    e.next_tab();
    e.prev_tab();
    assert_eq!(e.close_active(), None);
}

// ---- split panes --------------------------------------------------------------

#[test]
fn split_shows_another_tab_and_focus_wraps() {
    let mut e = Editor::default();
    assert!(!e.is_split());
    assert_eq!(e.split_layout(AREA), vec![]);
    assert_eq!(e.split_dividers(AREA), vec![]);
    assert!(!e.focus_pane_at(AREA, 1, 1));
    assert!(!e.resize_split_at(AREA, 40, 1));
    // One tab: both panes show it.
    e.set_split(SplitDir::Vertical);
    let tabs: Vec<usize> = e.split_layout(AREA).iter().map(|b| b.tab).collect();
    assert_eq!(tabs, vec![0, 0]);
    e.unsplit();
    // Two tabs, the second active: it stays in the focused first pane.
    e.new_tab();
    e.set_split(SplitDir::Vertical);
    let boxes = e.split_layout(AREA);
    assert_eq!(boxes.iter().map(|b| b.tab).collect::<Vec<_>>(), vec![1, 0]);
    assert_eq!((e.focused_leaf(), e.active), (0, 1));
    e.focus_other_pane();
    assert_eq!((e.focused_leaf(), e.active), (1, 0));
    e.focus_other_pane();
    assert_eq!((e.focused_leaf(), e.active), (0, 1));
    // A click in the right pane focuses it.
    let right = boxes[1].rect;
    assert!(e.focus_pane_at(AREA, right.x + 1, right.y + 1));
    assert_eq!((e.focused_leaf(), e.active), (1, 0));
    // Dragging the divider resizes.
    let (dir, div) = e.split_dividers(AREA)[0];
    assert_eq!(dir, SplitDir::Vertical);
    assert!(e.resize_split_at(AREA, div.x, div.y));
    assert!(!e.resize_split_at(AREA, 0, 0));
}

#[test]
fn splits_cap_at_four_panes_and_unsplit_collapses() {
    let mut e = Editor::default();
    e.new_tab();
    for _ in 0..6 {
        e.set_split(SplitDir::Horizontal);
    }
    assert_eq!(e.split_layout(AREA).len(), 4);
    e.focus_leaf(3);
    assert_eq!(e.focused_leaf(), 3);
    // Focusing a leaf that doesn't exist changes nothing.
    e.focus_leaf(9);
    assert_eq!(e.focused_leaf(), 3);
    e.unsplit();
    assert_eq!(e.split_layout(AREA).len(), 3);
    assert!(e.focused_leaf() <= 2);
    e.unsplit();
    e.unsplit();
    assert!(!e.is_split());
    assert_eq!(e.focused_leaf(), 0);
    assert!(e.active < e.tabs.len());
    // Unsplitting an unsplit editor is a no-op.
    e.unsplit();
    assert!(!e.is_split());
}

#[test]
fn split_layout_clamps_panes_to_closed_tabs() {
    let mut e = Editor::default();
    e.new_tab();
    e.new_tab();
    e.set_split(SplitDir::Vertical);
    e.focus_other_pane();
    // Close tabs until only one remains: every pane falls back to tab 0.
    e.close_active();
    e.close_active();
    let tabs: Vec<usize> = e.split_layout(AREA).iter().map(|b| b.tab).collect();
    assert!(tabs.iter().all(|&t| t < e.tabs.len()), "{tabs:?}");
    e.unsplit();
    assert!(e.active < e.tabs.len());
}

// ---- open / save ----------------------------------------------------------------

#[test]
fn open_focuses_an_open_file_and_reuses_the_preview_tab() {
    let dir = scratch("open");
    let (a, b) = (dir.join("a.txt"), dir.join("b.txt"));
    fs::write(&a, "A\n").unwrap();
    fs::write(&b, "B\n").unwrap();
    let mut e = Editor::default();
    e.open(&a, true).unwrap();
    assert_eq!((e.tabs.len(), e.active), (2, 1));
    assert!(e.active_tab().unwrap().preview);
    // The next preview replaces the preview tab instead of adding one.
    e.open(&b, true).unwrap();
    assert_eq!((e.tabs.len(), text(&e)), (2, "B\n".to_string()));
    // Opening it for real (through a non-canonical path) promotes, no new tab.
    e.open(&dir.join(".").join("b.txt"), false).unwrap();
    assert_eq!(e.tabs.len(), 2);
    assert!(!e.active_tab().unwrap().preview);
    e.open(&a, false).unwrap();
    assert_eq!(e.tabs.len(), 3);
    e.open(&b, true).unwrap();
    assert_eq!((e.tabs.len(), e.active), (3, 1), "an open file is focused");
    // A missing file is an error and opens nothing.
    let err = e.open(&dir.join("missing.txt"), false).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotFound);
    assert_eq!(e.tabs.len(), 3);
    // A file over the limit is refused until the user confirms.
    e.max_text_bytes = 2;
    let big = dir.join("big.txt");
    fs::write(&big, "0123456789").unwrap();
    assert!(e.open(&big, false).is_err());
    e.open_unbounded(&big, false).unwrap();
    assert_eq!(text(&e), "0123456789");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_options_normalize_the_written_text() {
    let dir = scratch("save-opts");
    for (input, trim, final_nl, want) in [
        ("a  \nb\t", true, false, "a\nb"),
        ("a  \nb\t", false, true, "a  \nb\t\n"),
        ("a  \nb\t", true, true, "a\nb\n"),
        ("x\n", false, true, "x\n"),
        ("", true, true, ""),
        // CRLF lines keep their `\r`, and the spaces before it still go.
        ("a  \r\nb \t\r\n", true, false, "a\r\nb\r\n"),
        ("日本 \u{3000}\n", true, false, "日本 \u{3000}\n"),
    ] {
        let mut e = ed(input);
        let path = dir.join("out.txt");
        let opts = SaveOptions {
            trim_trailing_whitespace: trim,
            ensure_final_newline: final_nl,
        };
        assert_eq!(e.save_active_as(path.clone(), opts).unwrap(), path);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            want,
            "{input:?} trim={trim} final_nl={final_nl}"
        );
        // The buffer itself is not rewritten by the save.
        assert_eq!(text(&e), input);
    }
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_as_adopts_the_path_and_save_needs_one() {
    let dir = scratch("save");
    let mut e = ed("hi");
    let err = e.save_active(PLAIN).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotFound);
    let err = e.save_active_overwriting(PLAIN).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotFound);
    let t = e.active_tab_mut().unwrap();
    t.dirty = true;
    t.preview = true;
    let path = dir.join("hi.txt");
    e.save_active_as(path.clone(), PLAIN).unwrap();
    let tab = e.active_tab().unwrap();
    assert_eq!(
        (tab.path.as_deref(), tab.dirty, tab.preview),
        (Some(path.as_path()), false, false)
    );
    e.insert_str("!", AREA);
    assert_eq!(e.save_active(PLAIN).unwrap(), path);
    assert_eq!(fs::read_to_string(&path).unwrap(), "!hi");
    // A failing write keeps the buffer dirty and its old path.
    e.insert_str("?", AREA);
    let bad = dir.join("no-such-dir").join("x.txt");
    assert!(e.save_active_as(bad, PLAIN).is_err());
    let tab = e.active_tab().unwrap();
    assert_eq!(
        (tab.path.as_deref(), tab.dirty),
        (Some(path.as_path()), true)
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn reopen_with_encoding_needs_a_file() {
    let mut e = ed("x");
    let err = e
        .reopen_active_with_encoding(vix_encoding::TextEncoding::UTF8)
        .unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotFound);
}

#[test]
fn reload_skips_dirty_buffers_and_clamps_the_cursor() {
    let dir = scratch("reload");
    let (a, b) = (dir.join("a.txt"), dir.join("b.txt"));
    fs::write(&a, "a long first version\n").unwrap();
    fs::write(&b, "b\n").unwrap();
    let mut e = Editor::default();
    e.open(&a, false).unwrap();
    e.active_tab_mut().unwrap().editor.set_cursor(15);
    e.open(&b, false).unwrap();
    e.insert_str("edited ", AREA);
    fs::write(&a, "short\n").unwrap();
    fs::write(&b, "theirs\n").unwrap();
    assert_eq!(e.reload_clean_from_disk(), 1);
    assert_eq!(e.tabs[1].text(), "short\n");
    assert!(
        e.tabs[1].editor.get_cursor() <= 6,
        "cursor clamped into the new text"
    );
    assert_eq!(e.tabs[2].text(), "edited b\n", "unsaved edits are kept");
    // Nothing changed since: nothing reloads.
    assert_eq!(e.reload_clean_from_disk(), 0);
    fs::remove_dir_all(&dir).ok();
}

// ---- properties -----------------------------------------------------------------

/// A short buffer mixing ASCII, accents, wide CJK, combining marks, blank
/// lines and CRLF.
fn buffer() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop_oneof![
            Just("a"),
            Just("Zz_9"),
            Just(" "),
            Just("\t"),
            Just("\n"),
            Just("\n\n"),
            Just("\r\n"),
            Just(". "),
            Just("é"),
            Just("e\u{301}"),
            Just("日本"),
            Just("("),
            Just(")"),
        ],
        0..24,
    )
    .prop_map(|parts| parts.concat())
}

/// Every cursor motion, as an index into a fixed table.
fn motion(e: &mut Editor, op: u8, n: usize) {
    match op % 24 {
        0 => e.goto(n, Some(n), AREA),
        1 => e.goto_offset(n, AREA),
        2 => e.goto_percent(n, AREA),
        3 => e.goto_byte(n, AREA),
        4 => e.cursor_line_home(),
        5 => e.cursor_line_end(),
        6 => e.cursor_line_start(),
        7 => e.cursor_document_start(AREA),
        8 => e.cursor_document_end(AREA),
        9 => e.cursor_paragraph_start(AREA),
        10 => e.cursor_paragraph_end(AREA),
        11 => e.cursor_section_start(AREA),
        12 => e.cursor_section_end(AREA),
        13 => e.cursor_paragraph_next(AREA),
        14 => e.cursor_section_prev(AREA),
        15 => e.goto_paragraph(n, AREA),
        16 => e.cursor_sentence_end(AREA),
        17 => e.cursor_sentence_prev(AREA),
        18 => e.goto_sentence(n, AREA),
        19 => e.cursor_word_end(AREA),
        20 => e.cursor_word_prev(AREA),
        21 => e.goto_word(n, AREA),
        22 => e.page_down(n % 5),
        _ => e.page_up(n % 5),
    }
}

proptest! {
    /// No motion moves the cursor outside the buffer or past its line's end.
    #[test]
    fn motions_keep_the_cursor_in_bounds(
        text in buffer(),
        ops in proptest::collection::vec((any::<u8>(), 0usize..40), 0..16),
    ) {
        let mut e = ed(&text);
        for (op, n) in ops {
            motion(&mut e, op, n);
            let code = e.active_tab().unwrap().editor.code_ref();
            let c = cur(&e);
            prop_assert!(c <= code.len_chars(), "cursor {c} past the end of {text:?}");
            let row = code.char_to_line(c);
            prop_assert!(c <= code.line_to_char(row) + code.line_len(row));
        }
    }

    /// Inserting text and undoing it restores the buffer exactly; deleting
    /// forward never grows it.
    #[test]
    fn insert_then_undo_restores_the_text(
        text in buffer(),
        insert in buffer(),
        at in 0usize..40,
    ) {
        let mut e = ed(&text);
        e.goto_offset(at, AREA);
        if !insert.is_empty() {
            e.insert_str(&insert, AREA);
            e.active_tab_mut().unwrap().editor.undo();
            prop_assert_eq!(self::text(&e), text);
        }
        let before = self::text(&e).chars().count();
        e.delete_forward();
        prop_assert!(self::text(&e).chars().count() <= before);
    }
}
