//! Add, drop, and toggle a `prefix`/`suffix` pair around `text`.
//!
//! The behavior follows these worked examples (the contract):
//!
//! ```
//! use vix::affix::{add, drop, toggle};
//! assert_eq!(add("alfa", "bravo", "charlie"), "alfabravocharlie");
//! assert_eq!(drop("alfabravocharlie", "bravo", "charlie"), "alfa");
//! assert_eq!(toggle("alfa", "bravo", "charlie"), "alfabravocharlie");
//! assert_eq!(toggle("alfabravocharlie", "bravo", "charlie"), "alfa");
//! ```
//!
//! So [`add`] appends `prefix` then `suffix` to `text`; [`drop`] removes a
//! trailing `suffix` and then a trailing `prefix`; [`toggle`] drops them when the
//! text already ends with `prefix` immediately followed by `suffix`, otherwise
//! adds them.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Append `prefix` then `suffix` to `text`.
#[must_use]
pub fn add(text: &str, prefix: &str, suffix: &str) -> String {
    format!("{text}{prefix}{suffix}")
}

/// Remove a trailing `suffix` (if present) and then a trailing `prefix` (if then
/// present) from `text`. Anything not present is left as is.
#[must_use]
pub fn drop(text: &str, prefix: &str, suffix: &str) -> String {
    let without_suffix = text.strip_suffix(suffix).unwrap_or(text);
    without_suffix.strip_suffix(prefix).unwrap_or(without_suffix).to_string()
}

/// [`drop`] the pair when `text` already ends with `prefix` immediately followed
/// by `suffix`; otherwise [`add`] it.
#[must_use]
pub fn toggle(text: &str, prefix: &str, suffix: &str) -> String {
    let wrapped = format!("{prefix}{suffix}");
    if text.ends_with(&wrapped) {
        drop(text, prefix, suffix)
    } else {
        add(text, prefix, suffix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_appends_prefix_then_suffix() {
        assert_eq!(add("alfa", "bravo", "charlie"), "alfabravocharlie");
        assert_eq!(add("x", "", ""), "x");
    }

    #[test]
    fn drop_removes_trailing_suffix_then_prefix() {
        assert_eq!(drop("alfabravocharlie", "bravo", "charlie"), "alfa");
        // Missing affixes leave the text unchanged.
        assert_eq!(drop("alfa", "bravo", "charlie"), "alfa");
        // Only the suffix present: just that is removed.
        assert_eq!(drop("alfacharlie", "bravo", "charlie"), "alfa");
    }

    #[test]
    fn toggle_round_trips() {
        assert_eq!(toggle("alfa", "bravo", "charlie"), "alfabravocharlie");
        assert_eq!(toggle("alfabravocharlie", "bravo", "charlie"), "alfa");
        // Toggling twice returns the original.
        let once = toggle("alfa", "bravo", "charlie");
        assert_eq!(toggle(&once, "bravo", "charlie"), "alfa");
    }
}
