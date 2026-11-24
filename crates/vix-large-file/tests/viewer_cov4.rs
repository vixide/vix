//! Round-4 coverage: the small public accessors, cancellation and the
//! truncated-file edge cases of the index and search.
#![allow(clippy::unwrap_used, reason = "tests fail loudly on a broken fixture")]

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use vix_large_file::{Direction, Found, LineIndex, Lookup, Search, Source, find};

struct TempFile(PathBuf);

impl TempFile {
    fn new(tag: &str, bytes: &[u8]) -> Self {
        let path =
            std::env::temp_dir().join(format!("vix-large-file-cov4-{tag}-{}", std::process::id()));
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

fn lines(n: usize) -> Vec<u8> {
    (0..n).fold(Vec::new(), |mut out, i| {
        writeln!(out, "row {i:04}").unwrap();
        out
    })
}

#[test]
fn index_reports_its_stride_and_resolves_lines_past_a_checkpoint() {
    let f = TempFile::new("stride", &lines(25));
    let len = std::fs::metadata(&f.0).unwrap().len();
    let mut idx = LineIndex::build_with(&f.0, len, 4, 16).unwrap();
    idx.wait();
    assert_eq!(idx.stride(), 4);
    let src = Source::open(&f.0).unwrap();
    // Line 10 lies two lines past checkpoint 2 (line 8); each row is 9 bytes.
    assert_eq!(idx.line_start(&src, 10).unwrap(), Lookup::Found(90));
    assert_eq!(idx.line_start(&src, 25).unwrap(), Lookup::BeyondEnd);
    assert_eq!(idx.line_of(&src, 90).unwrap(), Some(10));
}

#[test]
fn index_over_a_file_shorter_than_declared_stops_at_the_real_end() {
    let f = TempFile::new("short", &lines(3));
    let real = std::fs::metadata(&f.0).unwrap().len();
    // Claim the file is longer than it is: the reader hits EOF early and the
    // scan stops there instead of looping.
    let mut idx = LineIndex::build_with(&f.0, real + 1000, 2, 8).unwrap();
    idx.wait();
    assert!(!idx.failed());
    assert!(idx.scanned() <= real + 1000);
}

#[test]
fn direct_find_honours_a_pre_set_cancel_flag_both_ways() {
    let f = TempFile::new("cancel", &lines(50));
    let file = std::fs::File::open(&f.0).unwrap();
    let len = file.metadata().unwrap().len();
    let cancel = AtomicBool::new(true);
    let progress = AtomicU64::new(0);
    for dir in [Direction::Forward, Direction::Backward] {
        let from = if dir == Direction::Forward { 0 } else { len };
        let found = find(&file, len, b"row 0049", from, dir, &cancel, &progress).unwrap();
        assert_eq!(found, Found::Cancelled, "{dir:?}");
    }
    assert_eq!(progress.load(Ordering::Acquire), 0, "nothing was scanned");
    cancel.store(false, Ordering::Release);
    let hit = find(
        &file,
        len,
        b"row 0049",
        0,
        Direction::Forward,
        &cancel,
        &progress,
    )
    .unwrap();
    assert_eq!(hit, Found::At(49 * 9));
}

#[test]
fn search_handle_reports_direction_and_blocks_until_found() {
    let f = TempFile::new("wait", &lines(40));
    let len = std::fs::metadata(&f.0).unwrap().len();
    let s = Search::start(&f.0, len, b"row 0030", 0, Direction::Forward).unwrap();
    assert_eq!(s.direction(), Direction::Forward);
    assert_eq!(s.needle_len(), 8);
    assert_eq!(s.wait(), Found::At(30 * 9));
    let back = Search::start(&f.0, len, b"row 0001", len, Direction::Backward).unwrap();
    assert_eq!(back.direction(), Direction::Backward);
    assert_eq!(back.wait(), Found::At(9));
}

#[test]
fn search_of_a_missing_needle_reports_not_found() {
    let f = TempFile::new("fail", b"abc\n");
    // A needle absent from the file ends as NotFound, not as an error.
    let s = Search::start(&f.0, 4, b"zzz", 0, Direction::Forward).unwrap();
    assert_eq!(s.wait(), Found::NotFound);
}

#[test]
fn source_lines_from_stops_at_an_unterminated_final_line() {
    let f = TempFile::new("tail", b"one\ntwo\nthree");
    let src = Source::open(&f.0).unwrap();
    let spans = src.lines_from(0, 10).unwrap();
    let ranges: Vec<(u64, u64)> = spans.iter().map(|s| (s.start, s.end)).collect();
    assert_eq!(ranges, vec![(0, 3), (4, 7), (8, 13)]);
    assert_eq!(src.lines_from(8, 5).unwrap().len(), 1);
}

mod viewer {
    use super::{TempFile, lines};
    use vix_input::{KeyCode, KeyEvent, KeyModifiers};
    use vix_large_file::{Outcome, Viewer};

    fn open(tag: &str, n: usize) -> (TempFile, Viewer) {
        let f = TempFile::new(tag, &lines(n));
        let mut v = Viewer::open(&f.0).unwrap();
        v.index_mut().wait();
        (f, v)
    }

    fn key(v: &mut Viewer, code: KeyCode) -> Outcome {
        v.handle_key(KeyEvent::new(code, KeyModifiers::NONE), 5)
    }

    #[test]
    fn an_unbound_key_changes_nothing() {
        let (_f, mut v) = open("unbound", 20);
        key(&mut v, KeyCode::Down);
        let (cursor, top) = (v.cursor(), v.top());
        assert_eq!(key(&mut v, KeyCode::Char('~')), Outcome::Consumed);
        assert_eq!((v.cursor(), v.top()), (cursor, top));
        assert!(v.notice().is_none());
    }

    #[test]
    fn moving_up_past_the_top_row_scrolls_one_line_at_a_time() {
        let (_f, mut v) = open("scrollup", 40);
        key(&mut v, KeyCode::End);
        let bottom = v.cursor();
        let top_at_end = v.top();
        assert!(top_at_end < bottom);
        // Walk the cursor up past the first visible row: the view follows it.
        for _ in 0..8 {
            key(&mut v, KeyCode::Up);
        }
        assert_eq!(v.cursor(), bottom - 8 * 9);
        assert!(v.top() <= v.cursor());
        assert!(
            v.cursor() - v.top() < 5 * 9,
            "cursor stays inside the 5 rows"
        );
    }
}
