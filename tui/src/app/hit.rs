//! Mouse hit-testing shared by the panel handlers.
//!
//! Every list-shaped overlay answered a click the same way: ignore anything
//! but a left press, ignore a press outside the panel's rectangle, then take
//! the press's offset from the rectangle's top as the row. Each handler
//! spelled that out by hand; this is the one copy, a pure function a test can
//! drive without building an [`App`](super::App) or drawing a frame.

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

use crate::geo::Geo;
use ratatui::layout::Rect;
use vix_input::{MouseButton, MouseEvent, MouseEventKind};

/// Whether `(col, row)` lies inside `r`.
pub(super) fn rect_has(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x.plus(r.width) && row >= r.y && row < r.y.plus(r.height)
}

/// The zero-based row, counted from the top of `rect`, that a left-button
/// press lands on; `None` for any other mouse event or a press outside
/// `rect`. The caller adds its own scroll offset where the list scrolls.
pub(super) fn left_click_row(rect: Rect, mouse: MouseEvent) -> Option<usize> {
    (matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && rect_has(rect, mouse.column, mouse.row))
    .then(|| usize::from(mouse.row.minus(rect.y)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vix_input::KeyModifiers;

    fn at(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    const R: Rect = Rect {
        x: 10,
        y: 5,
        width: 20,
        height: 4,
    };

    #[test]
    fn a_left_press_inside_reports_its_row_from_the_top() {
        let left = MouseEventKind::Down(MouseButton::Left);
        assert_eq!(left_click_row(R, at(left, 10, 5)), Some(0));
        assert_eq!(
            left_click_row(R, at(left, 29, 8)),
            Some(3),
            "bottom-right cell"
        );
    }

    #[test]
    fn presses_outside_the_rectangle_are_ignored_on_every_edge() {
        let left = MouseEventKind::Down(MouseButton::Left);
        for (col, row) in [(9, 5), (30, 5), (10, 4), (10, 9)] {
            assert_eq!(left_click_row(R, at(left, col, row)), None, "{col},{row}");
        }
    }

    #[test]
    fn only_a_left_press_counts() {
        for kind in [
            MouseEventKind::Down(MouseButton::Right),
            MouseEventKind::Up(MouseButton::Left),
            MouseEventKind::Drag(MouseButton::Left),
            MouseEventKind::Moved,
            MouseEventKind::ScrollDown,
        ] {
            assert_eq!(left_click_row(R, at(kind, 12, 6)), None, "{kind:?}");
        }
    }

    #[test]
    fn an_empty_rectangle_contains_nothing() {
        let left = MouseEventKind::Down(MouseButton::Left);
        let empty = Rect::new(3, 3, 0, 0);
        assert_eq!(left_click_row(empty, at(left, 3, 3)), None);
    }
}
