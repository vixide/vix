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
//! `tui/src/app.rs`/`tui/src/ui/*.rs`) or wrapping them in an extra field
//! (`panel.cursor.selected` instead of `panel.selected`), and several
//! panels don't store a plain index at all — `vix-edit-bytes` derives a row
//! from a byte cursor (`cursor / COLS`), `vix-edit-outline` derives a
//! position from a computed visible-node list. Free functions over plain
//! `usize`s fit every one of these without changing any panel's public
//! shape; only each panel's *method bodies* change.
//!
//! All functions are pure and total: none panics, including on an empty list
//! (`len == 0`) or a zero-height viewport.
//!
//! Arithmetic follows the `rust-checked-arithmetic` spec: every step uses
//! `checked_*` with the fallback the owning spec declares (`LIST-F1`..`LIST-F4`).
//!
//! Owning spec: `crates/vix-list-state/spec/index.md`.
//!
//! # Examples
//!
//! ```
//! use vix_list_state::{down, ensure_visible, page_down};
//! let mut selected = 0;
//! let mut scroll = 0;
//! for _ in 0..7 {
//!     selected = down(selected, 20);
//! }
//! // A 5-row viewport scrolls just enough to keep row 7 on screen.
//! scroll = ensure_visible(selected, scroll, 5, 20);
//! assert_eq!((selected, scroll), (7, 3));
//! assert_eq!(page_down(selected, 10, 20), 17);
//! ```

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

/// `minuend - subtrahend`, or the declared `fallback` when it would go below
/// zero. The fallback is a parameter so each call site names (and cites) the
/// declaration it comes from; `checked_sub(..).unwrap_or(0)` written inline
/// is rewritten by Clippy to `saturating_sub`, which the checked-arithmetic
/// spec forbids (RCA-INT-4).
fn sub_or(minuend: usize, subtrahend: usize, fallback: usize) -> usize {
    minuend.checked_sub(subtrahend).unwrap_or(fallback)
}

/// The highlighted row one step up, stopping at the top (row 0).
#[must_use]
pub fn up(selected: usize) -> usize {
    sub_or(selected, 1, 0) // LIST-F1: stop at the top
}

/// The highlighted row one step down, stopping at the last of `len` rows.
/// `len == 0` leaves `selected` unchanged (there is nothing to move onto).
#[must_use]
pub fn down(selected: usize, len: usize) -> usize {
    selected
        .checked_add(1)
        .filter(|next| *next < len)
        .unwrap_or(selected) // LIST-F2: stay on the last row
}

/// The highlighted row one step up, wrapping from row 0 to the last of `len`
/// rows. `len == 0` yields row 0 (there is nothing to wrap onto) instead of
/// the divide-by-zero the hand-rolled `(selected + len - 1) % len` hit.
#[must_use]
pub fn wrap_up(selected: usize, len: usize) -> usize {
    // LIST-F3: no rows, so row 0.
    let Some(current) = selected.checked_rem(len) else {
        return 0;
    };
    current
        .checked_sub(1)
        .or_else(|| len.checked_sub(1))
        .unwrap_or(0) // LIST-F3: row 0 wraps to the last row
}

/// The highlighted row one step down, wrapping from the last of `len` rows
/// to row 0. `len == 0` yields row 0.
#[must_use]
pub fn wrap_down(selected: usize, len: usize) -> usize {
    // LIST-F3: no rows, so row 0.
    let Some(current) = selected.checked_rem(len) else {
        return 0;
    };
    current
        .checked_add(1)
        .filter(|next| *next < len)
        .unwrap_or(0) // LIST-F3: the last row wraps to row 0
}

/// The highlighted row `page` steps up (minimum 1), stopping at the top.
#[must_use]
pub fn page_up(selected: usize, page: usize) -> usize {
    sub_or(selected, page.max(1), 0) // LIST-F1: stop at the top
}

/// The highlighted row `page` steps down (minimum 1), stopping at the last
/// of `len` rows. `len == 0` clamps to row 0.
#[must_use]
pub fn page_down(selected: usize, page: usize, len: usize) -> usize {
    let last = sub_or(len, 1, 0); // LIST-F2: no rows, so row 0
    selected
        .checked_add(page.max(1))
        .unwrap_or(last) // LIST-F2: past the end of `usize` is past the last row
        .min(last) // LIST-F2: stop at the last row
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
    // Minimal scrolling: only move the window when the selection has left it.
    let below = scroll
        .checked_add(height)
        .is_some_and(|window_end| selected >= window_end);
    let scroll = if selected < scroll {
        selected
    } else if below {
        selected
            .checked_add(1)
            .and_then(|end| end.checked_sub(height))
            .unwrap_or(selected) // LIST-F4: put the selection at the top
    } else {
        scroll
    };
    // Final clamp: after the list shrinks (filter, deletion) a stale offset
    // must not leave blank rows below the last item.
    let max_scroll = sub_or(len, height, 0); // LIST-F4: the list fits
    scroll.min(max_scroll) // LIST-F4: no blank rows below the last item
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

    #[test]
    fn page_down_adds_the_page_size() {
        // Mutation T606: `+` -> `*` survived because no test used selected > 1.
        assert_eq!(page_down(2, 3, 100), 5);
        assert_eq!(page_down(7, 4, 100), 11);
        assert_eq!(page_down(2, 0, 100), 3, "page 0 still moves one row");
    }

    #[test]
    fn ensure_visible_scrolls_when_selection_sits_just_below_the_window() {
        // Row 5 is the first row outside a 5-row window starting at 0.
        assert_eq!(ensure_visible(5, 0, 5, 100), 1);
        // Row 4 is the last visible row: no scroll.
        assert_eq!(ensure_visible(4, 0, 5, 100), 0);
    }

    #[test]
    fn ensure_visible_keeps_a_selection_on_the_top_edge() {
        assert_eq!(ensure_visible(3, 4, 5, 100), 3);
        assert_eq!(ensure_visible(4, 4, 5, 100), 4, "on the top edge: stays");
    }

    #[test]
    fn ensure_visible_scrolls_down_from_a_nonzero_offset() {
        // Mutation T606: `scroll + height` -> `scroll * height` only differs
        // once scroll > 0 and height > 2.
        assert_eq!(ensure_visible(8, 3, 5, 100), 4, "first row past the window");
        assert_eq!(ensure_visible(7, 3, 5, 100), 3, "last visible row: stays");
        assert_eq!(ensure_visible(9, 3, 5, 100), 5);
    }

    #[test]
    fn wrapping_steps_cycle_through_every_row() {
        assert_eq!(wrap_down(0, 3), 1);
        assert_eq!(wrap_down(2, 3), 0, "past the last row wraps to the first");
        assert_eq!(wrap_up(1, 3), 0);
        assert_eq!(wrap_up(0, 3), 2, "before the first row wraps to the last");
        assert_eq!(wrap_up(0, 1), 0);
        assert_eq!(wrap_down(0, 1), 0);
    }

    #[test]
    fn wrapping_steps_never_divide_by_zero_and_tolerate_a_stale_selection() {
        assert_eq!(wrap_up(0, 0), 0);
        assert_eq!(wrap_down(0, 0), 0);
        assert_eq!(wrap_up(5, 0), 0);
        // A selection left past the end of a list that shrank is folded in.
        assert_eq!(wrap_down(7, 3), 2);
        assert_eq!(wrap_up(7, 3), 0);
    }

    /// The checked-arithmetic spec: no input panics or wraps, even at the
    /// edge of `usize` (a plain `selected + 1` would overflow here).
    #[test]
    fn extreme_values_neither_panic_nor_wrap() {
        let max = usize::MAX;
        assert_eq!(down(max, max), max, "LIST-F2: already on the last row");
        assert_eq!(down(max, 5), max, "LIST-F2: a stale selection stays put");
        assert_eq!(page_down(max, 10, 5), 4, "LIST-F2: clamps to the last row");
        assert_eq!(page_down(3, max, 5), 4, "LIST-F2: a huge page clamps");
        assert_eq!(page_down(3, 10, 0), 0, "LIST-F2: no rows, so row 0");
        assert_eq!(page_up(3, max), 0, "LIST-F1: a huge page stops at the top");
        assert_eq!(wrap_down(max, 3), (max % 3 + 1) % 3);
        assert_eq!(wrap_up(max, 3), (max % 3 + 2) % 3);
        assert_eq!(wrap_up(0, 3), 2, "LIST-F3: row 0 wraps to the last row");
        assert_eq!(wrap_down(2, 3), 0, "LIST-F3: the last row wraps to row 0");
        assert_eq!(wrap_up(7, 0), 0);
        assert_eq!(wrap_down(7, 0), 0);
        // `selected + 1` overflows: LIST-F4 puts the selection at the top of
        // the window, and the list clamp then stops at `len - height`.
        assert_eq!(ensure_visible(max, 0, 5, max), max - 5);
        assert_eq!(
            ensure_visible(max, max, 5, 10),
            5,
            "LIST-F4: clamped to the list"
        );
        assert_eq!(
            ensure_visible(3, 0, usize::MAX, 10),
            0,
            "window larger than the list"
        );
    }
}
