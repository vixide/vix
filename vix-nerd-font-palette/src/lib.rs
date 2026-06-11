//! A curated set of [Nerd Font](https://www.nerdfonts.com/) glyphs and the
//! character picker's grid-selection state.
//!
//! Vix's Tools menu offers a *Nerd Font Palette*: a small grid of icon glyphs
//! the user can browse with the arrow keys (or the mouse) and insert into the
//! active editor. This crate is pure data — it lists the glyphs and tracks which
//! cell is highlighted in a fixed-width grid. The host renders the grid, maps
//! clicks to cells, and inserts the chosen glyph.
//!
//! The glyphs are drawn from the common Nerd Font ranges (Font Awesome, Devicons,
//! Powerline, Octicons) that almost every patched font ships, so the picker shows
//! something useful regardless of which Nerd Font the terminal uses. A glyph that
//! a particular font lacks simply renders as a fallback box; nothing breaks.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// One selectable glyph and a short, human-readable name.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Glyph {
    /// The glyph itself (a single `char`, typically in a private-use area).
    pub ch: char,
    /// A short name shown beside the grid (not translated; these are icon names).
    pub name: &'static str,
}

/// Number of columns in the picker grid. Navigation and the host's layout both
/// rely on this fixed width.
pub const COLS: usize = 8;

/// The curated glyphs, in grid order (row-major, [`COLS`] per row).
pub const GLYPHS: &[Glyph] = &[
    // Files & folders
    Glyph { ch: '\u{f07b}', name: "folder" },
    Glyph { ch: '\u{f07c}', name: "folder-open" },
    Glyph { ch: '\u{f15b}', name: "file" },
    Glyph { ch: '\u{f0c5}', name: "copy" },
    Glyph { ch: '\u{f0c7}', name: "save" },
    Glyph { ch: '\u{f019}', name: "download" },
    Glyph { ch: '\u{f093}', name: "upload" },
    Glyph { ch: '\u{f1f8}', name: "trash" },
    // Editing
    Glyph { ch: '\u{f040}', name: "pencil" },
    Glyph { ch: '\u{f044}', name: "edit" },
    Glyph { ch: '\u{f0ea}', name: "paste" },
    Glyph { ch: '\u{f0c4}', name: "cut" },
    Glyph { ch: '\u{f031}', name: "font" },
    Glyph { ch: '\u{f035}', name: "list" },
    Glyph { ch: '\u{f0e2}', name: "undo" },
    Glyph { ch: '\u{f01e}', name: "redo" },
    // Navigation & status
    Glyph { ch: '\u{f015}', name: "home" },
    Glyph { ch: '\u{f002}', name: "search" },
    Glyph { ch: '\u{f013}', name: "cog" },
    Glyph { ch: '\u{f085}', name: "cogs" },
    Glyph { ch: '\u{f021}', name: "refresh" },
    Glyph { ch: '\u{f023}', name: "lock" },
    Glyph { ch: '\u{f09c}', name: "unlock" },
    Glyph { ch: '\u{f024}', name: "flag" },
    // Symbols & marks
    Glyph { ch: '\u{f005}', name: "star" },
    Glyph { ch: '\u{f004}', name: "heart" },
    Glyph { ch: '\u{f00c}', name: "check" },
    Glyph { ch: '\u{f00d}', name: "close" },
    Glyph { ch: '\u{f067}', name: "plus" },
    Glyph { ch: '\u{f068}', name: "minus" },
    Glyph { ch: '\u{f071}', name: "warning" },
    Glyph { ch: '\u{f06a}', name: "exclamation" },
    // People & comms
    Glyph { ch: '\u{f007}', name: "user" },
    Glyph { ch: '\u{f0c0}', name: "users" },
    Glyph { ch: '\u{f0e0}', name: "envelope" },
    Glyph { ch: '\u{f075}', name: "comment" },
    Glyph { ch: '\u{f0f3}', name: "bell" },
    Glyph { ch: '\u{f1d8}', name: "paper-plane" },
    Glyph { ch: '\u{f0a1}', name: "bullhorn" },
    Glyph { ch: '\u{f0c1}', name: "link" },
    // Dev & tools
    Glyph { ch: '\u{f120}', name: "terminal" },
    Glyph { ch: '\u{f121}', name: "code" },
    Glyph { ch: '\u{f126}', name: "code-fork" },
    Glyph { ch: '\u{f188}', name: "bug" },
    Glyph { ch: '\u{f1c0}', name: "database" },
    Glyph { ch: '\u{f0eb}', name: "lightbulb" },
    Glyph { ch: '\u{f0e7}', name: "bolt" },
    Glyph { ch: '\u{f135}', name: "rocket" },
    // Media & misc
    Glyph { ch: '\u{f04b}', name: "play" },
    Glyph { ch: '\u{f04c}', name: "pause" },
    Glyph { ch: '\u{f04d}', name: "stop" },
    Glyph { ch: '\u{f06e}', name: "eye" },
    Glyph { ch: '\u{f017}', name: "clock" },
    Glyph { ch: '\u{f073}', name: "calendar" },
    Glyph { ch: '\u{f0f4}', name: "coffee" },
    Glyph { ch: '\u{e0a0}', name: "git-branch" },
];

/// Selection state for the Nerd Font palette overlay: an index into [`GLYPHS`]
/// navigated as a [`COLS`]-wide grid.
pub struct Palette {
    /// Index into [`GLYPHS`] of the highlighted cell.
    pub selected: usize,
}

impl Default for Palette {
    fn default() -> Self {
        Palette::open()
    }
}

impl Palette {
    /// Open the palette with the first glyph highlighted.
    #[must_use]
    pub fn open() -> Self {
        Palette { selected: 0 }
    }

    /// Total number of glyphs in the grid.
    #[must_use]
    pub fn len(&self) -> usize {
        GLYPHS.len()
    }

    /// Whether the grid is empty (it never is, but clippy asks for this).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        GLYPHS.is_empty()
    }

    /// Number of grid rows (the last row may be partial).
    #[must_use]
    pub fn rows(&self) -> usize {
        GLYPHS.len().div_ceil(COLS)
    }

    /// Move the highlight up one row, staying put on the top row.
    pub fn up(&mut self) {
        if self.selected >= COLS {
            self.selected -= COLS;
        }
    }

    /// Move the highlight down one row, staying within the populated grid.
    pub fn down(&mut self) {
        if self.selected + COLS < GLYPHS.len() {
            self.selected += COLS;
        }
    }

    /// Move the highlight one cell left, stopping at the first glyph.
    pub fn left(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Move the highlight one cell right, stopping at the last glyph.
    pub fn right(&mut self) {
        if self.selected + 1 < GLYPHS.len() {
            self.selected += 1;
        }
    }

    /// Select the cell at grid (`row`, `col`), if it holds a glyph. Returns
    /// whether the coordinates landed on a real cell (used for mouse hit-testing).
    pub fn select_at(&mut self, row: usize, col: usize) -> bool {
        if col >= COLS {
            return false;
        }
        let idx = row * COLS + col;
        if idx < GLYPHS.len() {
            self.selected = idx;
            true
        } else {
            false
        }
    }

    /// The highlighted glyph.
    #[must_use]
    pub fn selected_glyph(&self) -> char {
        GLYPHS[self.selected].ch
    }

    /// The highlighted glyph's name.
    #[must_use]
    pub fn selected_name(&self) -> &'static str {
        GLYPHS[self.selected].name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_on_first_glyph() {
        let p = Palette::open();
        assert_eq!(p.selected, 0);
        assert_eq!(p.selected_glyph(), GLYPHS[0].ch);
    }

    #[test]
    fn grid_navigation_moves_and_clamps() {
        let mut p = Palette::open();
        p.right();
        assert_eq!(p.selected, 1);
        p.left();
        assert_eq!(p.selected, 0);
        // Up/left at the origin stays put.
        p.up();
        p.left();
        assert_eq!(p.selected, 0);
        // Down moves a full row.
        p.down();
        assert_eq!(p.selected, COLS);
        p.up();
        assert_eq!(p.selected, 0);
    }

    #[test]
    fn down_never_leaves_the_populated_grid() {
        let mut p = Palette::open();
        for _ in 0..p.rows() + 2 {
            p.down();
        }
        assert!(p.selected < GLYPHS.len());
    }

    #[test]
    fn right_stops_at_the_last_glyph() {
        let mut p = Palette::open();
        for _ in 0..GLYPHS.len() + 5 {
            p.right();
        }
        assert_eq!(p.selected, GLYPHS.len() - 1);
    }

    #[test]
    fn select_at_hits_real_cells_only() {
        let mut p = Palette::open();
        assert!(p.select_at(1, 2));
        assert_eq!(p.selected, COLS + 2);
        // A column past the grid width is never a cell.
        assert!(!p.select_at(0, COLS));
        // A row past the populated grid is never a cell.
        assert!(!p.select_at(p.rows(), 0));
    }
}
