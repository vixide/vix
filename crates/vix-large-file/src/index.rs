//! [`LineIndex`]: a sparse line-number → byte-offset index, built once on a
//! background thread while the file is already on screen.

use std::fs::File;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;

use crate::read::read_full;
use crate::source::Source;

/// The index keeps the offset of every this-many-th line: line `k * stride`
/// starts at checkpoint `k`. 1,024 lines of a typical log is ~100 KB, so a
/// lookup reads at most that much, and a 4 GiB file of 100-byte lines needs
/// only ~330 KB of checkpoints.
pub const DEFAULT_STRIDE: u64 = 1024;

/// How many bytes the index thread reads per step.
const INDEX_CHUNK: usize = 1024 * 1024;

/// Record the checkpoints that fall in `chunk`, which starts at absolute byte
/// `base` of a `len`-byte file, after `lines_before` newlines: each `\n` that
/// begins line `stride * k` (and is not the file's final byte) pushes that
/// line's start. Returns how many newlines `chunk` held.
pub fn scan_chunk(
    chunk: &[u8],
    base: u64,
    len: u64,
    lines_before: u64,
    stride: u64,
    out: &mut Vec<u64>,
) -> u64 {
    let stride = stride.max(1);
    let mut lines = lines_before;
    for i in memchr::memchr_iter(b'\n', chunk) {
        lines += 1;
        let start = base + i as u64 + 1;
        if start < len && lines.is_multiple_of(stride) {
            out.push(start);
        }
    }
    lines - lines_before
}

/// The answer to "where does line N start?".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lookup {
    /// It starts at this byte offset.
    Found(u64),
    /// The index has not reached it yet; ask again later.
    Pending,
    /// The file has fewer lines.
    BeyondEnd,
}

/// State shared with the index thread.
#[derive(Debug, Default)]
struct Shared {
    checkpoints: Mutex<Vec<u64>>,
    /// Bytes scanned so far.
    scanned: AtomicU64,
    /// Newlines seen so far.
    newlines: AtomicU64,
    /// The file's last byte is `\n` (valid once `done`).
    trailing_newline: AtomicBool,
    done: AtomicBool,
    failed: AtomicBool,
    cancel: AtomicBool,
}

/// A sparse line index over one file, filled in by a background thread.
/// Dropping it stops the thread at its next chunk.
#[derive(Debug)]
pub struct LineIndex {
    shared: Arc<Shared>,
    stride: u64,
    len: u64,
    handle: Option<JoinHandle<()>>,
}

impl LineIndex {
    /// Start indexing the first `len` bytes of `path` with [`DEFAULT_STRIDE`].
    ///
    /// # Errors
    ///
    /// The error opening the file.
    pub fn build(path: &Path, len: u64) -> io::Result<Self> {
        Self::build_with(path, len, DEFAULT_STRIDE, INDEX_CHUNK)
    }

    /// [`LineIndex::build`] with an explicit stride and read size (tests use
    /// small ones to cross chunk edges often).
    ///
    /// # Errors
    ///
    /// The error opening the file.
    pub fn build_with(path: &Path, len: u64, stride: u64, chunk: usize) -> io::Result<Self> {
        let file = File::open(path)?;
        let shared = Arc::new(Shared::default());
        shared
            .checkpoints
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(0);
        let stride = stride.max(1);
        let worker = Arc::clone(&shared);
        let handle = std::thread::Builder::new()
            .name("vix-large-file-index".into())
            .spawn(move || run(&file, len, stride, chunk.max(1), &worker))?;
        Ok(Self {
            shared,
            stride,
            len,
            handle: Some(handle),
        })
    }

    /// Block until the index is complete (or failed). For tests and the
    /// benchmark; the editor polls instead.
    pub fn wait(&mut self) {
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }

    /// Bytes scanned so far.
    #[must_use]
    pub fn scanned(&self) -> u64 {
        self.shared.scanned.load(Ordering::Acquire)
    }

    /// Percent of the file indexed, 0–100.
    #[must_use]
    pub fn percent(&self) -> u8 {
        percent_of(self.scanned(), self.len)
    }

    /// Whether the whole file is indexed.
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.shared.done.load(Ordering::Acquire)
    }

    /// Whether a read error stopped the index.
    #[must_use]
    pub fn failed(&self) -> bool {
        self.shared.failed.load(Ordering::Acquire)
    }

    /// The line count, once the index is done. A trailing `\n` ends the last
    /// line rather than starting another; an empty file has one line.
    #[must_use]
    pub fn total_lines(&self) -> Option<u64> {
        if !self.is_done() {
            return None;
        }
        let newlines = self.shared.newlines.load(Ordering::Acquire);
        let trailing = self.shared.trailing_newline.load(Ordering::Acquire);
        Some(newlines + u64::from(self.len == 0 || !trailing))
    }

    /// A copy of the checkpoints found so far.
    #[must_use]
    pub fn checkpoints(&self) -> Vec<u64> {
        self.shared
            .checkpoints
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The stride this index was built with.
    #[must_use]
    pub fn stride(&self) -> u64 {
        self.stride
    }

    /// Where 0-based `line` starts, reading at most one stride of lines from
    /// `src` past the nearest checkpoint.
    ///
    /// # Errors
    ///
    /// A read error from `src`.
    pub fn line_start(&self, src: &Source, line: u64) -> io::Result<Lookup> {
        if let Some(total) = self.total_lines()
            && line >= total
        {
            return Ok(Lookup::BeyondEnd);
        }
        let k = line / self.stride;
        let checkpoint = {
            let cps = self
                .shared
                .checkpoints
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            usize::try_from(k).ok().and_then(|k| cps.get(k).copied())
        };
        let Some(at) = checkpoint else {
            return Ok(if self.is_done() {
                Lookup::BeyondEnd
            } else {
                Lookup::Pending
            });
        };
        Ok(match src.forward_exact(at, line - k * self.stride)? {
            Some(off) => Lookup::Found(off),
            None if self.is_done() => Lookup::BeyondEnd,
            None => Lookup::Pending,
        })
    }

    /// The 0-based line number of the line starting at `offset`, once the
    /// index has scanned that far (`None` before then).
    ///
    /// # Errors
    ///
    /// A read error from `src`.
    pub fn line_of(&self, src: &Source, offset: u64) -> io::Result<Option<u64>> {
        if !self.is_done() && self.scanned() < offset {
            return Ok(None);
        }
        let (k, at) = {
            let cps = self
                .shared
                .checkpoints
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let k = cps.partition_point(|&c| c <= offset).saturating_sub(1);
            (k as u64, cps.get(k).copied().unwrap_or(0))
        };
        Ok(Some(k * self.stride + src.count_newlines(at, offset)?))
    }
}

impl Drop for LineIndex {
    fn drop(&mut self) {
        self.shared.cancel.store(true, Ordering::Release);
    }
}

/// `part` of `whole` as a whole percent, 0–100 (100 for an empty whole).
pub(crate) fn percent_of(part: u64, whole: u64) -> u8 {
    if whole == 0 {
        return 100;
    }
    let p = u128::from(part.min(whole)) * 100 / u128::from(whole);
    u8::try_from(p).unwrap_or(100)
}

/// The index thread: scan `len` bytes of `file` chunk by chunk.
fn run(file: &File, len: u64, stride: u64, chunk: usize, shared: &Shared) {
    let mut buf = vec![0; chunk];
    let mut pos = 0;
    let mut lines = 0;
    let mut found = Vec::new();
    let mut last = None;
    while pos < len {
        if shared.cancel.load(Ordering::Acquire) {
            return;
        }
        let want = usize::try_from(len - pos).map_or(chunk, |r| r.min(chunk));
        let got = match read_full(file, &mut buf[..want], pos) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => {
                shared.failed.store(true, Ordering::Release);
                return;
            }
        };
        found.clear();
        lines += scan_chunk(&buf[..got], pos, len, lines, stride, &mut found);
        if !found.is_empty() {
            shared
                .checkpoints
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend_from_slice(&found);
        }
        last = buf.get(got - 1).copied();
        pos += got as u64;
        shared.newlines.store(lines, Ordering::Release);
        shared.scanned.store(pos, Ordering::Release);
    }
    shared
        .trailing_newline
        .store(last == Some(b'\n'), Ordering::Release);
    shared.scanned.store(len, Ordering::Release);
    shared.done.store(true, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{random_text, temp_file};
    use proptest::prelude::*;

    /// Every line start, the slow obvious way.
    fn naive_starts(bytes: &[u8]) -> Vec<u64> {
        let mut starts = vec![0];
        for (i, b) in bytes.iter().enumerate() {
            if *b == b'\n' && i + 1 < bytes.len() {
                starts.push(i as u64 + 1);
            }
        }
        starts
    }

    #[test]
    fn counts_lines_with_and_without_a_trailing_newline() {
        for (text, lines) in [
            (&b""[..], 1),
            (b"a", 1),
            (b"a\n", 1),
            (b"a\nb", 2),
            (b"a\nb\n", 2),
            (b"\n\n", 2),
        ] {
            let path = temp_file(text);
            let mut idx = LineIndex::build_with(&path, text.len() as u64, 2, 3).unwrap();
            idx.wait();
            assert_eq!(idx.total_lines(), Some(lines), "{text:?}");
            assert_eq!(idx.percent(), 100);
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn lookups_past_the_end_say_so() {
        let text = b"a\nb\nc\n";
        let path = temp_file(text);
        let src = Source::open(&path).unwrap();
        let mut idx = LineIndex::build_with(&path, src.len(), 2, 4).unwrap();
        idx.wait();
        assert_eq!(idx.line_start(&src, 2).unwrap(), Lookup::Found(4));
        assert_eq!(idx.line_start(&src, 3).unwrap(), Lookup::BeyondEnd);
        assert_eq!(idx.line_of(&src, 4).unwrap(), Some(2));
        std::fs::remove_file(path).unwrap();
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]
        #[test]
        fn index_matches_a_naive_scan(
            len in 0usize..200_000,
            seed in any::<u64>(),
            density in 1u64..400,
            stride in 1u64..64,
            chunk in 16usize..5000,
            probes in proptest::collection::vec(any::<usize>(), 1..20),
        ) {
            let bytes = random_text(len, seed, density);
            let path = temp_file(&bytes);
            let src = Source::open(&path).unwrap();
            let mut idx = LineIndex::build_with(&path, src.len(), stride, chunk).unwrap();
            idx.wait();
            let starts = naive_starts(&bytes);
            prop_assert_eq!(idx.total_lines(), Some(starts.len() as u64));
            let expected: Vec<u64> = starts
                .iter()
                .copied()
                .step_by(usize::try_from(stride).unwrap())
                .collect();
            prop_assert_eq!(idx.checkpoints(), expected);
            for p in probes {
                let line = p % starts.len();
                prop_assert_eq!(
                    idx.line_start(&src, line as u64).unwrap(),
                    Lookup::Found(starts[line])
                );
                prop_assert_eq!(idx.line_of(&src, starts[line]).unwrap(), Some(line as u64));
            }
            std::fs::remove_file(path).unwrap();
        }
    }
}
