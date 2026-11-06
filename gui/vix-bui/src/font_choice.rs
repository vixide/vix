//! The cell metrics the window is laid out with.
//!
//! BUI draws text with the one font it embeds (Inter, proportional) and has no
//! API to pick a family, so there is no font choice to make here, unlike the
//! other front ends. Every character is therefore placed at its exact cell
//! column by `crate::scene`, and the cell is sized for Inter's widest common
//! glyphs rather than measured from a monospace face.

/// Font size, in logical pixels.
pub const FONT_SIZE: f32 = 14.0;

/// Line height as a multiple of [`FONT_SIZE`].
pub const LINE_HEIGHT: f32 = 1.3;

/// The horizontal distance between two cell columns, in logical pixels.
///
/// Inter's average advance is about 0.56 em; 8 px at 14 px leaves a letter
/// pair a hair of air without a visible gap between neighbours.
pub const CELL_WIDTH: f32 = 8.0;

/// The cell height for [`FONT_SIZE`], in logical pixels.
#[must_use]
pub fn cell_height() -> f32 {
    (FONT_SIZE * LINE_HEIGHT).round()
}

/// How far to move a character right (negative: left) inside its cell so a
/// proportional glyph reads as if centered: Inter's `i`, `l` and `.` are far
/// narrower than a cell and its `m` and `W` wider. A rough, by-eye table,
/// since BUI exposes no text measurement.
#[must_use]
pub fn nudge(ch: char) -> f32 {
    match ch {
        'i' | 'l' | '.' | ',' | ':' | ';' | '!' | '|' | '\'' | '`' => 2.0,
        'j' | 't' | 'f' | 'I' | 'r' | '1' | '(' | ')' | '[' | ']' | '{' | '}' => 1.0,
        'm' | 'M' => -2.0,
        'w' | 'W' | '@' => -1.5,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narrow_glyphs_move_right_and_wide_ones_left() {
        assert!(nudge('l') > nudge('t'));
        assert!(nudge('t') > nudge('e'));
        assert!(nudge('m') < 0.0);
        assert!(nudge('e').abs() < f32::EPSILON);
        // No nudge pushes a glyph out of its cell.
        for c in ' '..='~' {
            assert!(nudge(c).abs() <= CELL_WIDTH / 3.0, "{c}");
        }
    }

    #[test]
    fn cell_is_taller_than_wide_and_whole_pixels() {
        assert!(cell_height() > CELL_WIDTH);
        assert!((cell_height() - cell_height().round()).abs() < f32::EPSILON);
        assert!((CELL_WIDTH - CELL_WIDTH.round()).abs() < f32::EPSILON);
    }
}
