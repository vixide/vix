//! [`Search`]: a substring search streamed through the file on a background
//! thread, with progress and cancel.
//!
//! The file is scanned in [`SEARCH_CHUNK`]-sized steps; each window is read
//! `needle.len() - 1` bytes past its end so a match straddling two steps is
//! still seen. Matching is ASCII "smart case" over raw bytes (no UTF-8
//! decoding), using `memchr`'s `memmem` finders. [`find`] is the synchronous
//! core; [`Search`] wraps it in a thread. Spec:
//! `crates/vix-large-file/spec/index.md`.

use std::fs::File;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use crate::index::percent_of;
use crate::read::read_full;

/// How many bytes one search step reads (plus the needle's length, so a match
/// straddling two steps is still seen).
const SEARCH_CHUNK: usize = 1024 * 1024;

/// Which way a search runs from its starting offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Toward the end: the first match starting at or after the offset.
    Forward,
    /// Toward the start: the last match starting before the offset.
    Backward,
}

/// How a search ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    /// A match starts at this byte offset.
    At(u64),
    /// No match in that direction.
    NotFound,
    /// Cancelled before it finished.
    Cancelled,
    /// A read failed (the error's text).
    Failed(String),
}

/// Whether `needle` should match case-insensitively: "smart case", as in the
/// editor's own find — only when it has no uppercase letter.
fn smart_case_insensitive(needle: &[u8]) -> bool {
    !needle.iter().any(u8::is_ascii_uppercase)
}

/// Search `len` bytes of `file` for `needle` from `from` in `direction`,
/// adding the bytes scanned to `progress` and stopping early when `cancel` is
/// set. Smart case: a needle without uppercase letters matches ASCII letters
/// of either case.
///
/// # Errors
///
/// The underlying read error.
pub fn find(
    file: &File,
    len: u64,
    needle: &[u8],
    from: u64,
    direction: Direction,
    cancel: &AtomicBool,
    progress: &AtomicU64,
) -> io::Result<Found> {
    if needle.is_empty() {
        return Ok(Found::NotFound);
    }
    let fold = smart_case_insensitive(needle);
    let needle: Vec<u8> = if fold {
        needle.to_ascii_lowercase()
    } else {
        needle.to_vec()
    };
    // Windows reach this far past their end so a straddling match is found.
    // (`needle` is non-empty here, so the subtraction cannot underflow.)
    let overlap = needle.len() as u64 - 1;
    let step = SEARCH_CHUNK as u64;
    let mut reader = Windows {
        file,
        len,
        overlap,
        fold,
        buf: Vec::new(),
    };
    match direction {
        Direction::Forward => {
            let finder = memchr::memmem::Finder::new(&needle);
            let mut pos = from.min(len);
            while pos < len {
                if cancel.load(Ordering::Acquire) {
                    return Ok(Found::Cancelled);
                }
                let end = pos.saturating_add(step).min(len);
                // A hit inside the overlap tail is still the earliest match:
                // any earlier start would have been found in a previous window.
                if let Some(i) = finder.find(reader.window(pos, end)?) {
                    return Ok(Found::At(pos + i as u64));
                }
                progress.fetch_add(end - pos, Ordering::Release);
                pos = end;
            }
        }
        Direction::Backward => {
            let finder = memchr::memmem::FinderRev::new(&needle);
            let mut end = from.min(len);
            while end > 0 {
                if cancel.load(Ordering::Acquire) {
                    return Ok(Found::Cancelled);
                }
                let begin = end.saturating_sub(step);
                // The overlap tail cannot hold a match *starting* at or after
                // `end` (it is one byte too short), so `rfind` honours "starts
                // before the offset".
                if let Some(i) = finder.rfind(reader.window(begin, end)?) {
                    return Ok(Found::At(begin + i as u64));
                }
                progress.fetch_add(end - begin, Ordering::Release);
                end = begin;
            }
        }
    }
    Ok(Found::NotFound)
}

/// Reads the overlapping windows a search scans. Keeps one reusable buffer so
/// a multi-gigabyte scan allocates once.
struct Windows<'f> {
    file: &'f File,
    len: u64,
    /// The needle's length minus one: how far a window reaches past its end.
    overlap: u64,
    /// Lowercase each window (smart case).
    fold: bool,
    buf: Vec<u8>,
}

impl Windows<'_> {
    /// The bytes `[begin, end + overlap)`, clamped to the file, folded if need be.
    fn window(&mut self, begin: u64, end: u64) -> io::Result<&[u8]> {
        let stop = end.saturating_add(self.overlap).min(self.len);
        self.buf
            .resize(usize::try_from(stop - begin).unwrap_or(0), 0);
        let got = read_full(self.file, &mut self.buf, begin)?;
        self.buf.truncate(got);
        if self.fold {
            self.buf.make_ascii_lowercase();
        }
        Ok(&self.buf)
    }
}

/// State shared with the search thread.
#[derive(Debug, Default)]
struct Shared {
    /// Bytes scanned so far (monotonic); drives the progress percentage.
    progress: AtomicU64,
    /// Set by [`Search::cancel`] / `Drop`; checked once per step.
    cancel: AtomicBool,
    /// The outcome, written once by the thread, taken by [`Search::poll`].
    result: Mutex<Option<Found>>,
}

/// A search running on a background thread.
#[derive(Debug)]
pub struct Search {
    shared: Arc<Shared>,
    /// Bytes it may have to scan, for the progress percentage.
    span: u64,
    /// Needle length in bytes, so the viewer can highlight the match.
    needle_len: u64,
    direction: Direction,
}

impl Search {
    /// Start searching `len` bytes of `path` for `needle` from `from`.
    ///
    /// # Errors
    ///
    /// The error opening the file or starting the thread.
    pub fn start(
        path: &Path,
        len: u64,
        needle: &[u8],
        from: u64,
        direction: Direction,
    ) -> io::Result<Self> {
        let file = File::open(path)?;
        let shared = Arc::new(Shared::default());
        let worker = Arc::clone(&shared);
        let owned = needle.to_vec();
        std::thread::Builder::new()
            .name("vix-large-file-search".into())
            .spawn(move || {
                let found = find(
                    &file,
                    len,
                    &owned,
                    from,
                    direction,
                    &worker.cancel,
                    &worker.progress,
                )
                .unwrap_or_else(|e| Found::Failed(e.to_string()));
                *worker.result.lock().unwrap_or_else(PoisonError::into_inner) = Some(found);
            })?;
        let span = match direction {
            Direction::Forward => len.saturating_sub(from),
            Direction::Backward => from.min(len),
        };
        Ok(Self {
            shared,
            span,
            needle_len: needle.len() as u64,
            direction,
        })
    }

    /// Percent of its range scanned so far, 0–100.
    #[must_use]
    pub fn percent(&self) -> u8 {
        percent_of(self.shared.progress.load(Ordering::Acquire), self.span)
    }

    /// The needle's length in bytes.
    #[must_use]
    pub fn needle_len(&self) -> u64 {
        self.needle_len
    }

    /// Which way it runs.
    #[must_use]
    pub fn direction(&self) -> Direction {
        self.direction
    }

    /// Ask the thread to stop; it reports [`Found::Cancelled`].
    pub fn cancel(&self) {
        self.shared.cancel.store(true, Ordering::Release);
    }

    /// The result, once the search has finished (taken: later calls return
    /// `None`).
    #[must_use]
    pub fn poll(&self) -> Option<Found> {
        self.shared
            .result
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }

    /// Block until the search finishes. For tests and the benchmark.
    #[must_use]
    pub fn wait(&self) -> Found {
        loop {
            if let Some(found) = self.poll() {
                return found;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}

impl Drop for Search {
    /// Cancel on drop so an abandoned search (viewer closed, new search
    /// started) stops reading the file at its next step.
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::temp_file;
    use proptest::prelude::*;

    fn run(bytes: &[u8], needle: &[u8], from: u64, dir: Direction) -> Found {
        let path = temp_file(bytes);
        let file = File::open(&path).unwrap();
        let found = find(
            &file,
            bytes.len() as u64,
            needle,
            from,
            dir,
            &AtomicBool::new(false),
            &AtomicU64::new(0),
        )
        .unwrap();
        std::fs::remove_file(path).unwrap();
        found
    }

    #[test]
    fn finds_forward_and_backward_with_smart_case() {
        let text = b"one Two three two";
        assert_eq!(run(text, b"two", 0, Direction::Forward), Found::At(4));
        assert_eq!(run(text, b"two", 5, Direction::Forward), Found::At(14));
        assert_eq!(run(text, b"Two", 5, Direction::Forward), Found::NotFound);
        assert_eq!(run(text, b"two", 14, Direction::Backward), Found::At(4));
        assert_eq!(run(text, b"two", 4, Direction::Backward), Found::NotFound);
        assert_eq!(run(text, b"", 0, Direction::Forward), Found::NotFound);
    }

    #[test]
    fn finds_a_match_straddling_a_chunk_edge() {
        let mut text = vec![b'.'; SEARCH_CHUNK + 10];
        let at = SEARCH_CHUNK - 2;
        text[at..at + 5].copy_from_slice(b"NEEDL");
        assert_eq!(
            run(&text, b"NEEDL", 0, Direction::Forward),
            Found::At(at as u64)
        );
        assert_eq!(
            run(&text, b"NEEDL", text.len() as u64, Direction::Backward),
            Found::At(at as u64)
        );
    }

    #[test]
    fn a_cancelled_search_says_so() {
        let path = temp_file(&vec![b'x'; 3 * SEARCH_CHUNK]);
        let file = File::open(&path).unwrap();
        let found = find(
            &file,
            3 * SEARCH_CHUNK as u64,
            b"y",
            0,
            Direction::Forward,
            &AtomicBool::new(true),
            &AtomicU64::new(0),
        )
        .unwrap();
        assert_eq!(found, Found::Cancelled);
        std::fs::remove_file(path).unwrap();
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]
        #[test]
        fn matches_a_naive_search(
            bytes in proptest::collection::vec(prop_oneof![Just(b'a'), Just(b'b'), Just(b'\n')], 0..2000),
            needle in proptest::collection::vec(prop_oneof![Just(b'a'), Just(b'b')], 1..4),
            from in 0usize..2000,
        ) {
            let from = from.min(bytes.len());
            let hits: Vec<usize> = (0..bytes.len().saturating_sub(needle.len() - 1))
                .filter(|&i| bytes[i..].starts_with(&needle))
                .collect();
            let fwd = hits.iter().find(|&&i| i >= from).map_or(Found::NotFound, |&i| Found::At(i as u64));
            let back = hits.iter().rev().find(|&&i| i < from).map_or(Found::NotFound, |&i| Found::At(i as u64));
            prop_assert_eq!(run(&bytes, &needle, from as u64, Direction::Forward), fwd);
            prop_assert_eq!(run(&bytes, &needle, from as u64, Direction::Backward), back);
        }
    }
}
