//! The Large File Viewer (T733): a read-only view of files too big to load
//! into a text buffer — multi-gigabyte logs, dumps, and data files.
//!
//! Nothing here ever reads the whole file into memory. [`Source`] reads
//! fixed-size chunks with positioned reads (`pread` on Unix, `seek_read` on
//! Windows — both safe std APIs, so no memory map and no `unsafe`).
//! [`LineIndex`] scans the file once on a background thread and keeps the byte
//! offset of every [`DEFAULT_STRIDE`]th line, so going to a line number reads
//! at most one stride of lines. [`Search`] streams a substring search through
//! the file on another thread, with progress and cancel. [`Viewer`] ties these
//! together with a cursor, a scroll position, a line selection, and the keys
//! the host routes to it, returning an [`Outcome`]; the host renders the
//! [`ViewLine`]s and [`Status`] it hands back.
//!
//! Concurrency model: the index and search threads own their own file handle
//! and publish progress through atomics (`Release` stores, `Acquire` loads)
//! and results through a `Mutex`; the UI thread never blocks on them, it
//! polls ([`Viewer::poll`]) between redraws. Both threads stop when their
//! owner is dropped. See this crate's `spec/index.md`.
//!
//! The viewer reads local files only: it opens the path with `std::fs`, not
//! through `vix-authority`, so a remote workspace's files are not viewable
//! this way yet.

mod index;
mod read;
mod search;
mod source;
mod viewer;

#[cfg(test)]
mod testutil;

pub use index::{DEFAULT_STRIDE, LineIndex, Lookup, scan_chunk};
pub use search::{Direction, Found, Search, find};
pub use source::{LineSpan, Source};
pub use viewer::{
    COPY_LIMIT, InputKind, Notice, Outcome, Status, Target, ViewLine, Viewer, parse_target,
};
