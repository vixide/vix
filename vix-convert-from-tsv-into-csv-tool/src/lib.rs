//! Convert TSV text into CSV (Tools → Convert → TSV → CSV).
//!
//! Fields containing commas, quotes or newlines are RFC 4180 quoted on output.
//! See [`vix_convert_tabular`] for the shared logic.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Convert TSV `input` to CSV.
///
/// # Errors
/// Never fails today; returns `Result` for a uniform tool interface.
pub fn convert(input: &str) -> Result<String, String> {
    Ok(vix_convert_tabular::write_csv(&vix_convert_tabular::parse_tsv(input)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_and_quotes_when_needed() {
        assert_eq!(convert("a\tb\n1\t2\n").unwrap(), "a,b\n1,2\n");
        // A field that already contains a comma must be quoted in CSV.
        assert_eq!(convert("a,b\tc\n").unwrap(), "\"a,b\",c\n");
    }
}
