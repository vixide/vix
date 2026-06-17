//! Conversions between the editor's char-offset columns and the code-unit
//! columns LSP positions use, under the negotiated [`Encoding`].
//!
//! LSP positions are `(line, character)` where `character` counts code units in
//! the server's chosen encoding (UTF-16 by default). The editor counts Unicode
//! scalar values (chars). These helpers convert a *column within a single line*
//! both ways; the host pairs them with line-start offsets to map whole positions.

/// The position encoding negotiated with the server (`positionEncoding` in the
/// initialize result), deciding what a `character` column counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Encoding {
    /// UTF-8 code units (bytes).
    Utf8,
    /// UTF-16 code units — the LSP default.
    #[default]
    Utf16,
    /// UTF-32 code units (one per char).
    Utf32,
}

impl Encoding {
    /// Parse the LSP `positionEncoding` string; unknown values fall back to the
    /// UTF-16 default.
    #[must_use]
    pub fn from_lsp(name: &str) -> Self {
        match name {
            "utf-8" => Encoding::Utf8,
            "utf-32" => Encoding::Utf32,
            _ => Encoding::Utf16,
        }
    }

    /// The number of code units one char occupies in this encoding.
    fn units(self, ch: char) -> u32 {
        match self {
            Encoding::Utf8 => ch.len_utf8() as u32,
            Encoding::Utf16 => ch.len_utf16() as u32,
            Encoding::Utf32 => 1,
        }
    }
}

/// Convert an encoding `col` within `line` to a 0-based char index. A column past
/// the end of the line clamps to the line's char length.
#[must_use]
pub fn col_to_char(line: &str, col: u32, enc: Encoding) -> usize {
    let mut units = 0u32;
    for (char_idx, ch) in line.chars().enumerate() {
        if units >= col {
            return char_idx;
        }
        units += enc.units(ch);
    }
    line.chars().count()
}

/// Convert a 0-based char index within `line` to an encoding `col`. A char index
/// past the end clamps to the line's total width.
#[must_use]
pub fn char_to_col(line: &str, char_idx: usize, enc: Encoding) -> u32 {
    let mut units = 0u32;
    for (i, ch) in line.chars().enumerate() {
        if i >= char_idx {
            break;
        }
        units += enc.units(ch);
    }
    units
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_columns_are_one_to_one() {
        let line = "let x = 1;";
        for enc in [Encoding::Utf8, Encoding::Utf16, Encoding::Utf32] {
            assert_eq!(col_to_char(line, 4, enc), 4);
            assert_eq!(char_to_col(line, 4, enc), 4);
        }
    }

    #[test]
    fn astral_chars_count_per_encoding() {
        // "💡x": the emoji is 1 char, 2 UTF-16 units, 4 UTF-8 bytes.
        let line = "💡x";
        assert_eq!(char_to_col(line, 1, Encoding::Utf16), 2);
        assert_eq!(char_to_col(line, 1, Encoding::Utf8), 4);
        assert_eq!(char_to_col(line, 1, Encoding::Utf32), 1);
        // The char after the emoji ('x') is char index 1 in every encoding.
        assert_eq!(col_to_char(line, 2, Encoding::Utf16), 1);
        assert_eq!(col_to_char(line, 4, Encoding::Utf8), 1);
        assert_eq!(col_to_char(line, 1, Encoding::Utf32), 1);
    }

    #[test]
    fn overshoot_clamps_to_line_length() {
        let line = "abc";
        assert_eq!(col_to_char(line, 99, Encoding::Utf16), 3);
        assert_eq!(char_to_col(line, 99, Encoding::Utf16), 3);
    }

    #[test]
    fn from_lsp_defaults_to_utf16() {
        assert_eq!(Encoding::from_lsp("utf-8"), Encoding::Utf8);
        assert_eq!(Encoding::from_lsp("utf-32"), Encoding::Utf32);
        assert_eq!(Encoding::from_lsp("utf-16"), Encoding::Utf16);
        assert_eq!(Encoding::from_lsp("weird"), Encoding::Utf16);
    }
}
