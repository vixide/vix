//! Shared arithmetic for a scrollable, single-selection list: move the
//! highlight up/down (one row or one page), select a row directly, and keep
//! a scroll offset following the highlight within a fixed-height viewport.
//!
//! Every panel with a "highlighted row in a scrolling list" (file browser,
//! character pickers, the outline panel, the workspace search panel, the DB
//! workbench's catalog/results/statement lists, and more — T144 counted 17
//! crates reimplementing the same handful of clamp expressions with minor
//! copy-paste drift) owns its own `selected`/`scroll` (or `sel`/`row`/`top`)
//! fields and calls these as **plain functions** rather than adopting a
//! shared struct: a struct would mean either renaming every panel's public
//! fields (breaking every external `.selected`/`.scroll` read across
//! `src/app.rs`/`src/ui/*.rs`) or wrapping them in an extra field
//! (`panel.cursor.selected` instead of `panel.selected`), and several
//! panels don't store a plain index at all — `vix-edit-bytes` derives a row
//! from a byte cursor (`cursor / COLS`), `vix-edit-outline` derives a
//! position from a computed visible-node list. Free functions over plain
//! `usize`s fit every one of these without changing any panel's public
//! shape; only each panel's *method bodies* change.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]

/// The highlighted row one step up, stopping at the top (row 0).
#[must_use]
pub fn up(selected: usize) -> usize {
    selected.saturating_sub(1)
}

/// The highlighted row one step down, stopping at the last of `len` rows.
/// `len == 0` leaves `selected` unchanged (there is nothing to move onto).
#[must_use]
pub fn down(selected: usize, len: usize) -> usize {
    if selected + 1 < len {
        selected + 1
    } else {
        selected
    }
}

/// The highlighted row `page` steps up (minimum 1), stopping at the top.
#[must_use]
pub fn page_up(selected: usize, page: usize) -> usize {
    selected.saturating_sub(page.max(1))
}

/// The highlighted row `page` steps down (minimum 1), stopping at the last
/// of `len` rows. `len == 0` clamps to row 0.
#[must_use]
pub fn page_down(selected: usize, page: usize, len: usize) -> usize {
    (selected + page.max(1)).min(len.saturating_sub(1))
}

/// Select row `idx` directly (e.g. from a mouse click), if it names a real
/// row among `len`. Returns `None` — leaving the caller's selection
/// untouched — for an out-of-range `idx`, most often a click past the end
/// of a shorter-than-expected list.
#[must_use]
pub fn select_index(idx: usize, len: usize) -> Option<usize> {
    (idx < len).then_some(idx)
}

/// The scroll offset that keeps `selected` inside a `height`-row viewport:
/// scrolls up just enough when the selection is above the window, down just
/// enough when it's below, and never scrolls past the point where the last
/// of `len` rows would leave blank space at the bottom.
///
/// `height` is clamped to at least 1 (a zero-height viewport can't keep
/// anything visible, so it degenerates to "one row").
#[must_use]
pub fn ensure_visible(selected: usize, scroll: usize, height: usize, len: usize) -> usize {
    let height = height.max(1);
    let scroll = if selected < scroll {
        selected
    } else if selected >= scroll + height {
        selected + 1 - height
    } else {
        scroll
    };
    scroll.min(len.saturating_sub(height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_stops_at_the_top() {
        assert_eq!(up(5), 4);
        assert_eq!(up(0), 0);
    }

    #[test]
    fn down_stops_at_the_last_row() {
        assert_eq!(down(2, 5), 3);
        assert_eq!(down(4, 5), 4, "already at the last row");
        assert_eq!(down(0, 0), 0, "empty list: nothing to move onto");
    }

    #[test]
    fn page_up_defaults_a_zero_page_to_one_row() {
        assert_eq!(page_up(5, 2), 3);
        assert_eq!(page_up(5, 0), 4, "page 0 still moves one row");
        assert_eq!(page_up(1, 10), 0, "stops at the top");
    }

    #[test]
    fn page_down_stops_at_the_last_row() {
        assert_eq!(page_down(2, 2, 10), 4);
        assert_eq!(page_down(8, 5, 10), 9, "clamped to the last row");
        assert_eq!(page_down(0, 3, 0), 0, "empty list clamps to row 0");
    }

    #[test]
    fn select_index_only_accepts_a_real_row() {
        assert_eq!(select_index(3, 10), Some(3));
        assert_eq!(select_index(10, 10), None, "one past the end");
        assert_eq!(select_index(0, 0), None, "empty list");
    }

    #[test]
    fn ensure_visible_scrolls_up_to_a_selection_above_the_window() {
        assert_eq!(ensure_visible(2, 5, 10, 100), 2);
    }

    #[test]
    fn ensure_visible_scrolls_down_to_a_selection_below_the_window() {
        assert_eq!(ensure_visible(20, 0, 10, 100), 11);
    }

    #[test]
    fn ensure_visible_leaves_scroll_alone_when_selection_is_already_visible() {
        assert_eq!(ensure_visible(5, 3, 10, 100), 3);
    }

    #[test]
    fn ensure_visible_never_scrolls_past_the_end_of_a_short_list() {
        // 12 rows, a 10-row viewport: scroll can go no further than 2, even
        // for a selection right at the last row.
        assert_eq!(ensure_visible(11, 3, 10, 12), 2);
    }

    #[test]
    fn ensure_visible_clamps_a_zero_height_to_one_row() {
        assert_eq!(ensure_visible(5, 0, 0, 100), 5);
    }
}
