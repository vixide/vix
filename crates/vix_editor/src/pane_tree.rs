//! A binary tree of editor panes, for arbitrary split layouts (including a 2x2
//! grid). Each leaf shows a tab; each internal node splits its area in two by a
//! direction and ratio. Pure geometry + tree surgery so it is unit-testable; the
//! host ([`crate::editor`]) owns which leaf is focused and the editor widgets.
//!
//! Leaves are addressed by their **in-order index** (left-to-right / top-to-bottom
//! as laid out). The tree is capped at [`MAX_LEAVES`] panes.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use ratatui_core::layout::Rect;

use crate::editor::SplitDir;

/// Maximum number of panes (leaves) in the split tree.
pub const MAX_LEAVES: usize = 4;

/// One node of the split tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pane {
    /// A pane showing tab index `usize`.
    Leaf(usize),
    /// A split of two child panes by `dir`, the first taking `ratio` percent.
    Split {
        /// Side-by-side (`Vertical`) or stacked (`Horizontal`).
        dir: SplitDir,
        /// Percent of the area given to the first child (clamped 10..=90).
        ratio: u16,
        /// First child (left / top).
        first: Box<Pane>,
        /// Second child (right / bottom).
        second: Box<Pane>,
    },
}

/// One laid-out leaf: its in-order index, the tab it shows, and its rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeafBox {
    /// In-order leaf index.
    pub leaf: usize,
    /// Tab shown in this pane.
    pub tab: usize,
    /// Pane rectangle (before any per-pane scrollbar split).
    pub rect: Rect,
}

impl Pane {
    /// Number of leaves in this subtree.
    #[must_use]
    pub fn leaf_count(&self) -> usize {
        match self {
            Pane::Leaf(_) => 1,
            Pane::Split { first, second, .. } => first.leaf_count() + second.leaf_count(),
        }
    }

    /// The tab shown by each leaf, in in-order traversal order.
    #[must_use]
    pub fn leaf_tabs(&self) -> Vec<usize> {
        let mut out = Vec::new();
        self.collect_tabs(&mut out);
        out
    }

    fn collect_tabs(&self, out: &mut Vec<usize>) {
        match self {
            Pane::Leaf(t) => out.push(*t),
            Pane::Split { first, second, .. } => {
                first.collect_tabs(out);
                second.collect_tabs(out);
            }
        }
    }

    /// Clamp every leaf's tab to `< tab_count` (used after tabs close).
    pub fn clamp_tabs(&mut self, tab_count: usize) {
        match self {
            Pane::Leaf(t) => *t = (*t).min(tab_count.saturating_sub(1)),
            Pane::Split { first, second, .. } => {
                first.clamp_tabs(tab_count);
                second.clamp_tabs(tab_count);
            }
        }
    }

    /// Set the tab shown by the leaf at in-order index `n`.
    pub fn set_leaf_tab(&mut self, n: usize, tab: usize) {
        let mut i = 0;
        self.visit_leaf_mut(&mut i, n, &mut |t| *t = tab);
    }

    /// The tab shown by the leaf at in-order index `n`, if it exists.
    #[must_use]
    pub fn leaf_tab(&self, n: usize) -> Option<usize> {
        self.leaf_tabs().get(n).copied()
    }

    fn visit_leaf_mut(&mut self, i: &mut usize, target: usize, f: &mut impl FnMut(&mut usize)) {
        match self {
            Pane::Leaf(t) => {
                if *i == target {
                    f(t);
                }
                *i += 1;
            }
            Pane::Split { first, second, .. } => {
                first.visit_leaf_mut(i, target, f);
                second.visit_leaf_mut(i, target, f);
            }
        }
    }

    /// Split the leaf at in-order index `n` into two, the new `second` showing
    /// `new_tab`. No-op when the tree is already at [`MAX_LEAVES`].
    pub fn split_leaf(&mut self, n: usize, dir: SplitDir, new_tab: usize) {
        if self.leaf_count() >= MAX_LEAVES {
            return;
        }
        let mut i = 0;
        self.split_leaf_inner(&mut i, n, dir, new_tab);
    }

    fn split_leaf_inner(&mut self, i: &mut usize, target: usize, dir: SplitDir, new_tab: usize) {
        match self {
            Pane::Leaf(t) => {
                if *i == target {
                    let old = *t;
                    *self = Pane::Split {
                        dir,
                        ratio: 50,
                        first: Box::new(Pane::Leaf(old)),
                        second: Box::new(Pane::Leaf(new_tab)),
                    };
                }
                *i += 1;
            }
            Pane::Split { first, second, .. } => {
                first.split_leaf_inner(i, target, dir, new_tab);
                second.split_leaf_inner(i, target, dir, new_tab);
            }
        }
    }

    /// Remove the leaf at in-order index `n`, collapsing its parent so the sibling
    /// takes the parent's place. Returns the resulting tree, or `None` if the
    /// whole tree was that single leaf.
    #[must_use]
    pub fn remove_leaf(self, n: usize) -> Option<Pane> {
        let mut i = 0;
        self.remove_leaf_inner(&mut i, n)
    }

    fn remove_leaf_inner(self, i: &mut usize, target: usize) -> Option<Pane> {
        match self {
            Pane::Leaf(_) => {
                let here = *i;
                *i += 1;
                if here == target { None } else { Some(self) }
            }
            Pane::Split {
                dir,
                ratio,
                first,
                second,
            } => {
                let first = first.remove_leaf_inner(i, target);
                let second = second.remove_leaf_inner(i, target);
                match (first, second) {
                    (Some(a), Some(b)) => Some(Pane::Split {
                        dir,
                        ratio,
                        first: Box::new(a),
                        second: Box::new(b),
                    }),
                    (Some(only), None) | (None, Some(only)) => Some(only),
                    (None, None) => None,
                }
            }
        }
    }

    /// Lay out every leaf into `area`, returning their rectangles in in-order.
    #[must_use]
    pub fn layout(&self, area: Rect) -> Vec<LeafBox> {
        let mut out = Vec::new();
        let mut i = 0;
        self.layout_inner(area, &mut i, &mut out);
        out
    }

    fn layout_inner(&self, area: Rect, i: &mut usize, out: &mut Vec<LeafBox>) {
        match self {
            Pane::Leaf(tab) => {
                out.push(LeafBox {
                    leaf: *i,
                    tab: *tab,
                    rect: area,
                });
                *i += 1;
            }
            Pane::Split {
                dir,
                ratio,
                first,
                second,
            } => {
                let (a, _div, b) = split_rects(area, *dir, *ratio);
                first.layout_inner(a, i, out);
                second.layout_inner(b, i, out);
            }
        }
    }

    /// The divider rectangles (one per internal node), with their direction, for
    /// drawing the split lines.
    #[must_use]
    pub fn dividers(&self, area: Rect) -> Vec<(SplitDir, Rect)> {
        let mut out = Vec::new();
        self.dividers_inner(area, &mut out);
        out
    }

    fn dividers_inner(&self, area: Rect, out: &mut Vec<(SplitDir, Rect)>) {
        if let Pane::Split {
            dir,
            ratio,
            first,
            second,
        } = self
        {
            let (a, div, b) = split_rects(area, *dir, *ratio);
            out.push((*dir, div));
            first.dividers_inner(a, out);
            second.dividers_inner(b, out);
        }
    }

    /// The in-order leaf index whose rectangle contains `(col, row)`, if any.
    #[must_use]
    pub fn leaf_at(&self, area: Rect, col: u16, row: u16) -> Option<usize> {
        self.layout(area)
            .into_iter()
            .find(|b| contains(b.rect, col, row))
            .map(|b| b.leaf)
    }

    /// If `(col, row)` is on a divider, set that split's ratio from the pointer's
    /// position within the split's area; returns whether anything changed.
    pub fn resize_at(&mut self, area: Rect, col: u16, row: u16) -> bool {
        if let Pane::Split {
            dir,
            ratio,
            first,
            second,
        } = self
        {
            let (a, div, b) = split_rects(area, *dir, *ratio);
            if contains(div, col, row) {
                *ratio = ratio_from_pointer(area, *dir, col, row);
                return true;
            }
            return first.resize_at(a, col, row) || second.resize_at(b, col, row);
        }
        false
    }
}

/// Compute the (first, divider, second) rectangles of a split of `area`.
fn split_rects(area: Rect, dir: SplitDir, ratio: u16) -> (Rect, Rect, Rect) {
    let ratio = ratio.clamp(10, 90);
    match dir {
        SplitDir::Vertical => {
            let total = area.width;
            let w0 = u16::try_from(u32::from(total) * u32::from(ratio) / 100).unwrap_or(u16::MAX);
            let w0 = w0.clamp(1, total.saturating_sub(2).max(1));
            (
                Rect { width: w0, ..area },
                Rect {
                    x: area.x + w0,
                    y: area.y,
                    width: 1,
                    height: area.height,
                },
                Rect {
                    x: area.x + w0 + 1,
                    y: area.y,
                    width: total.saturating_sub(w0 + 1),
                    height: area.height,
                },
            )
        }
        SplitDir::Horizontal => {
            let total = area.height;
            let h0 = u16::try_from(u32::from(total) * u32::from(ratio) / 100).unwrap_or(u16::MAX);
            let h0 = h0.clamp(1, total.saturating_sub(2).max(1));
            (
                Rect { height: h0, ..area },
                Rect {
                    x: area.x,
                    y: area.y + h0,
                    width: area.width,
                    height: 1,
                },
                Rect {
                    x: area.x,
                    y: area.y + h0 + 1,
                    width: area.width,
                    height: total.saturating_sub(h0 + 1),
                },
            )
        }
    }
}

/// The ratio (10..=90) for a pointer at `(col, row)` within `area` along `dir`.
fn ratio_from_pointer(area: Rect, dir: SplitDir, col: u16, row: u16) -> u16 {
    let pct = match dir {
        SplitDir::Vertical if area.width > 1 => {
            u32::from(col.saturating_sub(area.x)) * 100 / u32::from(area.width)
        }
        SplitDir::Horizontal if area.height > 1 => {
            u32::from(row.saturating_sub(area.y)) * 100 / u32::from(area.height)
        }
        _ => 50,
    };
    u16::try_from(pct).unwrap_or(50).clamp(10, 90)
}

/// Whether `rect` contains `(col, row)`.
fn contains(rect: Rect, col: u16, row: u16) -> bool {
    col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        }
    }

    #[test]
    fn split_and_count_and_tabs() {
        let mut p = Pane::Leaf(0);
        p.split_leaf(0, SplitDir::Vertical, 1);
        assert_eq!(p.leaf_count(), 2);
        assert_eq!(p.leaf_tabs(), vec![0, 1]);
        // Split the second leaf to make a 3-pane tree.
        p.split_leaf(1, SplitDir::Horizontal, 2);
        assert_eq!(p.leaf_tabs(), vec![0, 1, 2]);
    }

    #[test]
    fn split_caps_at_max_leaves() {
        let mut p = Pane::Leaf(0);
        for t in 1..10 {
            p.split_leaf(0, SplitDir::Vertical, t);
        }
        assert_eq!(p.leaf_count(), MAX_LEAVES);
    }

    #[test]
    fn remove_collapses_parent() {
        let mut p = Pane::Leaf(0);
        p.split_leaf(0, SplitDir::Vertical, 1);
        p.split_leaf(1, SplitDir::Horizontal, 2); // leaves: 0,1,2
        let p = p.remove_leaf(1).unwrap();
        assert_eq!(p.leaf_tabs(), vec![0, 2]);
        let single = p.remove_leaf(0).unwrap();
        assert_eq!(single.leaf_tabs(), vec![2]);
        assert!(single.remove_leaf(0).is_none());
    }

    #[test]
    fn layout_partitions_the_area() {
        let mut p = Pane::Leaf(0);
        p.split_leaf(0, SplitDir::Vertical, 1);
        let boxes = p.layout(area());
        assert_eq!(boxes.len(), 2);
        assert_eq!(boxes[0].rect.x, 0);
        assert!(boxes[1].rect.x > boxes[0].rect.width); // second pane is to the right
        // The pointer in the left pane resolves to leaf 0.
        assert_eq!(p.leaf_at(area(), 5, 5), Some(0));
    }

    #[test]
    fn resize_changes_ratio_on_divider() {
        let mut p = Pane::Leaf(0);
        p.split_leaf(0, SplitDir::Vertical, 1);
        let div = p.dividers(area())[0].1;
        assert!(
            p.resize_at(area(), div.x, div.y),
            "pointer on divider resizes"
        );
        // Off a divider does nothing.
        assert!(!p.resize_at(area(), 1, 1));
    }
}
