//! Small pure whole-text transforms used by Edit/Tools actions: line-ending
//! conversion, blank-line squeezing, and ROT13.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Convert all line endings to LF (`\n`), dropping any `\r`.
#[must_use]
pub fn to_lf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Convert all line endings to CRLF (`\r\n`). Normalizes to LF first so mixed
/// input doesn't produce `\r\r\n`.
#[must_use]
pub fn to_crlf(text: &str) -> String {
    to_lf(text).replace('\n', "\r\n")
}

/// Collapse runs of two or more blank (empty or whitespace-only) lines into a
/// single empty line. A trailing newline is preserved.
#[must_use]
pub fn squeeze_blank_lines(text: &str) -> String {
    let had_trailing_newline = text.ends_with('\n');
    let mut out: Vec<&str> = Vec::new();
    let mut prev_blank = false;
    for line in text.split('\n') {
        let blank = line.trim().is_empty();
        if blank && prev_blank {
            continue;
        }
        out.push(line);
        prev_blank = blank;
    }
    let mut joined = out.join("\n");
    if had_trailing_newline && !joined.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

/// ROT13: rotate ASCII letters by 13 (its own inverse); other chars unchanged.
#[must_use]
pub fn rot13(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'a'..='z' => (b'a' + (c as u8 - b'a' + 13) % 26) as char,
            'A'..='Z' => (b'A' + (c as u8 - b'A' + 13) % 26) as char,
            _ => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_ending_conversions_round_trip() {
        assert_eq!(to_lf("a\r\nb\rc\n"), "a\nb\nc\n");
        assert_eq!(to_crlf("a\nb\n"), "a\r\nb\r\n");
        // Mixed input normalizes cleanly (no doubled \r).
        assert_eq!(to_crlf("a\r\nb\n"), "a\r\nb\r\n");
    }

    #[test]
    fn squeeze_collapses_runs_of_blanks() {
        assert_eq!(squeeze_blank_lines("a\n\n\n\nb\n"), "a\n\nb\n");
        // A single blank line is kept; whitespace-only counts as blank.
        assert_eq!(squeeze_blank_lines("a\n\nb"), "a\n\nb");
        assert_eq!(squeeze_blank_lines("a\n \n\t\nb"), "a\n \nb");
    }

    #[test]
    fn rot13_is_its_own_inverse() {
        assert_eq!(rot13("Hello, World!"), "Uryyb, Jbeyq!");
        assert_eq!(rot13(&rot13("Hello, World!")), "Hello, World!");
    }
}
