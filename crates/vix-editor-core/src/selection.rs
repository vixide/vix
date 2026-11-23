//! Selection value types: [`Selection`] (a start/end character range) and
//! [`SelectionSnap`] (how a mouse drag extends it). All offsets are *character*
//! offsets into the rope buffer (not bytes, not display columns). The editor
//! widget in [`crate::editor`] owns the live selection state; this module is
//! pure data. See `crates/vix-editor-core/spec/index.md`.

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
///
/// The fields are public, so a value built by struct literal is *not*
/// guaranteed ordered (`start > end` is representable); use [`Selection::new`]
/// or [`Selection::from_anchor_and_cursor`] to get ordered ends, or
/// [`Selection::sorted`] to normalize.
///
/// # Examples
///
/// ```
/// use vix_editor_core::selection::Selection;
///
/// let s = Selection::new(7, 2);
/// assert_eq!((s.start, s.end), (2, 7));
/// assert!(s.is_active() && s.contains(2) && !s.contains(7));
/// ```
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
            Selection {
                start: anchor,
                end: cursor,
            }
        } else {
            Selection {
                start: cursor,
                end: anchor,
            }
        }
    }

    /// Return `true` if the selection spans at least one character.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.start != self.end
    }

    /// Return `true` if the selection is empty (start equals end); the exact
    /// complement of [`Selection::is_active`], and order-insensitive.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.start.max(self.end) == self.start.min(self.end)
    }

    /// Return `true` if `index` falls within the selection (start inclusive, end exclusive).
    #[must_use]
    pub fn contains(&self, index: usize) -> bool {
        index >= self.start && index < self.end
    }

    /// Return the (lower, higher) offsets of the selection, even if the public
    /// fields were set out of order.
    #[must_use]
    pub fn sorted(&self) -> (usize, usize) {
        if self.start <= self.end {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }
}
