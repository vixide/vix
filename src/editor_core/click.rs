#![warn(clippy::pedantic)]
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
        Self { last: None, prev: None, max_dt }
    }

    /// Record a click at `cursor` and return its classified [`ClickKind`].
    pub fn register(&mut self, cursor: usize) -> ClickKind {
        let now = Instant::now();
        let dbl = self.last
            .is_some_and(|(t, p)| p == cursor && now.duration_since(t) < self.max_dt);
        let tpl = self.last.zip(self.prev)
            .is_some_and(|((t1, p1), (t0, p0))| {
                p0 == cursor && p1 == cursor &&
                now.duration_since(t0) < self.max_dt &&
                t1.duration_since(t0) < self.max_dt
            });

        self.prev = self.last;
        self.last = Some((now, cursor));

        if tpl { ClickKind::Triple }
        else if dbl { ClickKind::Double }
        else { ClickKind::Single }
    }
}


