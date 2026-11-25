//! Viewer behavior beyond the happy path: selection, goto targets, searches in
//! both directions, notices, scrolling keys and the status numbers.
#![allow(clippy::unwrap_used, reason = "tests fail loudly on a broken fixture")]

use std::io::Write as _;
use std::path::PathBuf;

use vix_input::{KeyCode, KeyEvent, KeyModifiers};
use vix_large_file::{InputKind, Notice, Outcome, Target, Viewer};

struct TempFile(PathBuf);

impl TempFile {
    fn new(tag: &str, bytes: &[u8]) -> Self {
        let path =
            std::env::temp_dir().join(format!("vix-large-file-more-{tag}-{}", std::process::id()));
        std::fs::File::create(&path)
            .unwrap()
            .write_all(bytes)
            .unwrap();
        Self(path)
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn log_lines(n: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for i in 1..=n {
        writeln!(out, "line {i:06} the quick brown fox jumps over the dog").unwrap();
    }
    out
}

fn line_len() -> u64 {
    log_lines(1).len() as u64
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn press(v: &mut Viewer, code: KeyCode) -> Outcome {
    v.handle_key(key(code), 10)
}

fn ctrl(v: &mut Viewer, c: char) -> Outcome {
    v.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL), 10)
}

fn type_str(v: &mut Viewer, s: &str) {
    for c in s.chars() {
        press(v, KeyCode::Char(c));
    }
}

fn open(tag: &str, bytes: &[u8]) -> (TempFile, Viewer) {
    let file = TempFile::new(tag, bytes);
    let mut v = Viewer::open(&file.0).unwrap();
    v.index_mut().wait();
    (file, v)
}

fn settle(v: &mut Viewer) {
    for _ in 0..1_000_000 {
        if v.search().is_none() {
            return;
        }
        v.poll();
        std::thread::yield_now();
    }
    panic!("search did not finish");
}

#[test]
fn accessors_report_a_finished_index_and_a_quiet_viewer() {
    let (file, v) = open("acc", &log_lines(30));
    assert_eq!(v.path(), file.0.as_path());
    assert!(v.index().is_done());
    assert_eq!(v.index().total_lines(), Some(30));
    assert_eq!((v.cursor(), v.top()), (0, 0));
    assert!(v.input().is_none() && v.notice().is_none() && v.search().is_none());
    assert!(!v.busy());
}

#[test]
fn cursor_moves_by_lines_and_stops_at_the_ends() {
    let (_f, mut v) = open("move", &log_lines(30));
    press(&mut v, KeyCode::Up);
    assert_eq!(v.cursor(), 0, "up at the top stays");
    press(&mut v, KeyCode::Down);
    press(&mut v, KeyCode::Char('j'));
    assert_eq!(v.cursor(), 2 * line_len());
    press(&mut v, KeyCode::Char('k'));
    assert_eq!(v.cursor(), line_len());
    press(&mut v, KeyCode::End);
    assert_eq!(v.cursor(), 29 * line_len());
    press(&mut v, KeyCode::Down);
    assert_eq!(v.cursor(), 29 * line_len(), "down at the end stays");
    press(&mut v, KeyCode::Home);
    assert_eq!((v.cursor(), v.top()), (0, 0));
    press(&mut v, KeyCode::Char('G'));
    assert_eq!(v.cursor(), 29 * line_len());
    press(&mut v, KeyCode::Char('g'));
    assert_eq!(v.cursor(), 0);
}

#[test]
fn page_keys_move_a_screenful_less_one() {
    let (_f, mut v) = open("page", &log_lines(100));
    v.handle_key(key(KeyCode::PageDown), 10);
    assert_eq!(v.cursor(), 9 * line_len());
    v.handle_key(key(KeyCode::PageDown), 10);
    assert_eq!(v.cursor(), 18 * line_len());
    v.handle_key(key(KeyCode::PageUp), 10);
    assert_eq!(v.cursor(), 9 * line_len());
    assert!(v.top() <= v.cursor());
}

#[test]
fn horizontal_scroll_steps_and_resets() {
    let (_f, mut v) = open("hs", &log_lines(5));
    press(&mut v, KeyCode::Right);
    let step = v.status().hscroll;
    assert!(step > 0);
    press(&mut v, KeyCode::Char('l'));
    assert_eq!(v.status().hscroll, 2 * step);
    press(&mut v, KeyCode::Left);
    assert_eq!(v.status().hscroll, step);
    press(&mut v, KeyCode::Char('h'));
    press(&mut v, KeyCode::Char('h'));
    assert_eq!(v.status().hscroll, 0, "never below zero");
    press(&mut v, KeyCode::Right);
    press(&mut v, KeyCode::Char('0'));
    assert_eq!(v.status().hscroll, 0);
}

#[test]
fn selection_marks_toggle_extend_and_clear() {
    let (_f, mut v) = open("sel", &log_lines(10));
    press(&mut v, KeyCode::Char('m'));
    assert!(v.status().selecting);
    press(&mut v, KeyCode::Char(' '));
    assert!(!v.status().selecting, "space toggles it off again");
    v.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT), 10);
    assert!(v.status().selecting, "shift+down starts a selection");
    let rows = v.view(10, 80);
    assert!(rows[0].selected && rows[1].selected && !rows[2].selected);
    assert!(rows[1].cursor && !rows[0].cursor);
    assert_eq!(press(&mut v, KeyCode::Esc), Outcome::Consumed);
    assert!(!v.status().selecting, "first Esc only clears the selection");
    assert_eq!(press(&mut v, KeyCode::Esc), Outcome::Close);
}

#[test]
fn copy_returns_the_selected_lines_and_notices_the_size() {
    let (_f, mut v) = open("copy", &log_lines(10));
    press(&mut v, KeyCode::Char('m'));
    press(&mut v, KeyCode::Down);
    press(&mut v, KeyCode::Down);
    let Outcome::Copy(text) = press(&mut v, KeyCode::Char('y')) else {
        panic!("y copies");
    };
    assert_eq!(text.as_bytes(), &log_lines(3)[..]);
    assert_eq!(v.notice(), Some(&Notice::Copied(3 * line_len())));
    assert!(!v.status().selecting, "copying clears the selection");
    // Without a selection, `c` copies just the cursor line; Ctrl+C the same.
    let Outcome::Copy(one) = press(&mut v, KeyCode::Char('c')) else {
        panic!("c copies");
    };
    assert!(one.starts_with("line 000003"));
    assert_eq!(one.len() as u64, line_len());
    assert!(matches!(ctrl(&mut v, 'c'), Outcome::Copy(_)));
    assert_eq!(press(&mut v, KeyCode::Char('q')), Outcome::Close);
}

#[test]
fn copying_more_than_the_limit_is_refused() {
    let file = TempFile::new("big", &[]);
    let f = std::fs::OpenOptions::new()
        .write(true)
        .open(&file.0)
        .unwrap();
    f.set_len(vix_large_file::COPY_LIMIT + 1024).unwrap();
    let mut v = Viewer::open(&file.0).unwrap();
    assert_eq!(press(&mut v, KeyCode::Char('y')), Outcome::Consumed);
    assert_eq!(v.notice(), Some(&Notice::CopyTooLarge));
}

#[test]
fn goto_input_line_edits_and_targets() {
    let (_f, mut v) = open("goto", &log_lines(300));
    press(&mut v, KeyCode::Char(':'));
    assert_eq!(v.input(), Some((InputKind::Goto, "")));
    type_str(&mut v, "12x");
    press(&mut v, KeyCode::Backspace);
    assert_eq!(v.input(), Some((InputKind::Goto, "12")));
    // Ctrl chars are not typed into the line, other keys are ignored.
    ctrl(&mut v, 'x');
    press(&mut v, KeyCode::Tab);
    assert_eq!(v.input(), Some((InputKind::Goto, "12")));
    press(&mut v, KeyCode::Enter);
    assert!(v.input().is_none());
    assert_eq!(v.cursor(), 11 * line_len());
    assert_eq!(v.status().line, Some(12));

    ctrl(&mut v, 'g');
    assert_eq!(v.input().map(|(k, _)| k), Some(InputKind::Goto));
    press(&mut v, KeyCode::Esc);
    assert!(v.input().is_none(), "Esc abandons the input");
    assert_eq!(v.cursor(), 11 * line_len());

    press(&mut v, KeyCode::Char(':'));
    type_str(&mut v, "nope");
    press(&mut v, KeyCode::Enter);
    assert_eq!(v.notice(), Some(&Notice::BadGoto));

    press(&mut v, KeyCode::Char(':'));
    type_str(&mut v, "9999");
    press(&mut v, KeyCode::Enter);
    assert_eq!(v.notice(), Some(&Notice::BeyondEnd(300)));
    // The next ordinary key clears the notice.
    press(&mut v, KeyCode::Char('j'));
    assert!(v.notice().is_none());
}

#[test]
fn goto_percent_and_byte_targets_land_on_the_right_line() {
    let (_f, mut v) = open("pct", &log_lines(100));
    v.goto(Target::Percent(5000));
    assert_eq!(v.cursor(), 50 * line_len());
    v.goto(Target::Percent(0));
    assert_eq!(v.cursor(), 0);
    v.goto(Target::Byte(3 * line_len() + 7));
    assert_eq!(v.cursor(), 3 * line_len(), "the line holding the byte");
    v.goto(Target::Byte(u64::MAX));
    assert_eq!(v.cursor(), 99 * line_len(), "past the end: the last line");
    v.goto(Target::Line(0));
    assert_eq!(v.cursor(), 0, "line zero clamps to the first");
}

#[test]
fn forward_and_backward_searches_repeat_in_their_direction() {
    let (_f, mut v) = open("srch", &log_lines(100));
    press(&mut v, KeyCode::Char('/'));
    type_str(&mut v, "000040");
    press(&mut v, KeyCode::Enter);
    settle(&mut v);
    assert_eq!(v.cursor(), 39 * line_len());
    let hit_row = v
        .view(10, 80)
        .into_iter()
        .find(|r| r.cursor)
        .expect("cursor row visible");
    assert_eq!(hit_row.hit, Some((5, 11)), "the match is highlighted");

    // `n` with nothing further forward reports not found.
    press(&mut v, KeyCode::Char('n'));
    settle(&mut v);
    assert_eq!(v.notice(), Some(&Notice::NotFound));

    // `N` goes the other way from the same place: nothing before it either.
    press(&mut v, KeyCode::Char('N'));
    settle(&mut v);
    assert_eq!(v.notice(), Some(&Notice::NotFound));

    // Backward search from the end finds the last "fox" line and then the one before.
    press(&mut v, KeyCode::End);
    press(&mut v, KeyCode::Char('?'));
    type_str(&mut v, "line 0000");
    press(&mut v, KeyCode::Enter);
    settle(&mut v);
    // "line 000100" does not match, so line 99 is the last hit.
    assert_eq!(v.cursor(), 98 * line_len());
    press(&mut v, KeyCode::Char('n'));
    settle(&mut v);
    assert_eq!(v.cursor(), 97 * line_len(), "n repeats backward");
    press(&mut v, KeyCode::Char('N'));
    settle(&mut v);
    assert_eq!(v.cursor(), 98 * line_len(), "N goes forward");
}

#[test]
fn empty_search_and_repeat_without_history_do_nothing() {
    let (_f, mut v) = open("empty", &log_lines(5));
    press(&mut v, KeyCode::Char('n'));
    assert!(v.search().is_none());
    press(&mut v, KeyCode::Char('/'));
    press(&mut v, KeyCode::Enter);
    assert!(v.search().is_none() && v.input().is_none());
}

#[test]
fn escape_cancels_a_running_search() {
    let (_f, mut v) = open("cancel", &log_lines(20_000));
    press(&mut v, KeyCode::Char('/'));
    type_str(&mut v, "never present anywhere");
    press(&mut v, KeyCode::Enter);
    if v.search().is_some() {
        assert!(v.busy());
        assert!(v.status().searching.is_some());
        assert_eq!(press(&mut v, KeyCode::Esc), Outcome::Consumed);
        assert_eq!(v.notice(), Some(&Notice::SearchCancelled));
        assert!(v.search().is_none());
    } else {
        // The search finished before Esc could land; its answer is "not found".
        assert_eq!(v.notice(), Some(&Notice::NotFound));
    }
}

#[test]
fn an_unfound_search_leaves_the_cursor_and_says_so() {
    let (_f, mut v) = open("nf", &log_lines(50));
    press(&mut v, KeyCode::Char('/'));
    type_str(&mut v, "zebra");
    press(&mut v, KeyCode::Enter);
    settle(&mut v);
    assert_eq!(v.notice(), Some(&Notice::NotFound));
    assert_eq!(v.cursor(), 0);
}

#[test]
fn a_long_line_scrolls_horizontally_to_show_a_far_match() {
    let mut text = vec![b'a'; 500];
    text.extend_from_slice(b"NEEDLE");
    text.push(b'\n');
    text.extend_from_slice(b"short\n");
    let (_f, mut v) = open("far", &text);
    let rows = v.view(5, 40);
    assert!(rows[0].more_right, "the long line is cut off");
    press(&mut v, KeyCode::Char('/'));
    type_str(&mut v, "NEEDLE");
    press(&mut v, KeyCode::Enter);
    settle(&mut v);
    assert!(v.status().hscroll > 0, "scrolled right to reveal the match");
    let rows = v.view(5, 40);
    assert!(rows[0].text.contains("NEEDLE"));
    assert!(rows[0].hit.is_some());
}

#[test]
fn status_reports_position_and_progress() {
    let (_f, mut v) = open("stat", &log_lines(100));
    let s = v.status();
    assert_eq!(
        (s.line, s.total_lines, s.indexed),
        (Some(1), Some(100), 100)
    );
    assert_eq!(s.len, 100 * line_len());
    assert_eq!(s.percent, 0);
    assert_eq!(s.searching, None);
    press(&mut v, KeyCode::End);
    let s = v.status();
    assert_eq!(s.line, Some(100));
    assert!(s.percent >= 98);
    // Asking twice at the same cursor uses the cached line number.
    assert_eq!(v.status().line, Some(100));
}

#[test]
fn empty_file_is_viewable() {
    let (_f, mut v) = open("zero", b"");
    assert!(v.view(5, 20).iter().all(|r| r.text.is_empty()));
    assert_eq!(v.status().len, 0);
    press(&mut v, KeyCode::End);
    press(&mut v, KeyCode::PageDown);
    assert_eq!(v.cursor(), 0);
}

#[test]
fn opening_a_directory_or_missing_file_is_an_error() {
    assert!(Viewer::open(&std::env::temp_dir()).is_err());
    assert!(Viewer::open(&std::env::temp_dir().join("vix-large-file-missing-xyz")).is_err());
}
