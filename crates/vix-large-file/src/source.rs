//! [`Source`]: an open file of a fixed length, navigated line by line through
//! bounded chunk reads.
//!
//! All navigation (`forward`, `back`, `lines_from`, ...) works on **byte
//! offsets of line starts** and never reads more than [`CHUNK`] bytes at a
//! time, so cost is proportional to the lines moved, not the file size. A
//! trailing `\n` ends the last line rather than starting an empty one; an
//! empty file is one empty line. Used by `LineIndex` (`index.rs`) and `Viewer`
//! (`viewer.rs`); spec: `crates/vix-large-file/spec/index.md`.

use std::fs::File;
use std::io;
use std::path::Path;

use crate::read::read_full;

/// How many bytes one navigation read covers (64 KiB: big enough to amortize
/// syscalls, small enough that a one-line move stays cheap).
const CHUNK: usize = 64 * 1024;

/// One line of the file: `start..end` in bytes, `end` excluding the `\n`
/// (a `\r` before it, from a CRLF ending, is left in and trimmed for display).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineSpan {
    /// Byte offset of the line's first byte.
    pub start: u64,
    /// Byte offset just past the line's last byte, before its `\n`.
    pub end: u64,
}

/// A regular file opened for positioned reads. Its length is taken once at
/// open: bytes appended later are not shown, and reads past a truncation come
/// back short rather than failing.
#[derive(Debug)]
pub struct Source {
    file: File,
    len: u64,
}

impl Source {
    /// Open `path`, refusing anything that is not a regular file (a FIFO or
    /// device would block or never end).
    ///
    /// # Errors
    ///
    /// The open or metadata error, or `InvalidInput` for a non-regular file.
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let meta = file.metadata()?;
        if !meta.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a regular file",
            ));
        }
        Ok(Self {
            file,
            len: meta.len(),
        })
    }

    /// The file's length in bytes, as of open.
    #[must_use]
    pub fn len(&self) -> u64 {
        self.len
    }

    /// Whether the file was empty at open.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Up to `max` bytes from `offset`, clamped to the file's length.
    ///
    /// # Errors
    ///
    /// The underlying read error.
    pub fn read(&self, offset: u64, max: u64) -> io::Result<Vec<u8>> {
        let end = offset.saturating_add(max).min(self.len);
        let n = usize::try_from(end.saturating_sub(offset)).unwrap_or(usize::MAX);
        let mut buf = vec![0; n];
        let got = read_full(&self.file, &mut buf, offset)?;
        buf.truncate(got);
        Ok(buf)
    }

    /// Read the chunk `[begin, end)` into `buf` (reused across calls).
    fn chunk<'a>(&self, buf: &'a mut Vec<u8>, begin: u64, end: u64) -> io::Result<&'a [u8]> {
        let n = usize::try_from(end - begin).unwrap_or(CHUNK);
        buf.resize(n, 0);
        let got = read_full(&self.file, buf, begin)?;
        Ok(&buf[..got])
    }

    /// Start of the line holding byte `pos` (clamped to the length): one past
    /// the last `\n` before `pos`, or 0.
    ///
    /// # Errors
    ///
    /// The underlying read error.
    pub fn line_start_containing(&self, pos: u64) -> io::Result<u64> {
        let mut end = pos.min(self.len);
        let mut buf = Vec::new();
        while end > 0 {
            let begin = end.saturating_sub(CHUNK as u64);
            let bytes = self.chunk(&mut buf, begin, end)?;
            if let Some(i) = memchr::memrchr(b'\n', bytes) {
                return Ok(begin + i as u64 + 1);
            }
            end = begin;
        }
        Ok(0)
    }

    /// Start of the last line (0 for an empty file). A trailing `\n` ends the
    /// last line rather than starting an empty one.
    ///
    /// # Errors
    ///
    /// The underlying read error.
    pub fn last_line_start(&self) -> io::Result<u64> {
        if self.len == 0 {
            return Ok(0);
        }
        self.line_start_containing(self.len - 1)
    }

    /// The start of the line `n` lines after the one starting at `start`, or
    /// `None` when fewer than `n` lines follow it.
    ///
    /// # Errors
    ///
    /// The underlying read error.
    pub fn forward_exact(&self, start: u64, n: u64) -> io::Result<Option<u64>> {
        let (moved, at) = self.forward_count(start, n)?;
        Ok((moved == n).then_some(at))
    }

    /// The start of the line `n` lines after `start`, stopping at the last line.
    ///
    /// # Errors
    ///
    /// The underlying read error.
    pub fn forward(&self, start: u64, n: u64) -> io::Result<u64> {
        Ok(self.forward_count(start, n)?.1)
    }

    /// Step up to `n` lines forward from `start`: how many it moved and where.
    fn forward_count(&self, start: u64, n: u64) -> io::Result<(u64, u64)> {
        // `at` is the start of the last line reached; a `\n` that is the file's
        // final byte starts no new line, hence the `next >= self.len` stop.
        let mut at = start;
        let mut moved = 0;
        let mut pos = start;
        let mut buf = Vec::new();
        while moved < n && pos < self.len {
            let end = pos.saturating_add(CHUNK as u64).min(self.len);
            let bytes = self.chunk(&mut buf, pos, end)?;
            if bytes.is_empty() {
                break;
            }
            for i in memchr::memchr_iter(b'\n', bytes) {
                let next = pos + i as u64 + 1;
                if next >= self.len {
                    return Ok((moved, at));
                }
                at = next;
                moved += 1;
                if moved == n {
                    return Ok((moved, at));
                }
            }
            pos += bytes.len() as u64;
        }
        Ok((moved, at))
    }

    /// The start of the line `n` lines before the one starting at `start`,
    /// stopping at the first line.
    ///
    /// # Errors
    ///
    /// The underlying read error.
    pub fn back(&self, start: u64, n: u64) -> io::Result<u64> {
        if n == 0 || start == 0 {
            return Ok(start);
        }
        // `start - 1` is the `\n` ending the previous line; the newlines
        // before it each start one line further back.
        let mut end = (start - 1).min(self.len);
        let mut remaining = n;
        let mut buf = Vec::new();
        while end > 0 {
            let begin = end.saturating_sub(CHUNK as u64);
            let bytes = self.chunk(&mut buf, begin, end)?;
            for i in memchr::memrchr_iter(b'\n', bytes) {
                remaining -= 1;
                if remaining == 0 {
                    return Ok(begin + i as u64 + 1);
                }
            }
            end = begin;
        }
        Ok(0)
    }

    /// Up to `n` lines starting with the one at `start`. An empty file has one
    /// empty line.
    ///
    /// # Errors
    ///
    /// The underlying read error.
    pub fn lines_from(&self, start: u64, n: usize) -> io::Result<Vec<LineSpan>> {
        let mut spans = Vec::with_capacity(n.min(4096));
        // `start == len` is past a trailing newline (no line there), except in
        // an empty file where the single empty line starts at 0 == len.
        if n == 0 || start > self.len || (start == self.len && self.len > 0) {
            return Ok(spans);
        }
        let mut line_start = start;
        let mut pos = start;
        let mut buf = Vec::new();
        while spans.len() < n {
            if pos >= self.len {
                spans.push(LineSpan {
                    start: line_start,
                    end: self.len,
                });
                break;
            }
            let end = pos.saturating_add(CHUNK as u64).min(self.len);
            let bytes = self.chunk(&mut buf, pos, end)?;
            if bytes.is_empty() {
                spans.push(LineSpan {
                    start: line_start,
                    end: pos,
                });
                break;
            }
            for i in memchr::memchr_iter(b'\n', bytes) {
                let nl = pos + i as u64;
                spans.push(LineSpan {
                    start: line_start,
                    end: nl,
                });
                line_start = nl + 1;
                if spans.len() == n || line_start >= self.len {
                    return Ok(spans);
                }
            }
            pos += bytes.len() as u64;
        }
        Ok(spans)
    }

    /// How many `\n` bytes lie in `[from, to)`.
    ///
    /// # Errors
    ///
    /// The underlying read error.
    pub fn count_newlines(&self, from: u64, to: u64) -> io::Result<u64> {
        let to = to.min(self.len);
        let mut pos = from;
        let mut count = 0;
        let mut buf = Vec::new();
        while pos < to {
            let end = pos.saturating_add(CHUNK as u64).min(to);
            let bytes = self.chunk(&mut buf, pos, end)?;
            if bytes.is_empty() {
                break;
            }
            count += memchr::memchr_iter(b'\n', bytes).count() as u64;
            pos += bytes.len() as u64;
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{random_text, temp_file};
    use proptest::prelude::*;

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
    fn navigates_lines() {
        let text = b"alpha\nbeta\r\ngamma\n\ndelta";
        let path = temp_file(text);
        let src = Source::open(&path).unwrap();
        let starts = naive_starts(text);
        assert_eq!(starts, vec![0, 6, 12, 18, 19]);
        assert_eq!(src.forward(0, 2).unwrap(), 12);
        assert_eq!(src.forward(0, 99).unwrap(), 19);
        assert_eq!(src.forward_exact(0, 5).unwrap(), None);
        assert_eq!(src.forward_exact(0, 4).unwrap(), Some(19));
        assert_eq!(src.back(19, 2).unwrap(), 12);
        assert_eq!(src.back(19, 99).unwrap(), 0);
        assert_eq!(src.line_start_containing(14).unwrap(), 12);
        assert_eq!(src.line_start_containing(12).unwrap(), 12);
        assert_eq!(src.last_line_start().unwrap(), 19);
        let spans = src.lines_from(6, 10).unwrap();
        assert_eq!(
            spans,
            vec![
                LineSpan { start: 6, end: 11 },
                LineSpan { start: 12, end: 17 },
                LineSpan { start: 18, end: 18 },
                LineSpan { start: 19, end: 24 },
            ]
        );
        assert_eq!(src.count_newlines(0, 24).unwrap(), 4);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn trailing_newline_ends_the_last_line() {
        let path = temp_file(b"one\ntwo\n");
        let src = Source::open(&path).unwrap();
        assert_eq!(src.last_line_start().unwrap(), 4);
        assert_eq!(src.forward(0, 5).unwrap(), 4);
        assert_eq!(
            src.lines_from(4, 5).unwrap(),
            vec![LineSpan { start: 4, end: 7 }]
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn empty_file_has_one_empty_line() {
        let path = temp_file(b"");
        let src = Source::open(&path).unwrap();
        assert!(src.is_empty());
        assert_eq!(
            src.lines_from(0, 3).unwrap(),
            vec![LineSpan { start: 0, end: 0 }]
        );
        assert_eq!(src.last_line_start().unwrap(), 0);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn refuses_a_directory() {
        assert!(Source::open(&std::env::temp_dir()).is_err());
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]
        #[test]
        fn navigation_matches_a_naive_scan(
            len in 0usize..300_000,
            seed in any::<u64>(),
            density in 1u64..400,
            pick in 0usize..10_000,
            n in 0u64..400,
        ) {
            let bytes = random_text(len, seed, density);
            let path = temp_file(&bytes);
            let src = Source::open(&path).unwrap();
            let starts = naive_starts(&bytes);
            let i = pick % starts.len();
            let at = starts[i];
            let fwd = starts[(i + usize::try_from(n).unwrap()).min(starts.len() - 1)];
            prop_assert_eq!(src.forward(at, n).unwrap(), fwd);
            let back = starts[i.saturating_sub(usize::try_from(n).unwrap())];
            prop_assert_eq!(src.back(at, n).unwrap(), back);
            prop_assert_eq!(src.last_line_start().unwrap(), *starts.last().unwrap());
            let spans = src.lines_from(at, 3).unwrap();
            for (k, s) in spans.iter().enumerate() {
                prop_assert_eq!(s.start, starts[i + k]);
            }
            std::fs::remove_file(path).unwrap();
        }
    }
}
