//! The byte editor: a hex/ASCII view for editing raw file bytes.
//!
//! Vix's Tools menu offers an *Edit Bytes* command that shows the active
//! buffer's bytes as a classic hex dump — an offset column, sixteen hex byte
//! pairs, and an ASCII gutter — and lets the user move a byte cursor and
//! overwrite bytes by typing hex digits. Saving writes the bytes back.
//!
//! Editing is overwrite-only (no insert/delete) in this version, which keeps the
//! file length stable. The module owns the bytes, the cursor, and an undo
//! history, and interprets keys itself, returning an [`Outcome`] telling the host
//! when to close or save.
//!
//! Keys: **↑/↓/←/→** (or `k`/`j`/`h`/`l`) move the byte cursor; **Home/End** jump
//! to the row ends; **PageUp/PageDown** page; a **hex digit** (`0`–`9`, `a`–`f`)
//! overwrites the current byte's high then low nibble; **u**/**Ctrl+R**
//! undo/redo; **Ctrl+S** save; **Esc**/**q** close.

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

use vix_input::{KeyCode, KeyEvent, KeyModifiers};

/// Bytes shown per row.
pub const COLS: usize = 16;

/// Maximum number of undo steps retained.
const HISTORY_CAP: usize = 200;

/// The last column of a row.
const LAST_COL: usize = COLS - 1;

/// The row a byte index is on.
fn row_of(index: usize) -> usize {
    #[expect(
        clippy::integer_division,
        reason = "RCA-EXC-2: COLS is the non-zero constant 16 and the quotient is a row number"
    )]
    let row = index / COLS;
    row
}

/// The index of the first byte of the row holding `index`.
fn row_start(index: usize) -> usize {
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "RCA-EXC-2: row_of(index) * COLS is at most index"
    )]
    let start = row_of(index) * COLS;
    start
}

/// The bytes a page of `page` rows covers (at least one row); a count too
/// large to hold is `usize::MAX`, which movement then stops short of.
fn page_bytes(page: usize) -> usize {
    mul_or(page.max(1), COLS, usize::MAX)
}

/// The index of the last of `len` bytes; with none, 0.
fn last_index(len: usize) -> usize {
    sub_or(len, 1, 0)
}

/// `a * b`, or the declared `fallback` when the product does not fit.
fn mul_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_mul(b).unwrap_or(fallback)
}

/// `a - b`, or the declared `fallback` when it would go below zero. The
/// fallback is a parameter so each call names the declaration it comes from;
/// written inline, `checked_sub(..).unwrap_or(0)` is rewritten by Clippy to
/// `saturating_sub`, which the checked-arithmetic spec forbids.
fn sub_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_sub(b).unwrap_or(fallback)
}

/// `a + b`, or the declared `fallback` when the sum does not fit (see
/// `sub_or` for why the fallback is a parameter).
fn add_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_add(b).unwrap_or(fallback)
}

/// What the host should do after the editor handled a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Handled internally; nothing further for the host to do.
    Consumed,
    /// The user asked to close the editor (Esc/`q`).
    Close,
    /// The user asked to save (Ctrl+S); the host should persist the bytes.
    Save,
}

/// A snapshot for undo/redo.
#[derive(Clone)]
struct Snapshot {
    bytes: Vec<u8>,
    cursor: usize,
}

/// A hex/ASCII byte editor with a cursor and undo history.
pub struct Hex {
    bytes: Vec<u8>,
    /// Selected byte index.
    cursor: usize,
    /// Which nibble of the current byte the next hex digit overwrites
    /// (`false` = high, `true` = low).
    nibble: bool,
    /// First visible row (scroll offset).
    scroll: usize,
    /// Whether there are unsaved edits.
    dirty: bool,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Hex {
    /// Create an editor over `bytes`.
    #[must_use]
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Hex {
            bytes,
            cursor: 0,
            nibble: false,
            scroll: 0,
            dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Number of bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether there are no bytes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Number of display rows.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.bytes.len().div_ceil(COLS)
    }

    /// The selected byte index.
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// The first visible row (scroll offset).
    #[must_use]
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// Whether there are unsaved edits.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// The byte at index `i`, or 0 when out of range.
    #[must_use]
    pub fn byte(&self, i: usize) -> u8 {
        self.bytes.get(i).copied().unwrap_or(0)
    }

    /// The bytes (for saving).
    #[must_use]
    pub fn to_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Mark the editor as saved (called by the host after a successful write).
    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    /// Adjust the scroll so the cursor's row stays within a `height`-row window.
    pub fn ensure_visible(&mut self, height: usize) {
        let row = row_of(self.cursor);
        self.scroll = vix_list_state::ensure_visible(row, self.scroll, height, self.rows());
    }

    /// Interpret a key event and report what the host should do next.
    pub fn handle_key(&mut self, key: KeyEvent, page: usize) -> Outcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('s') {
            return Outcome::Save;
        }
        match key.code {
            // Movement stops at the first and last byte (a declared boundary).
            KeyCode::Left | KeyCode::Char('h') => self.move_to(sub_or(self.cursor, 1, 0)),
            KeyCode::Right | KeyCode::Char('l') => self.move_to(add_or(self.cursor, 1, usize::MAX)),
            KeyCode::Up | KeyCode::Char('k') => self.move_to(sub_or(self.cursor, COLS, 0)),
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_to(add_or(self.cursor, COLS, usize::MAX));
            }
            KeyCode::Home => self.move_to(row_start(self.cursor)),
            KeyCode::End => self.move_to(add_or(row_start(self.cursor), LAST_COL, usize::MAX)),
            KeyCode::PageUp => self.move_to(sub_or(self.cursor, page_bytes(page), 0)),
            KeyCode::PageDown => {
                self.move_to(add_or(self.cursor, page_bytes(page), usize::MAX));
            }
            KeyCode::Char('u') if !ctrl => self.undo(),
            KeyCode::Char('r') if ctrl => self.redo(),
            KeyCode::Char(c) if c.is_ascii_hexdigit() => self.type_nibble(c),
            KeyCode::Esc | KeyCode::Char('q') => return Outcome::Close,
            _ => {}
        }
        Outcome::Consumed
    }

    /// Move the cursor to `to` (clamped) and reset the nibble.
    fn move_to(&mut self, to: usize) {
        if self.bytes.is_empty() {
            return;
        }
        self.cursor = to.min(last_index(self.bytes.len()));
        self.nibble = false;
    }

    /// Overwrite the current byte's high then low nibble with hex digit `c`.
    fn type_nibble(&mut self, c: char) {
        if self.bytes.is_empty() {
            return;
        }
        let Some(d) = c.to_digit(16).and_then(|d| u8::try_from(d).ok()) else {
            return;
        };
        self.push_undo();
        let Some(slot) = self.bytes.get_mut(self.cursor) else {
            return;
        };
        let b = *slot;
        if self.nibble {
            *slot = (b & 0xf0) | d;
            self.nibble = false;
            self.cursor = add_or(self.cursor, 1, usize::MAX).min(last_index(self.bytes.len()));
        } else {
            *slot = d.rotate_left(4) | (b & 0x0f);
            self.nibble = true;
        }
        self.dirty = true;
    }

    /// Capture the current state onto the undo stack and clear redo.
    fn push_undo(&mut self) {
        // Delegates to `vix-capped-stack` (Run H, T525) -- this crate and
        // four siblings used to each hand-roll this identical clamp.
        vix_capped_stack::push_capped(
            &mut self.undo,
            Snapshot {
                bytes: self.bytes.clone(),
                cursor: self.cursor,
            },
            HISTORY_CAP,
        );
        self.redo.clear();
    }

    /// Undo the most recent change.
    fn undo(&mut self) {
        if let Some(snap) = self.undo.pop() {
            self.redo.push(Snapshot {
                bytes: self.bytes.clone(),
                cursor: self.cursor,
            });
            self.bytes = snap.bytes;
            self.cursor = snap.cursor.min(last_index(self.bytes.len()));
            self.nibble = false;
            self.dirty = true;
        }
    }

    /// Redo the most recently undone change.
    fn redo(&mut self) {
        if let Some(snap) = self.redo.pop() {
            self.undo.push(Snapshot {
                bytes: self.bytes.clone(),
                cursor: self.cursor,
            });
            self.bytes = snap.bytes;
            self.cursor = snap.cursor.min(last_index(self.bytes.len()));
            self.nibble = false;
            self.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn code(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn hex() -> Hex {
        Hex::from_bytes(b"hello world!".to_vec())
    }

    #[test]
    fn reports_dimensions() {
        let h = Hex::from_bytes(vec![0u8; 20]);
        assert_eq!(h.len(), 20);
        assert_eq!(h.rows(), 2, "20 bytes over 16 cols = 2 rows");
        assert!(!h.is_empty());
    }

    #[test]
    fn navigation_moves_and_clamps() {
        let mut h = hex(); // 12 bytes
        h.handle_key(code(KeyCode::Right), 4);
        assert_eq!(h.cursor(), 1);
        h.handle_key(code(KeyCode::Left), 4);
        h.handle_key(code(KeyCode::Left), 4);
        assert_eq!(h.cursor(), 0, "clamps at start");
        h.handle_key(code(KeyCode::End), 4);
        assert_eq!(
            h.cursor(),
            11,
            "End clamps to last byte (row shorter than 16)"
        );
        h.handle_key(code(KeyCode::Down), 4);
        assert_eq!(h.cursor(), 11, "down past end clamps");
    }

    #[test]
    fn overwrites_a_byte_with_two_nibbles() {
        let mut h = hex();
        // 'h' = 0x68; type "41" -> 0x41 = 'A'
        h.handle_key(key('4'), 4);
        assert_eq!(h.byte(0), 0x48, "high nibble set, cursor stays");
        assert_eq!(h.cursor(), 0);
        h.handle_key(key('1'), 4);
        assert_eq!(h.byte(0), 0x41, "low nibble set");
        assert_eq!(h.cursor(), 1, "cursor advances after low nibble");
        assert!(h.is_dirty());
        assert_eq!(h.to_bytes()[0], b'A');
    }

    #[test]
    fn moving_resets_the_nibble() {
        let mut h = hex();
        h.handle_key(key('4'), 4); // high nibble of byte 0
        h.handle_key(code(KeyCode::Right), 4); // move resets nibble
        h.handle_key(key('1'), 4); // high nibble of byte 1, not low of byte 0
        assert_eq!(h.byte(0), 0x48);
        assert_eq!(
            h.byte(1),
            0x15,
            "'e'=0x65 with high nibble set to 1 -> 0x15"
        );
    }

    #[test]
    fn undo_and_redo() {
        let mut h = hex();
        h.handle_key(key('4'), 4);
        h.handle_key(key('1'), 4);
        assert_eq!(h.byte(0), 0x41);
        h.handle_key(key('u'), 4);
        h.handle_key(key('u'), 4);
        assert_eq!(h.byte(0), b'h', "undo restores original byte");
        h.handle_key(ctrl('r'), 4);
        assert_eq!(h.byte(0), 0x48, "redo reapplies first nibble");
    }

    #[test]
    fn save_and_close_outcomes() {
        let mut h = hex();
        assert_eq!(h.handle_key(ctrl('s'), 4), Outcome::Save);
        assert_eq!(h.handle_key(key('q'), 4), Outcome::Close);
        assert_eq!(h.handle_key(code(KeyCode::Esc), 4), Outcome::Close);
    }
}
