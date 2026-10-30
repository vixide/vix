//! The Large File Viewer end to end over a generated multi-megabyte file.
#![allow(clippy::unwrap_used, reason = "tests fail loudly on a broken fixture")]

use std::io::Write as _;
use std::path::PathBuf;

use vix_input::{KeyCode, KeyEvent, KeyModifiers};
use vix_large_file::{InputKind, Notice, Outcome, Target, Viewer};

/// A file in the temp directory, removed on drop.
struct TempFile(PathBuf);

impl TempFile {
    fn new(tag: &str, bytes: &[u8]) -> Self {
        let path =
            std::env::temp_dir().join(format!("vix-large-file-it-{tag}-{}", std::process::id()));
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

/// `n` numbered log lines: `line 000001 ...`, about 60 bytes each.
fn log_lines(n: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(n * 60);
    for i in 1..=n {
        writeln!(out, "line {i:06} the quick brown fox jumps over the dog").unwrap();
    }
    out
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn type_str(v: &mut Viewer, s: &str) {
    for c in s.chars() {
        v.handle_key(key(KeyCode::Char(c)), 20);
    }
}

/// Wait for a running search to finish and apply its result.
fn settle(v: &mut Viewer) {
    for _ in 0..10_000 {
        if v.search().is_none() {
            return;
        }
        v.poll();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("search did not finish");
}

#[test]
fn opens_scrolls_and_jumps_in_a_multi_megabyte_file() {
    // ~6 MB, 100,000 lines: several index chunks and many strides.
    let file = TempFile::new("nav", &log_lines(100_000));
    let mut v = Viewer::open(&file.0).unwrap();
    let rows = v.view(20, 80);
    assert_eq!(rows.len(), 20);
    assert!(rows[0].text.starts_with("line 000001"));
    assert!(rows[0].cursor);

    // Goto end works before the index is done: it reads backward from EOF.
    v.handle_key(key(KeyCode::End), 20);
    let rows = v.view(20, 80);
    assert!(rows.last().unwrap().text.starts_with("line 100000"));
    assert!(rows.last().unwrap().cursor);

    v.index_mut().wait();
    assert_eq!(v.index().total_lines(), Some(100_000));
    assert_eq!(v.status().line, Some(100_000));

    v.goto(Target::Line(54_321));
    let rows = v.view(20, 80);
    let cur = rows.iter().find(|r| r.cursor).unwrap();
    assert!(cur.text.starts_with("line 054321"), "{cur:?}");
    assert_eq!(v.status().line, Some(54_321));

    v.goto(Target::Percent(5000));
    let st = v.status();
    assert!((49..=51).contains(&st.percent), "{st:?}");

    v.goto(Target::Line(200_000));
    assert_eq!(v.notice(), Some(&Notice::BeyondEnd(100_000)));

    // Paging and arrows keep the cursor on screen.
    v.handle_key(key(KeyCode::Home), 20);
    v.handle_key(key(KeyCode::PageDown), 20);
    v.handle_key(key(KeyCode::Down), 20);
    assert_eq!(v.status().line, Some(21));
    let rows = v.view(20, 80);
    assert!(rows.iter().any(|r| r.cursor));

    // Paging at the end keeps a full screen.
    v.handle_key(key(KeyCode::End), 20);
    v.handle_key(key(KeyCode::PageDown), 20);
    assert_eq!(v.view(20, 80).len(), 20);
}

#[test]
fn goto_through_the_input_line() {
    let file = TempFile::new("input", &log_lines(5_000));
    let mut v = Viewer::open(&file.0).unwrap();
    v.index_mut().wait();
    v.handle_key(key(KeyCode::Char(':')), 20);
    assert_eq!(v.input(), Some((InputKind::Goto, "")));
    type_str(&mut v, "4000");
    v.handle_key(key(KeyCode::Enter), 20);
    assert_eq!(v.input(), None);
    assert_eq!(v.status().line, Some(4000));
    v.handle_key(key(KeyCode::Char(':')), 20);
    type_str(&mut v, "what");
    v.handle_key(key(KeyCode::Enter), 20);
    assert_eq!(v.notice(), Some(&Notice::BadGoto));
}

#[test]
fn searches_forward_backward_and_repeats() {
    let mut bytes = log_lines(50_000);
    let marker = b"NEEDLE-in-the-haystack\n";
    let at = bytes.len() / 3;
    let at = at + bytes[at..].iter().position(|&b| b == b'\n').unwrap() + 1;
    bytes.splice(at..at, marker.iter().copied());
    let file = TempFile::new("search", &bytes);
    let mut v = Viewer::open(&file.0).unwrap();
    v.handle_key(key(KeyCode::Char('/')), 20);
    type_str(&mut v, "NEEDLE");
    v.handle_key(key(KeyCode::Enter), 20);
    settle(&mut v);
    assert_eq!(v.cursor(), at as u64);
    let rows = v.view(20, 80);
    let cur = rows.iter().find(|r| r.cursor).unwrap();
    assert_eq!(cur.hit, Some((0, 6)));

    // `n` again finds nothing further on.
    v.handle_key(key(KeyCode::Char('n')), 20);
    settle(&mut v);
    assert_eq!(v.notice(), Some(&Notice::NotFound));

    // Smart case: lowercase matches either case, searching backward from EOF.
    v.handle_key(key(KeyCode::End), 20);
    v.handle_key(key(KeyCode::Char('?')), 20);
    type_str(&mut v, "needle");
    v.handle_key(key(KeyCode::Enter), 20);
    settle(&mut v);
    assert_eq!(v.cursor(), at as u64);
}

#[test]
fn esc_cancels_a_search_then_closes() {
    let file = TempFile::new("cancel", &log_lines(1_000));
    let mut v = Viewer::open(&file.0).unwrap();
    v.handle_key(key(KeyCode::Char('/')), 20);
    type_str(&mut v, "zzz-not-there");
    v.handle_key(key(KeyCode::Enter), 20);
    // Either it already finished (not found) or Esc cancels it; then Esc closes.
    if v.search().is_some() {
        assert_eq!(v.handle_key(key(KeyCode::Esc), 20), Outcome::Consumed);
    }
    v.poll();
    assert_eq!(v.handle_key(key(KeyCode::Esc), 20), Outcome::Close);
}

#[test]
fn copies_the_selected_lines() {
    let file = TempFile::new("copy", &log_lines(100));
    let mut v = Viewer::open(&file.0).unwrap();
    v.handle_key(key(KeyCode::Down), 20);
    v.handle_key(key(KeyCode::Char('m')), 20);
    v.handle_key(key(KeyCode::Down), 20);
    v.handle_key(key(KeyCode::Down), 20);
    let rows = v.view(20, 80);
    assert_eq!(rows.iter().filter(|r| r.selected).count(), 3);
    let Outcome::Copy(text) = v.handle_key(key(KeyCode::Char('y')), 20) else {
        panic!("expected a copy");
    };
    assert_eq!(text.lines().count(), 3);
    assert!(text.starts_with("line 000002"));
    assert!(text.ends_with("dog\n"));
    assert_eq!(v.notice(), Some(&Notice::Copied(text.len() as u64)));
}

#[test]
fn shows_crlf_invalid_utf8_and_long_lines_safely() {
    let mut bytes = b"dos line\r\n".to_vec();
    bytes.extend_from_slice(b"bad \xff\xfe bytes\ttab\n");
    bytes.extend(std::iter::repeat_n(b'w', 100_000));
    bytes.extend_from_slice(b"END\nlast");
    let file = TempFile::new("odd", &bytes);
    let mut v = Viewer::open(&file.0).unwrap();
    let rows = v.view(10, 40);
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0].text, "dos line");
    assert_eq!(rows[1].text, "bad \u{fffd}\u{fffd} bytes·tab");
    assert_eq!(rows[2].text.chars().count(), 40);
    assert!(rows[2].more_right);
    assert_eq!(rows[3].text, "last");
    // Scroll right far enough to see the long line's end.
    for _ in 0..(100_000 / 8) {
        v.handle_key(key(KeyCode::Right), 10);
    }
    let rows = v.view(10, 40);
    assert!(rows[2].text.ends_with("END"), "{:?}", rows[2]);
    assert!(!rows[2].more_right);
    assert_eq!(rows[0].text, "");
    v.handle_key(key(KeyCode::Char('0')), 10);
    assert_eq!(v.status().hscroll, 0);
}

#[test]
fn goto_a_line_before_the_index_reaches_it_waits_for_it() {
    let file = TempFile::new("pending", &log_lines(200_000));
    let mut v = Viewer::open(&file.0).unwrap();
    v.goto(Target::Line(199_999));
    // Either the index already had it, or the goto is pending until it does.
    for _ in 0..10_000 {
        if v.status().line == Some(199_999) {
            return;
        }
        v.poll();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("pending goto never applied: {:?}", v.notice());
}
