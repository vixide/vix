//! Convert CSV text into TSV (Tools → Convert → CSV → TSV).
//!
//! CSV quoting is honored on the way in; the tab-separated output has no quoting.
//! See [`crate::convert_tabular`] for the shared logic.

#![warn(clippy::pedantic)]

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Convert CSV `input` to TSV.
///
/// # Errors
/// Never fails today; returns `Result` for a uniform tool interface.
pub fn convert(input: &str) -> Result<String, String> {
    Ok(crate::convert_tabular::write_tsv(&crate::convert_tabular::parse_csv(input)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_and_unquotes() {
        assert_eq!(convert("a,b\n1,2\n").unwrap(), "a\tb\n1\t2\n");
        // A quoted comma is preserved as a literal field, not a delimiter.
        assert_eq!(convert("\"a,b\",c\n").unwrap(), "a,b\tc\n");
    }
}
