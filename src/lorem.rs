//! Lorem ipsum placeholder text generation for Tools → Insert → Lorem ipsum.
//!
//! Deterministic (no randomness): words, sentences, and paragraphs are derived
//! from a fixed canonical passage so output is stable and testable.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// The canonical lorem ipsum passage the generators draw from.
const PARAGRAPH: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, \
sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad \
minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea \
commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit \
esse cillum dolore eu fugiat nulla pariatur.";

/// The first `n` words of the passage (at least one), stripped of punctuation
/// and joined by spaces.
#[must_use]
pub fn words(n: usize) -> String {
    PARAGRAPH
        .split_whitespace()
        .map(|w| w.trim_end_matches([',', '.']))
        .take(n.max(1))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The passage's first sentence (ending in a period).
#[must_use]
pub fn sentence() -> String {
    PARAGRAPH
        .split_once(". ")
        .map_or_else(|| PARAGRAPH.to_string(), |(head, _)| format!("{head}."))
}

/// A full lorem ipsum paragraph.
#[must_use]
pub fn paragraph() -> String {
    PARAGRAPH.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_takes_the_first_n_without_punctuation() {
        assert_eq!(words(3), "Lorem ipsum dolor");
        assert!(!words(5).contains(','), "punctuation stripped");
        assert_eq!(words(0), "Lorem", "zero clamps to one word");
    }

    #[test]
    fn sentence_ends_with_a_single_period() {
        let s = sentence();
        assert!(s.ends_with('.'));
        assert!(!s[..s.len() - 1].contains('.'), "only the final period");
        assert!(s.starts_with("Lorem ipsum"));
    }

    #[test]
    fn paragraph_is_the_full_passage() {
        assert!(paragraph().contains("consectetur"));
        assert!(paragraph().contains("pariatur"));
    }
}
