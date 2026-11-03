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
//!
//! All three are pure `&str -> String` functions with no editor state, so the
//! host (the `vix` app's wrap-selection command, re-exported there as
//! `affix`) can apply them to any selection text and replace it with the
//! result. Matching is byte-wise on
//! UTF-8 string slices, so multi-byte prefixes and suffixes are handled
//! correctly. The names `add`/`drop`/`toggle` are meant to be called qualified
//! (`vix_affix::drop`), since `drop` shadows the prelude's [`std::mem::drop`]
//! when glob-imported.
//!
//! Spec: `crates/vix-affix/spec/index.md`.

/// Wrap `text` with `prefix` before and `suffix` after.
///
/// # Examples
///
/// ```
/// assert_eq!(vix_affix::add("x", "(", ")"), "(x)");
/// // Empty affixes are a no-op.
/// assert_eq!(vix_affix::add("x", "", ""), "x");
/// ```
#[must_use]
pub fn add(text: &str, prefix: &str, suffix: &str) -> String {
    format!("{prefix}{text}{suffix}")
}

/// Remove a leading `prefix` (if present) and a trailing `suffix` (if present)
/// from `text`. Anything not present is left as is, so the two sides are
/// independent: `drop("bravoalfa", "bravo", "charlie")` removes only the prefix.
///
/// # Examples
///
/// ```
/// assert_eq!(vix_affix::drop("(x)", "(", ")"), "x");
/// // Only the prefix is present, so only it is removed.
/// assert_eq!(vix_affix::drop("(x", "(", ")"), "x");
/// ```
#[must_use]
pub fn drop(text: &str, prefix: &str, suffix: &str) -> String {
    let without_prefix = text.strip_prefix(prefix).unwrap_or(text);
    without_prefix
        .strip_suffix(suffix)
        .unwrap_or(without_prefix)
        .to_string()
}

/// [`drop`] the pair when `text` is already wrapped (starts with `prefix` and
/// ends with `suffix`); otherwise [`add`] it.
///
/// Calling it twice returns the original text.
///
/// # Examples
///
/// ```
/// let once = vix_affix::toggle("x", "[", "]");
/// assert_eq!(once, "[x]");
/// assert_eq!(vix_affix::toggle(&once, "[", "]"), "x");
/// ```
#[must_use]
pub fn toggle(text: &str, prefix: &str, suffix: &str) -> String {
    // Require room for both affixes so a short string isn't mistaken for wrapped:
    // with prefix == suffix == "*", the one-char text "*" starts and ends with
    // "*" but is one star, not a wrapped empty string (which needs two).
    if text.len() >= prefix.len() + suffix.len()
        && text.starts_with(prefix)
        && text.ends_with(suffix)
    {
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
