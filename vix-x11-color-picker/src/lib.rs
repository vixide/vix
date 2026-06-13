//! The X11 color table and the picker's row-selection + scroll state.
//!
//! Vix's Tools menu offers an *X11 Colors* panel: a scrollable table of the
//! standard X11 colors, each shown as a swatch, its `#RRGGBB` hex string, and its
//! name. The user browses with the arrow keys (or the mouse) and inserts the
//! highlighted color's hex value into the active editor. This crate is pure data
//! — the color table is bundled as a TSV and parsed once on first use, and a
//! [`Panel`] tracks the highlighted row and scroll offset. The host renders the
//! rows, maps clicks to rows, and inserts the chosen hex.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::sync::OnceLock;

/// One X11 color: a name, its `#RRGGBB` hex string, and 8-bit RGB components.
pub struct Color {
    /// Color name, e.g. `"AliceBlue"`.
    pub name: &'static str,
    /// Hex string `#RRGGBB`, e.g. `"#F0F8FF"`.
    pub hex: &'static str,
    /// Red component (0–255).
    pub r: u8,
    /// Green component (0–255).
    pub g: u8,
    /// Blue component (0–255).
    pub b: u8,
}

/// The X11 color table, parsed once from the bundled TSV (the header row is
/// skipped). Never empty in practice.
#[must_use]
pub fn colors() -> &'static [Color] {
    static COLORS: OnceLock<Vec<Color>> = OnceLock::new();
    COLORS.get_or_init(|| {
        include_str!("x11-color-list.tsv")
            .lines()
            .skip(1)
            .filter_map(parse_line)
            .collect()
    })
}

/// Parse one `Name<TAB>#Hex<TAB>R<TAB>G<TAB>B` row; `None` for malformed lines.
fn parse_line(line: &'static str) -> Option<Color> {
    let mut f = line.split('\t');
    let name = f.next()?.trim();
    let hex = f.next()?.trim();
    let r = f.next()?.trim().parse().ok()?;
    let g = f.next()?.trim().parse().ok()?;
    let b = f.next()?.trim().parse().ok()?;
    if name.is_empty() {
        return None;
    }
    Some(Color { name, hex, r, g, b })
}

/// Selection + scroll state for the X11 color palette overlay: a row index into
/// [`colors`] plus the first visible row, so the host can render a scrolling
/// window.
pub struct Panel {
    /// Index of the highlighted row.
    pub selected: usize,
    /// First visible row, kept in sync with `selected` by [`Panel::ensure_visible`].
    pub scroll: usize,
}

impl Default for Panel {
    fn default() -> Self {
        Panel::open()
    }
}

impl Panel {
    /// Open the panel with the first color highlighted.
    #[must_use]
    pub fn open() -> Self {
        Panel { selected: 0, scroll: 0 }
    }

    /// Total number of colors in the table.
    #[must_use]
    pub fn len(&self) -> usize {
        colors().len()
    }

    /// Whether the table is empty (it never is; clippy asks for this).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        colors().is_empty()
    }

    /// Move the highlight up one row, stopping at the top.
    pub fn up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Move the highlight down one row, stopping at the bottom.
    pub fn down(&mut self) {
        if self.selected + 1 < self.len() {
            self.selected += 1;
        }
    }

    /// Move the highlight up one page (`page` rows), stopping at the top.
    pub fn page_up(&mut self, page: usize) {
        self.selected = self.selected.saturating_sub(page.max(1));
    }

    /// Move the highlight down one page (`page` rows), stopping at the bottom.
    pub fn page_down(&mut self, page: usize) {
        self.selected = (self.selected + page.max(1)).min(self.len().saturating_sub(1));
    }

    /// Select a row directly (e.g. from a mouse click); returns whether `idx`
    /// landed on a real row.
    pub fn select_index(&mut self, idx: usize) -> bool {
        if idx < self.len() {
            self.selected = idx;
            true
        } else {
            false
        }
    }

    /// Adjust [`scroll`](Self::scroll) so the highlighted row stays within a
    /// window of `height` visible rows.
    pub fn ensure_visible(&mut self, height: usize) {
        let height = height.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + height {
            self.scroll = self.selected + 1 - height;
        }
        let max_scroll = self.len().saturating_sub(height);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }

    /// The highlighted color.
    #[must_use]
    pub fn selected_color(&self) -> &'static Color {
        let i = self.selected.min(self.len().saturating_sub(1));
        &colors()[i]
    }

    /// The highlighted color's hex string (what insertion uses).
    #[must_use]
    pub fn selected_hex(&self) -> &'static str {
        self.selected_color().hex
    }

    /// The highlighted color's name.
    #[must_use]
    pub fn selected_name(&self) -> &'static str {
        self.selected_color().name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_parses_and_starts_with_alice_blue() {
        let c = colors();
        assert!(c.len() > 100, "the X11 table has well over 100 colors");
        assert_eq!(c[0].name, "AliceBlue");
        assert_eq!(c[0].hex, "#F0F8FF");
        assert_eq!((c[0].r, c[0].g, c[0].b), (240, 248, 255));
    }

    #[test]
    fn every_row_has_a_six_digit_hex() {
        for c in colors() {
            assert_eq!(c.hex.len(), 7, "{} hex should be #RRGGBB", c.name);
            assert!(c.hex.starts_with('#'));
        }
    }

    #[test]
    fn opens_on_the_first_row() {
        let p = Panel::open();
        assert_eq!(p.selected, 0);
        assert_eq!(p.selected_name(), "AliceBlue");
    }

    #[test]
    fn navigation_moves_and_clamps() {
        let mut p = Panel::open();
        let last = p.len() - 1;
        p.up();
        assert_eq!(p.selected, 0, "up at the top stays put");
        p.down();
        assert_eq!(p.selected, 1);
        p.page_down(1_000_000);
        assert_eq!(p.selected, last, "page down clamps to the bottom");
        p.down();
        assert_eq!(p.selected, last, "down at the bottom stays put");
        p.page_up(1_000_000);
        assert_eq!(p.selected, 0, "page up clamps to the top");
    }

    #[test]
    fn ensure_visible_keeps_selection_in_window() {
        let mut p = Panel::open();
        p.selected = 50;
        p.ensure_visible(10);
        assert!(p.scroll <= 50 && 50 < p.scroll + 10);
        p.selected = 3;
        p.ensure_visible(10);
        assert_eq!(p.scroll, 3);
    }

    #[test]
    fn select_index_hits_real_rows_only() {
        let mut p = Panel::open();
        assert!(p.select_index(5));
        assert_eq!(p.selected, 5);
        assert!(!p.select_index(p.len()));
        assert_eq!(p.selected, 5, "an out-of-range click leaves the selection put");
    }
}
