//! Small pure text transforms used by Edit/Tools actions.
//!
//! Two shapes live here: whole-text transforms (`&str -> String`: line-ending
//! conversion, blank-line squeezing, ROT13) and cursor-relative rewrites
//! (`(&str, usize) -> Option<(String, usize)>`: increment number, smart toggle,
//! transpose). The host applies the former via
//! `App::transform_selection_or_buffer` and the latter via
//! `App::rewrite_at_cursor`; everything here is unit-tested without a terminal.

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

/// Increment (or decrement, `delta = -1`) the integer at or immediately after the
/// cursor char offset `cursor` in `text`. Returns the rewritten text and the new
/// cursor offset (kept at the number's start), or `None` if no digit is found on
/// the cursor's line at/after the cursor. Handles an optional leading `-`.
#[must_use]
pub fn bump_number_at(text: &str, cursor: usize, delta: i64) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    // Search from the cursor to the end of the current line for a digit.
    let mut i = cursor.min(n);
    while i < n && chars[i] != '\n' && !chars[i].is_ascii_digit() {
        i += 1;
    }
    if i >= n || chars[i] == '\n' {
        return None;
    }
    // Expand left over digits, then include a leading '-' if present.
    let mut start = i;
    while start > 0 && chars[start - 1].is_ascii_digit() {
        start -= 1;
    }
    if start > 0 && chars[start - 1] == '-' {
        start -= 1;
    }
    let mut end = i;
    while end < n && chars[end].is_ascii_digit() {
        end += 1;
    }
    let token: String = chars[start..end].iter().collect();
    let value: i64 = token.parse().ok()?;
    let bumped = value.saturating_add(delta).to_string();
    let mut out: String = chars[..start].iter().collect();
    out.push_str(&bumped);
    out.extend(chars[end..].iter());
    Some((out, start))
}

/// Transpose the two characters around char offset `cursor` (Emacs `C-t`): swap
/// the char before the cursor with the one at it, advancing the cursor. At the
/// end of a line/buffer, swaps the last two characters. Never crosses newlines.
/// Returns the rewritten text and new cursor, or `None` if there is no pair.
#[must_use]
pub fn transpose_chars_at(text: &str, cursor: usize) -> Option<(String, usize)> {
    let mut chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    // The left index of the pair to swap.
    let i = if cursor >= 1 && cursor < n && chars[cursor] != '\n' {
        cursor - 1
    } else if cursor >= 2 {
        cursor - 2
    } else {
        return None;
    };
    if i + 1 >= n || chars[i] == '\n' || chars[i + 1] == '\n' {
        return None;
    }
    chars.swap(i, i + 1);
    Some((chars.iter().collect(), (i + 2).min(n)))
}

/// Transpose the word before the cursor with the word at/after it (Emacs `M-t`),
/// preserving the separator between them and leaving the cursor after the moved
/// pair. Returns `None` if two words can't be found.
#[must_use]
pub fn transpose_words_at(text: &str, cursor: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    // Start of the second word: the word containing the cursor, else the next one.
    let mut b = cursor.min(n);
    if b < n && is_word(chars[b]) {
        while b > 0 && is_word(chars[b - 1]) {
            b -= 1;
        }
    } else {
        while b < n && !is_word(chars[b]) {
            b += 1;
        }
    }
    if b >= n {
        return None;
    }
    let mut b_end = b;
    while b_end < n && is_word(chars[b_end]) {
        b_end += 1;
    }
    // The first word: the word ending before `b`.
    let mut a_end = b;
    while a_end > 0 && !is_word(chars[a_end - 1]) {
        a_end -= 1;
    }
    let mut a = a_end;
    while a > 0 && is_word(chars[a - 1]) {
        a -= 1;
    }
    if a == a_end {
        return None; // no preceding word
    }
    let word1: String = chars[a..a_end].iter().collect();
    let sep: String = chars[a_end..b].iter().collect();
    let word2: String = chars[b..b_end].iter().collect();
    let mut out: String = chars[..a].iter().collect();
    out.push_str(&word2);
    out.push_str(&sep);
    out.push_str(&word1);
    out.extend(chars[b_end..].iter());
    let new_cursor = a + word2.chars().count() + sep.chars().count() + word1.chars().count();
    Some((out, new_cursor))
}

/// Opposite-value pairs for [`smart_toggle_at`]. Word pairs are matched
/// whole-word and case-preserved; symbol pairs are matched literally.
const TOGGLE_WORDS: &[(&str, &str)] = &[
    ("true", "false"),
    ("yes", "no"),
    ("on", "off"),
    ("enable", "disable"),
    ("enabled", "disabled"),
    ("left", "right"),
    ("up", "down"),
    ("min", "max"),
    ("and", "or"),
];
const TOGGLE_SYMBOLS: &[(&str, &str)] = &[("&&", "||"), ("==", "!="), ("<=", ">="), ("<", ">"), ("++", "--")];

/// Toggle the boolean-ish token at char offset `cursor` to its opposite: word
/// pairs (`true`/`false`, `yes`/`no`, …) matched as whole words with case
/// preserved, or symbol pairs (`&&`/`||`, `==`/`!=`, …) at/around the cursor.
/// Returns the rewritten text and the cursor's new offset, or `None` if nothing
/// togglable is under the cursor.
#[must_use]
pub fn smart_toggle_at(text: &str, cursor: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let is_word = |c: char| c.is_alphanumeric() || c == '_';

    // Word pairs: find the identifier span covering the cursor (or just before it).
    let mut s = cursor.min(n);
    while s > 0 && is_word(chars[s - 1]) {
        s -= 1;
    }
    let mut e = s;
    while e < n && is_word(chars[e]) {
        e += 1;
    }
    if s < e {
        let word: String = chars[s..e].iter().collect();
        let lower = word.to_ascii_lowercase();
        for (a, b) in TOGGLE_WORDS {
            let to = if lower == *a { Some(*b) } else if lower == *b { Some(*a) } else { None };
            if let Some(to) = to {
                let replacement = match_case(&word, to);
                let mut out: String = chars[..s].iter().collect();
                out.push_str(&replacement);
                out.extend(chars[e..].iter());
                return Some((out, s));
            }
        }
    }

    // Symbol pairs: try each starting at, or one char before, the cursor.
    for (a, b) in TOGGLE_SYMBOLS {
        for start in [cursor, cursor.saturating_sub(1)] {
            for (from, to) in [(*a, *b), (*b, *a)] {
                let flen = from.chars().count();
                if start + flen <= n && chars[start..start + flen].iter().collect::<String>() == from {
                    let mut out: String = chars[..start].iter().collect();
                    out.push_str(to);
                    out.extend(chars[start + flen..].iter());
                    return Some((out, start));
                }
            }
        }
    }
    None
}

/// Recase `replacement` to match `sample`: all-upper, Titlecase, else lowercase.
fn match_case(sample: &str, replacement: &str) -> String {
    if sample.chars().all(|c| c.is_uppercase() || !c.is_alphabetic()) && sample.chars().any(char::is_uppercase) {
        replacement.to_ascii_uppercase()
    } else if sample.chars().next().is_some_and(char::is_uppercase) {
        let mut c = replacement.chars();
        c.next().map(|f| f.to_ascii_uppercase().to_string() + c.as_str()).unwrap_or_default()
    } else {
        replacement.to_string()
    }
}

/// The 0-based char column of `tag` in `line` when it appears as a whole word
/// (bounded by non-word characters), or `None`. Used by the TODO/FIXME finder.
#[must_use]
pub fn tag_column(line: &str, tag: &str) -> Option<usize> {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    for (col, (byte_idx, _)) in line.char_indices().enumerate() {
        if line[byte_idx..].starts_with(tag) {
            let before_ok = line[..byte_idx].chars().next_back().is_none_or(|c| !is_word(c));
            let after_ok = line[byte_idx + tag.len()..].chars().next().is_none_or(|c| !is_word(c));
            if before_ok && after_ok {
                return Some(col);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_number_increments_and_decrements() {
        // Cursor before the digits: increments in place, cursor at the number start.
        let (out, pos) = bump_number_at("x = 41;", 0, 1).unwrap();
        assert_eq!(out, "x = 42;");
        assert_eq!(pos, 4);
        // Decrement, cursor sitting on a digit.
        assert_eq!(bump_number_at("v9", 1, -1).unwrap().0, "v8");
        // Negative numbers: the leading '-' is part of the token.
        assert_eq!(bump_number_at("-1", 0, -1).unwrap().0, "-2");
        // No digit on the cursor's line → None.
        assert!(bump_number_at("no digits here", 0, 1).is_none());
        // A digit only on a later line is not reached from this line.
        assert!(bump_number_at("abc\n5", 0, 1).is_none());
    }

    #[test]
    fn transpose_chars_swaps_around_the_cursor() {
        // Cursor between 'a' and 'b' (offset 1): swap → "ba", cursor advances to 2.
        assert_eq!(transpose_chars_at("ab", 1), Some(("ba".to_string(), 2)));
        // At end of buffer: swap the last two.
        assert_eq!(transpose_chars_at("abc", 3), Some(("acb".to_string(), 3)));
        // No pair at the very start.
        assert!(transpose_chars_at("ab", 0).is_none());
        // Never across a newline.
        assert!(transpose_chars_at("a\nb", 1).is_none());
    }

    #[test]
    fn transpose_words_swaps_neighboring_words() {
        assert_eq!(transpose_words_at("foo bar", 5).unwrap().0, "bar foo");
        // Punctuation separator is preserved.
        assert_eq!(transpose_words_at("alpha, beta", 8).unwrap().0, "beta, alpha");
        // Only one word → nothing to do.
        assert!(transpose_words_at("solo", 0).is_none());
    }

    #[test]
    fn smart_toggle_flips_words_and_symbols() {
        // Word pair, case preserved.
        assert_eq!(smart_toggle_at("let ok = true;", 9).unwrap().0, "let ok = false;");
        assert_eq!(smart_toggle_at("v = FALSE", 4).unwrap().0, "v = TRUE");
        assert_eq!(smart_toggle_at("Yes", 0).unwrap().0, "No");
        // Symbol pair at the cursor.
        assert_eq!(smart_toggle_at("a && b", 2).unwrap().0, "a || b");
        assert_eq!(smart_toggle_at("x == y", 2).unwrap().0, "x != y");
        // Whole-word only: "online" is not "on".
        assert!(smart_toggle_at("online", 0).is_none());
        // Nothing togglable.
        assert!(smart_toggle_at("hello", 0).is_none());
    }

    #[test]
    fn tag_column_matches_whole_words_only() {
        assert_eq!(tag_column("// TODO: fix", "TODO"), Some(3));
        assert_eq!(tag_column("let todos = 1;", "TODO"), None, "identifier is not a tag");
        assert_eq!(tag_column("no tags here", "TODO"), None);
    }

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
