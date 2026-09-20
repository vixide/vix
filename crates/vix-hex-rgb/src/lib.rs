//! Lenient `#RRGGBB`/`RRGGBB` hex-string to `(r, g, b)` byte-triple parsing,
//! treating any missing or invalid component as `0`.
//!
//! Extracted (Run H, T530) after this exact parser turned up hand-rolled
//! independently in `vix-editor-core` and `vix-base16` -- same lenient
//! policy, real duplication rather than domain-driven similarity. Not a
//! replacement for `vix-color-converter-tool::from_hex`, a deliberately
//! *stricter* public-facing API that rejects malformed input instead of
//! zero-filling it.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Parse a `#RRGGBB` (or `RRGGBB`) hex string into an `(r, g, b)` triple.
///
/// Any component that is missing or invalid is treated as `0`.
#[must_use]
pub fn rgb(hex: &str) -> (u8, u8, u8) {
    let hex = hex.trim_start_matches('#');
    let component = |range: std::ops::Range<usize>| {
        hex.get(range)
            .and_then(|s| u8::from_str_radix(s, 16).ok())
            .unwrap_or(0)
    };
    (component(0..2), component(2..4), component(4..6))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_well_formed_hex_string_with_or_without_a_leading_hash() {
        assert_eq!(rgb("#F0F8FF"), (0xF0, 0xF8, 0xFF));
        assert_eq!(rgb("f0f8ff"), (0xF0, 0xF8, 0xFF));
    }

    #[test]
    fn zero_fills_missing_or_invalid_components_instead_of_erroring() {
        assert_eq!(rgb(""), (0, 0, 0));
        assert_eq!(rgb("f0"), (0xF0, 0, 0));
        assert_eq!(rgb("zzzzzz"), (0, 0, 0));
        assert_eq!(rgb("#12"), (0x12, 0, 0));
    }
}
