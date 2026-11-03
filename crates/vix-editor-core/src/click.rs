//! Multi-click classification for the mouse handler.
//!
//! Terminals deliver each press as an independent event, so the editor has to
//! work out single/double/triple clicks itself. [`ClickTracker`] remembers the
//! last two clicks (time + cursor offset) and classifies the next one; the
//! result drives word-snap (double) and line-snap (triple) selection in
//! [`crate::selection::SelectionSnap`]. See `crates/vix-editor-core/spec/index.md`.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// The kind of mouse click detected from timing and position.
pub enum ClickKind {
    /// A single click.
    Single,
    /// A double click (two clicks at the same position within `max_dt`).
    Double,
    /// A triple click (three clicks at the same position within `max_dt`).
    Triple,
}

/// Tracks recent clicks to classify the next click as single/double/triple.
///
/// Position comparison is on the *cursor offset* the click resolved to, not on
/// screen coordinates, so a click on the same character counts even if the
/// pointer moved by a few pixels/cells.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use vix_editor_core::click::{ClickKind, ClickTracker};
///
/// let mut t = ClickTracker::new(Duration::from_secs(60));
/// assert_eq!(t.register(3), ClickKind::Single);
/// assert_eq!(t.register(3), ClickKind::Double);
/// assert_eq!(t.register(3), ClickKind::Triple);
/// // A click elsewhere starts over.
/// assert_eq!(t.register(9), ClickKind::Single);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct ClickTracker {
    /// Time and cursor position of the most recent click.
    pub last: Option<(Instant, usize)>,
    /// Time and cursor position of the click before `last`.
    pub prev: Option<(Instant, usize)>,
    /// Maximum interval between clicks for them to count as a multi-click.
    pub max_dt: Duration,
}

impl ClickTracker {
    /// Create a tracker that treats clicks within `max_dt` as a multi-click.
    #[must_use]
    pub fn new(max_dt: Duration) -> Self {
        Self {
            last: None,
            prev: None,
            max_dt,
        }
    }

    /// Record a click at `cursor` (a character offset) and return its
    /// classified [`ClickKind`].
    ///
    /// Uses [`Instant::now`] as the click time, shifts `last` into `prev`, and
    /// stores this click as the new `last`.
    pub fn register(&mut self, cursor: usize) -> ClickKind {
        let now = Instant::now();
        // Double: the previous click was on the same offset within `max_dt`.
        let dbl = self
            .last
            .is_some_and(|(t, p)| p == cursor && now.duration_since(t) < self.max_dt);
        // Triple needs *both* earlier clicks on this offset, and both gaps
        // (prev -> last, prev -> now) inside `max_dt`, so a slow third click
        // degrades to a double rather than a triple.
        let tpl = self
            .last
            .zip(self.prev)
            .is_some_and(|((t1, p1), (t0, p0))| {
                p0 == cursor
                    && p1 == cursor
                    && now.duration_since(t0) < self.max_dt
                    && t1.duration_since(t0) < self.max_dt
            });

        // Always slide the window, even when this click classifies as Single.
        self.prev = self.last;
        self.last = Some((now, cursor));

        // Triple wins over double: a triple click is also, by definition, a double.
        if tpl {
            ClickKind::Triple
        } else if dbl {
            ClickKind::Double
        } else {
            ClickKind::Single
        }
    }
}
