//! Convert CSV text into TSV (Tools → Convert → CSV → TSV).
//!
//! CSV quoting is honored on the way in; the tab-separated output has no quoting.
//! See [`vix_convert_tabular`] for the shared logic.
//!
//! A thin adapter: it only composes the shared parser and writer. Spec:
//! `crates/vix-convert-from-csv-into-tsv-tool/spec/index.md`.

/// Convert CSV `input` to TSV.
///
/// Parses with [`vix_convert_tabular::parse_csv`] (quotes and embedded commas
/// honored) and renders with [`vix_convert_tabular::write_tsv`].
///
/// # Examples
/// ```
/// let tsv = vix_convert_from_csv_into_tsv_tool::convert("a,b\n1,2\n").unwrap();
/// assert_eq!(tsv, "a\tb\n1\t2\n");
/// ```
///
/// # Errors
/// Never fails today; returns `Result` for a uniform tool interface.
pub fn convert(input: &str) -> Result<String, vix_convert_tabular::ConvertError> {
    Ok(vix_convert_tabular::write_tsv(
        &vix_convert_tabular::parse_csv(input),
    ))
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
