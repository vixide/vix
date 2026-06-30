#![warn(clippy::pedantic)]
#[derive(Debug, Clone, Copy)]
/// How a drag selection snaps to text units as the cursor moves.
pub enum SelectionSnap {
    /// No snapping; select by individual characters.
    None,
    /// Snap selection to whole words, anchored at this offset.
    Word {
        /// The character offset where the word-snap drag began.
        anchor: usize,
    },
    /// Snap selection to whole lines, anchored at this offset.
    Line {
        /// The character offset where the line-snap drag began.
        anchor: usize,
    },
}

/// A selected text range between two character offsets.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Selection {
    /// Start character offset (inclusive).
    pub start: usize,
    /// End character offset (exclusive).
    pub end: usize,
}

impl Selection {
    /// Create a selection spanning `a` and `b`, ordering them as start/end.
    #[must_use] 
    pub fn new(a: usize, b: usize) -> Self {
        Self {
            start: a.min(b),
            end: a.max(b),
        }
    }

    /// Create a selection from an anchor and a cursor offset, ordered as start/end.
    #[must_use] 
    pub fn from_anchor_and_cursor(anchor: usize, cursor: usize) -> Self {
        if anchor <= cursor {
            Selection { start: anchor, end: cursor }
        } else {
            Selection { start: cursor, end: anchor }
        }
    }

    /// Return `true` if the selection spans at least one character.
    #[must_use] 
    pub fn is_active(&self) -> bool {
        self.start != self.end
    }

    /// Return `true` if the selection is empty (start equals end).
    #[must_use] 
    pub fn is_empty(&self) -> bool {
        self.start.max(self.end) == self.start.min(self.end)
    }

    /// Return `true` if `index` falls within the selection (start inclusive, end exclusive).
    #[must_use] 
    pub fn contains(&self, index: usize) -> bool {
        index >= self.start && index < self.end
    }

    /// Return the (lower, higher) offsets of the selection.
    #[must_use] 
    pub fn sorted(&self) -> (usize, usize) {
        if self.start <= self.end {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }
}
