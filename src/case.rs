//! Text case transforms applied to the editor selection (Edit → Case).
//!
//! `upper` / `lower` / `title` preserve word separators; `kebab` / `snake` /
//! `camel` / `pascal` re-tokenize the text into words (splitting on separators
//! and camelCase humps) and rejoin in the target style.

/// `FOO BAR`
#[must_use]
pub fn upper(s: &str) -> String {
    s.to_uppercase()
}

/// `foo bar`
#[must_use]
pub fn lower(s: &str) -> String {
    s.to_lowercase()
}

/// `Foo Bar` — capitalize the first letter of each run of letters, leaving all
/// separators (spaces, `_`, `-`, …) in place.
#[must_use]
pub fn title(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut at_boundary = true;
    for c in s.chars() {
        if c.is_alphabetic() {
            if at_boundary {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            at_boundary = false;
        } else {
            out.push(c);
            at_boundary = true;
        }
    }
    out
}

/// `foo-bar`
#[must_use]
pub fn kebab(s: &str) -> String {
    join_words(s, "-", false)
}

/// `foo_bar`
#[must_use]
pub fn snake(s: &str) -> String {
    join_words(s, "_", false)
}

/// `fooBar` — first word lowercase, the rest capitalized, no separators.
#[must_use]
pub fn camel(s: &str) -> String {
    let mut out = String::new();
    for (i, w) in split_words(s).into_iter().enumerate() {
        if i == 0 {
            out.push_str(&w.to_lowercase());
        } else {
            out.push_str(&capitalize(&w));
        }
    }
    out
}

/// `FooBar` — every word capitalized, no separators.
#[must_use]
pub fn pascal(s: &str) -> String {
    split_words(s).iter().map(|w| capitalize(w)).collect()
}

/// Lowercase all words and join with `sep`. (`upper` is `false` — reserved for a
/// future SCREAMING style.)
fn join_words(s: &str, sep: &str, upper: bool) -> String {
    split_words(s)
        .into_iter()
        .map(|w| if upper { w.to_uppercase() } else { w.to_lowercase() })
        .collect::<Vec<_>>()
        .join(sep)
}

/// Capitalize the first character of `w`, lowercasing the rest.
fn capitalize(w: &str) -> String {
    let mut chars = w.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars.flat_map(char::to_lowercase)).collect(),
        None => String::new(),
    }
}

/// Split text into words, breaking on any non-alphanumeric separator and on
/// lowercase/digit → uppercase transitions (camelCase humps).
fn split_words(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut prev: Option<char> = None;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if let Some(p) = prev
                && (p.is_lowercase() || p.is_numeric()) && c.is_uppercase() && !cur.is_empty() {
                    words.push(std::mem::take(&mut cur));
                }
            cur.push(c);
        } else if !cur.is_empty() {
            words.push(std::mem::take(&mut cur));
        }
        prev = Some(c);
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upper_lower_title_preserve_separators() {
        assert_eq!(upper("foo bar"), "FOO BAR");
        assert_eq!(lower("Foo BAR"), "foo bar");
        assert_eq!(title("foo bar"), "Foo Bar");
        assert_eq!(title("foo_bar baz"), "Foo_Bar Baz");
    }

    #[test]
    fn kebab_and_snake_from_mixed_input() {
        assert_eq!(kebab("foo bar"), "foo-bar");
        assert_eq!(snake("foo bar"), "foo_bar");
        assert_eq!(kebab("fooBar baz"), "foo-bar-baz");
        assert_eq!(snake("Foo-Bar_baz"), "foo_bar_baz");
    }

    #[test]
    fn camel_and_pascal_from_mixed_input() {
        assert_eq!(camel("foo bar"), "fooBar");
        assert_eq!(pascal("foo bar"), "FooBar");
        assert_eq!(camel("foo-bar_baz"), "fooBarBaz");
        assert_eq!(pascal("fooBar baz"), "FooBarBaz");
    }

    #[test]
    fn split_handles_camel_humps_and_digits() {
        assert_eq!(split_words("getHTTPResponse"), vec!["get", "HTTPResponse"]);
        assert_eq!(split_words("foo2bar Baz"), vec!["foo2bar", "Baz"]);
    }

    #[test]
    fn empty_input_is_empty() {
        assert_eq!(camel(""), "");
        assert_eq!(pascal(""), "");
        assert_eq!(kebab("   "), "");
    }
}
