//! Align lines on a delimiter (`=`, `:`, `,`, `|`, …).
//!
//! Each line's first occurrence of the delimiter is padded so every delimiter
//! lands in the same column, with exactly one space before it and the text after
//! it preserved. Lines without the delimiter are left unchanged. Leading
//! indentation is preserved.
//!
//! ```
//! let out = vix_align::on_delimiter("a = 1\nbbb = 2\n", '=');
//! assert_eq!(out, "a   = 1\nbbb = 2\n");
//! ```

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Align every line of `text` that contains `delim` so the (first) delimiter sits
/// in a common column. Preserves a trailing newline if present.
#[must_use]
pub fn on_delimiter(text: &str, delim: char) -> String {
    let had_trailing_newline = text.ends_with('\n');
    let lines: Vec<&str> = text.split('\n').collect();

    // Split each line at its first delimiter into (left, right); `None` for lines
    // without the delimiter (left as-is). The alignment column is the widest
    // trimmed-left part among the split lines.
    let split: Vec<Option<(&str, &str)>> = lines.iter().map(|l| l.split_once(delim)).collect();
    let width = split
        .iter()
        .filter_map(|s| s.map(|(left, _)| left.trim_end().chars().count()))
        .max()
        .unwrap_or(0);

    let mut out = Vec::with_capacity(lines.len());
    for (line, part) in lines.iter().zip(&split) {
        match part {
            Some((left, right)) => {
                let left = left.trim_end();
                let pad = " ".repeat(width - left.chars().count());
                out.push(format!("{left}{pad} {delim} {}", right.trim_start()));
            }
            None => out.push((*line).to_string()),
        }
    }
    let mut joined = out.join("\n");
    if had_trailing_newline && !joined.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligns_assignments() {
        let input = "a = 1\nbbb = 2\ncc = 3\n";
        assert_eq!(on_delimiter(input, '='), "a   = 1\nbbb = 2\ncc  = 3\n");
    }

    #[test]
    fn leaves_lines_without_the_delimiter() {
        // The delimiter is normalized to one space each side; the comment line
        // (no delimiter) is untouched.
        let input = "xx: 1\n# a comment\ny: 2";
        assert_eq!(on_delimiter(input, ':'), "xx : 1\n# a comment\ny  : 2");
    }

    #[test]
    fn normalizes_spacing_around_the_delimiter() {
        // Ragged input collapses to exactly one space on each side.
        assert_eq!(on_delimiter("a=1\nbb   =   2", '='), "a  = 1\nbb = 2");
    }

    #[test]
    fn only_the_first_delimiter_splits() {
        assert_eq!(on_delimiter("k = a = b", '='), "k = a = b");
    }
}
