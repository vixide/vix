//! The ASCII reference table and the picker's row-selection + scroll state.
//!
//! Vix's Tools menu offers an *ASCII* panel: a scrollable table of the 128 ASCII
//! codes, each shown as its decimal value, two-digit hexadecimal value, and a
//! character representation (a control mnemonic such as `NUL`/`ESC`/`DEL`, the
//! word `space` for 32, or the literal glyph). The user browses with the arrow
//! keys (or the mouse) and inserts the highlighted character into the active
//! editor. This crate is pure data — it owns the table accessors and tracks the
//! highlighted row and the scroll offset. The host renders the rows, maps clicks
//! to rows, and inserts the chosen character.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Number of ASCII codes in the table (`0..=127`).
pub const LEN: usize = 128;

/// Mnemonics for the 32 control codes `0..=31`, in order. `DEL` (127) is handled
/// separately by [`label`].
const CONTROL: [&str; 32] = [
    "NUL", "SOH", "STX", "ETX", "EOT", "ENQ", "ACK", "BEL", "BS", "HT", "LF", "VT", "FF", "CR",
    "SO", "SI", "DLE", "DC1", "DC2", "DC3", "DC4", "NAK", "SYN", "ETB", "CAN", "EM", "SUB", "ESC",
    "FS", "GS", "RS", "US",
];

/// The decimal value of a code (the code itself; exposed for table symmetry).
#[must_use]
pub fn dec(code: u8) -> u8 {
    code
}

/// Two-digit uppercase hexadecimal for a code, e.g. `0F`, `7F`.
#[must_use]
pub fn hex(code: u8) -> String {
    format!("{code:02X}")
}

/// The character representation shown in the table's `Char` column: a control
/// mnemonic (`NUL`, `ESC`, `DEL`), the word `space` for 32, or the literal glyph.
#[must_use]
pub fn label(code: u8) -> String {
    match code {
        0..=31 => CONTROL[code as usize].to_string(),
        32 => "space".to_string(),
        127 => "DEL".to_string(),
        _ => (code as char).to_string(),
    }
}

/// The actual character for a code (`char::from(code)`) — what gets inserted into
/// the editor when a row is chosen.
#[must_use]
pub fn ch(code: u8) -> char {
    code as char
}

/// Whether a code is a non-printable control character (`0..=31` or 127).
#[must_use]
pub fn is_control(code: u8) -> bool {
    code < 32 || code == 127
}

/// Selection + scroll state for the ASCII panel overlay: a row index into the
/// table plus the first visible row, so the host can render a scrolling window.
pub struct Panel {
    /// Index of the highlighted row, `0..LEN`.
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
    /// Open the panel with the first row (`NUL`) highlighted.
    #[must_use]
    pub fn open() -> Self {
        Panel { selected: 0, scroll: 0 }
    }

    /// Total number of rows in the table.
    #[must_use]
    pub fn len(&self) -> usize {
        LEN
    }

    /// Whether the table is empty (it never is; clippy asks for this).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        LEN == 0
    }

    /// Move the highlight up one row, stopping at the top.
    pub fn up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Move the highlight down one row, stopping at the bottom.
    pub fn down(&mut self) {
        if self.selected + 1 < LEN {
            self.selected += 1;
        }
    }

    /// Move the highlight up one page (`page` rows), stopping at the top.
    pub fn page_up(&mut self, page: usize) {
        self.selected = self.selected.saturating_sub(page.max(1));
    }

    /// Move the highlight down one page (`page` rows), stopping at the bottom.
    pub fn page_down(&mut self, page: usize) {
        self.selected = (self.selected + page.max(1)).min(LEN - 1);
    }

    /// Select a row directly (e.g. from a mouse click); returns whether `idx`
    /// landed on a real row.
    pub fn select_index(&mut self, idx: usize) -> bool {
        if idx < LEN {
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
        let max_scroll = LEN.saturating_sub(height);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }

    /// The highlighted ASCII code.
    #[must_use]
    pub fn selected_code(&self) -> u8 {
        self.selected as u8
    }

    /// The highlighted row's character (what insertion uses).
    #[must_use]
    pub fn selected_char(&self) -> char {
        ch(self.selected_code())
    }

    /// The highlighted row's character representation (mnemonic or glyph).
    #[must_use]
    pub fn selected_label(&self) -> String {
        label(self.selected_code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_accessors_cover_the_ranges() {
        assert_eq!(dec(65), 65);
        assert_eq!(hex(0), "00");
        assert_eq!(hex(15), "0F");
        assert_eq!(hex(127), "7F");
        assert_eq!(label(0), "NUL");
        assert_eq!(label(27), "ESC");
        assert_eq!(label(32), "space");
        assert_eq!(label(65), "A");
        assert_eq!(label(127), "DEL");
        assert_eq!(ch(65), 'A');
        assert!(is_control(0) && is_control(31) && is_control(127));
        assert!(!is_control(32) && !is_control(65));
    }

    #[test]
    fn opens_on_the_first_row() {
        let p = Panel::open();
        assert_eq!(p.selected, 0);
        assert_eq!(p.selected_char(), '\0');
        assert_eq!(p.selected_label(), "NUL");
    }

    #[test]
    fn navigation_moves_and_clamps() {
        let mut p = Panel::open();
        p.up();
        assert_eq!(p.selected, 0, "up at the top stays put");
        p.down();
        assert_eq!(p.selected, 1);
        p.page_down(16);
        assert_eq!(p.selected, 17);
        p.page_up(100);
        assert_eq!(p.selected, 0, "page up clamps to the top");
        p.page_down(1000);
        assert_eq!(p.selected, LEN - 1, "page down clamps to the bottom");
        p.down();
        assert_eq!(p.selected, LEN - 1, "down at the bottom stays put");
    }

    #[test]
    fn ensure_visible_scrolls_to_keep_selection_in_view() {
        let mut p = Panel::open();
        p.selected = 50;
        p.ensure_visible(10);
        assert!(p.scroll <= 50 && 50 < p.scroll + 10);
        // Scrolling back up pulls the window with it.
        p.selected = 3;
        p.ensure_visible(10);
        assert_eq!(p.scroll, 3);
        // The window never scrolls past the end of the table.
        p.selected = LEN - 1;
        p.ensure_visible(10);
        assert_eq!(p.scroll, LEN - 10);
    }

    #[test]
    fn select_index_hits_real_rows_only() {
        let mut p = Panel::open();
        assert!(p.select_index(65));
        assert_eq!(p.selected_char(), 'A');
        assert!(!p.select_index(LEN));
        assert_eq!(p.selected, 65, "an out-of-range click leaves the selection put");
    }
}
