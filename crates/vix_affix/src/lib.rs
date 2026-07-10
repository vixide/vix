//! Add, drop, and toggle a `prefix`/`suffix` pair around `text` (a conventional
//! wrap: the prefix goes before the text, the suffix after).
//!
//! ```
//! use vix_affix::{add, drop, toggle};
//! assert_eq!(add("alfa", "bravo", "charlie"), "bravoalfacharlie");
//! assert_eq!(drop("bravoalfacharlie", "bravo", "charlie"), "alfa");
//! assert_eq!(toggle("alfa", "bravo", "charlie"), "bravoalfacharlie");
//! assert_eq!(toggle("bravoalfacharlie", "bravo", "charlie"), "alfa");
//! ```
//!
//! So [`add`] returns `prefix + text + suffix`; [`drop`] removes a leading
//! `prefix` and a trailing `suffix`; [`toggle`] drops them when `text` is already
//! wrapped (starts with `prefix` and ends with `suffix`), otherwise adds them.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Wrap `text` with `prefix` before and `suffix` after.
#[must_use]
pub fn add(text: &str, prefix: &str, suffix: &str) -> String {
    format!("{prefix}{text}{suffix}")
}

/// Remove a leading `prefix` (if present) and a trailing `suffix` (if present)
/// from `text`. Anything not present is left as is.
#[must_use]
pub fn drop(text: &str, prefix: &str, suffix: &str) -> String {
    let without_prefix = text.strip_prefix(prefix).unwrap_or(text);
    without_prefix.strip_suffix(suffix).unwrap_or(without_prefix).to_string()
}

/// [`drop`] the pair when `text` is already wrapped (starts with `prefix` and
/// ends with `suffix`); otherwise [`add`] it.
#[must_use]
pub fn toggle(text: &str, prefix: &str, suffix: &str) -> String {
    // Require room for both affixes so a short string isn't mistaken for wrapped.
    if text.len() >= prefix.len() + suffix.len() && text.starts_with(prefix) && text.ends_with(suffix) {
        drop(text, prefix, suffix)
    } else {
        add(text, prefix, suffix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_wraps_prefix_text_suffix() {
        assert_eq!(add("alfa", "bravo", "charlie"), "bravoalfacharlie");
        assert_eq!(add("x", "(", ")"), "(x)");
        assert_eq!(add("x", "", ""), "x");
    }

    #[test]
    fn drop_removes_leading_prefix_and_trailing_suffix() {
        assert_eq!(drop("bravoalfacharlie", "bravo", "charlie"), "alfa");
        assert_eq!(drop("(x)", "(", ")"), "x");
        // Missing affixes leave the text unchanged.
        assert_eq!(drop("alfa", "bravo", "charlie"), "alfa");
        // Only the prefix present: just that is removed.
        assert_eq!(drop("bravoalfa", "bravo", "charlie"), "alfa");
    }

    #[test]
    fn toggle_round_trips() {
        assert_eq!(toggle("alfa", "bravo", "charlie"), "bravoalfacharlie");
        assert_eq!(toggle("bravoalfacharlie", "bravo", "charlie"), "alfa");
        // Toggling twice returns the original.
        let once = toggle("alfa", "bravo", "charlie");
        assert_eq!(toggle(&once, "bravo", "charlie"), "alfa");
    }
}
