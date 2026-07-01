//! Find / find-and-replace for the editor.
//!
//! This crate owns both the box's *state* — the query and replacement text,
//! which field has focus, the case / whole-word / regex toggles, and the
//! [`SearchBar::pattern`] builder — and the *search/replace logic* over buffer
//! text: [`matches`], [`next_match`], [`replace_all`], [`replace_one`], and the
//! replacement-template [`unescape`]. All operate on `&str` with **character**
//! offsets; the host owns the buffer and applies the returned text.

#![warn(clippy::pedantic)]

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use regex::Regex;

/// Byte offset of character index `ch` in `text` (or `text.len()` if past the end).
fn char_to_byte(text: &str, ch: usize) -> usize {
    text.char_indices().nth(ch).map_or(text.len(), |(b, _)| b)
}

/// All matches of `re` in `text`, as `(char_start, char_end)` ranges in
/// ascending order. Byte→char conversion is done with one pass over the text.
#[must_use]
pub fn matches(text: &str, re: &Regex) -> Vec<(usize, usize)> {
    // Byte offset where each character starts; `partition_point` maps byte→char.
    let starts: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
    let to_char = |byte: usize| starts.partition_point(|&b| b < byte);
    re.find_iter(text).map(|m| (to_char(m.start()), to_char(m.end()))).collect()
}

/// The first match of `re` at or after character offset `from_char`, as a
/// `(char_start, char_end)` range, or `None` if there is none.
#[must_use]
pub fn next_match(text: &str, re: &Regex, from_char: usize) -> Option<(usize, usize)> {
    let from_byte = char_to_byte(text, from_char);
    let m = re.find_iter(text).find(|m| m.start() >= from_byte)?;
    Some((text[..m.start()].chars().count(), text[..m.end()].chars().count()))
}

/// Replace every match of `re` in `text`. In `regex_mode` the `replacement` is
/// [`unescape`]d and `$group` references expand; otherwise it is inserted
/// literally. Returns the new text and the number of replacements.
#[must_use]
pub fn replace_all(text: &str, re: &Regex, regex_mode: bool, replacement: &str) -> (String, usize) {
    let count = re.find_iter(text).count();
    let new = if regex_mode {
        let rep = unescape(replacement);
        re.replace_all(text, rep.as_str()).into_owned()
    } else {
        re.replace_all(text, regex::NoExpand(replacement)).into_owned()
    };
    (new, count)
}

/// Replace the single match of `re` whose start is exactly character offset
/// `at_char`. `template` is assumed already [`unescape`]d; in `regex_mode`
/// `$group` references expand, otherwise it is inserted literally. Returns the
/// new text and the char offset just past the inserted text (where searching
/// should resume), or `None` if no match starts at `at_char`.
#[must_use]
pub fn replace_one(
    text: &str,
    re: &Regex,
    regex_mode: bool,
    template: &str,
    at_char: usize,
) -> Option<(String, usize)> {
    let at_byte = char_to_byte(text, at_char);
    for caps in re.captures_iter(text) {
        let m = caps.get(0)?;
        if m.start() == at_byte {
            let mut exp = String::new();
            if regex_mode {
                caps.expand(template, &mut exp);
            } else {
                exp.push_str(template);
            }
            let mut new = String::with_capacity(text.len() + exp.len());
            new.push_str(&text[..m.start()]);
            new.push_str(&exp);
            new.push_str(&text[m.end()..]);
            let resume = at_char + exp.chars().count();
            return Some((new, resume));
        }
        if m.start() > at_byte {
            break;
        }
    }
    None
}

/// Interpret `\n`, `\t`, `\r`, `\\` escapes in a regex replacement template,
/// leaving `$group` references intact for the regex engine to expand.
#[must_use]
pub fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some('n') => {
                    out.push('\n');
                    chars.next();
                }
                Some('t') => {
                    out.push('\t');
                    chars.next();
                }
                Some('r') => {
                    out.push('\r');
                    chars.next();
                }
                Some('\\') => {
                    out.push('\\');
                    chars.next();
                }
                _ => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Which input field of the box has focus.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    /// The search-pattern field.
    Query,
    /// The replacement field.
    Replace,
    /// The "include paths matching this regex" filter (workspace search).
    IncludePath,
    /// The "exclude paths matching this regex" filter (workspace search).
    ExcludePath,
}

/// A path filter for workspace-wide search: optional include and exclude regexes
/// tested against a file path. Empty patterns mean "no constraint".
pub struct PathFilter {
    include: Option<Regex>,
    exclude: Option<Regex>,
}

impl PathFilter {
    /// Compile a filter from `include` / `exclude` patterns. An empty string
    /// disables that side. Invalid patterns are ignored (treated as empty), so a
    /// half-typed regex never hides every file.
    #[must_use]
    pub fn new(include: &str, exclude: &str) -> Self {
        let compile = |p: &str| (!p.is_empty()).then(|| Regex::new(p).ok()).flatten();
        PathFilter { include: compile(include), exclude: compile(exclude) }
    }

    /// Whether `path` passes the filter: it must match the include regex (when
    /// set) and must not match the exclude regex (when set).
    #[must_use]
    pub fn allows(&self, path: &str) -> bool {
        if let Some(inc) = &self.include
            && !inc.is_match(path) {
                return false;
            }
        if let Some(exc) = &self.exclude
            && exc.is_match(path) {
                return false;
            }
        true
    }
}

/// State of the find / find-and-replace box.
// Independent search toggles (replacing/interactive/case/word/regex); grouping
// them only relocates the lint and adds noise at every call site.
#[allow(clippy::struct_excessive_bools)]
pub struct SearchBar {
    /// Search-pattern text.
    pub query: String,
    /// Replacement text.
    pub replace: String,
    /// Replace mode shows and uses the replacement field.
    pub replacing: bool,
    /// Interactive (query-replace) mode: Enter begins step-through y/n/!/q.
    pub interactive: bool,
    /// Which input field has focus (only meaningful while replacing).
    pub field: Field,
    /// Match case exactly.
    pub case_sensitive: bool,
    /// Smart case: when `case_sensitive` is off, match case-insensitively only if
    /// the query has no uppercase letter; an uppercase letter makes it sensitive.
    pub smart_case: bool,
    /// Match whole words only.
    pub whole_word: bool,
    /// Treat the query as a regular expression.
    pub regex: bool,
    /// Last status, e.g. match count or "no matches".
    pub status: String,
}

impl SearchBar {
    /// A fresh box; `replacing` selects find-and-replace mode.
    #[must_use]
    pub fn new(replacing: bool) -> Self {
        SearchBar {
            query: String::new(),
            replace: String::new(),
            replacing,
            interactive: false,
            field: Field::Query,
            case_sensitive: false,
            smart_case: true,
            whole_word: false,
            regex: false,
            status: String::new(),
        }
    }

    /// Mutable access to the currently focused field's text. The box uses only
    /// [`Field::Query`] / [`Field::Replace`]; the path-filter fields are for
    /// workspace search, so they fall back to the query field here.
    pub fn active_field_mut(&mut self) -> &mut String {
        match self.field {
            Field::Replace => &mut self.replace,
            _ => &mut self.query,
        }
    }

    /// Switch focus between the query and replace fields (replace mode only).
    pub fn toggle_field(&mut self) {
        if self.replacing {
            self.field = match self.field {
                Field::Query => Field::Replace,
                _ => Field::Query,
            };
        }
    }

    /// Build the effective regex pattern from the query and the toggles.
    /// Returns `None` for an empty query.
    #[must_use]
    pub fn pattern(&self) -> Option<String> {
        if self.query.is_empty() {
            return None;
        }
        let mut core = if self.regex {
            self.query.clone()
        } else {
            regex::escape(&self.query)
        };
        if self.whole_word {
            core = format!(r"\b{core}\b");
        }
        // Case-insensitive unless the case toggle is on, or smart-case is enabled
        // and the query contains an uppercase letter.
        let has_upper = self.query.chars().any(char::is_uppercase);
        let insensitive = !(self.case_sensitive || self.smart_case && has_upper);
        if insensitive {
            core = format!("(?i){core}");
        }
        Some(core)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_on_the_query_field() {
        let s = SearchBar::new(true);
        assert_eq!(s.field, Field::Query);
        assert!(s.replacing);
    }

    #[test]
    fn toggle_field_only_switches_while_replacing() {
        let mut s = SearchBar::new(false);
        s.toggle_field();
        assert_eq!(s.field, Field::Query, "find-only mode never leaves Query");

        let mut s = SearchBar::new(true);
        s.toggle_field();
        assert_eq!(s.field, Field::Replace);
        s.toggle_field();
        assert_eq!(s.field, Field::Query);
    }

    #[test]
    fn active_field_mut_targets_the_focused_field() {
        let mut s = SearchBar::new(true);
        s.active_field_mut().push_str("foo");
        s.toggle_field();
        s.active_field_mut().push_str("bar");
        assert_eq!(s.query, "foo");
        assert_eq!(s.replace, "bar");
    }

    #[test]
    fn pattern_escapes_a_literal_query() {
        let mut s = SearchBar::new(false);
        s.query = "a.b".to_string();
        assert_eq!(s.pattern().as_deref(), Some(r"(?i)a\.b"));
    }

    #[test]
    fn pattern_respects_regex_word_and_case_toggles() {
        let mut s = SearchBar::new(false);
        s.query = "a.b".to_string();
        s.regex = true;
        s.case_sensitive = true;
        s.whole_word = true;
        assert_eq!(s.pattern().as_deref(), Some(r"\ba.b\b"));
    }

    #[test]
    fn empty_query_has_no_pattern() {
        let s = SearchBar::new(false);
        assert_eq!(s.pattern(), None);
    }

    #[test]
    fn smart_case_is_sensitive_only_with_an_uppercase_letter() {
        let mut s = SearchBar::new(false); // smart_case on by default
        s.query = "foo".to_string();
        assert_eq!(s.pattern().as_deref(), Some("(?i)foo"), "all-lowercase → insensitive");
        s.query = "Foo".to_string();
        assert_eq!(s.pattern().as_deref(), Some("Foo"), "uppercase → case-sensitive");
        // Turning smart-case off reverts to always-insensitive (unless case toggle).
        s.smart_case = false;
        assert_eq!(s.pattern().as_deref(), Some("(?i)Foo"));
    }

    fn re(p: &str) -> Regex {
        Regex::new(p).unwrap()
    }

    #[test]
    fn matches_returns_char_ranges_including_multibyte() {
        // "é" is two bytes; char offsets must not be byte offsets.
        let text = "é foo foo";
        let m = matches(text, &re("foo"));
        assert_eq!(m, vec![(2, 5), (6, 9)]);
    }

    #[test]
    fn next_match_finds_at_or_after_a_char_offset() {
        let text = "foo foo foo";
        assert_eq!(next_match(text, &re("foo"), 0), Some((0, 3)));
        assert_eq!(next_match(text, &re("foo"), 1), Some((4, 7)));
        assert_eq!(next_match(text, &re("foo"), 8), Some((8, 11)));
        assert_eq!(next_match(text, &re("foo"), 9), None);
    }

    #[test]
    fn replace_all_literal_and_regex() {
        let (out, n) = replace_all("a.b a.b", &re(r"a\.b"), false, "X");
        assert_eq!((out.as_str(), n), ("X X", 2));
        // Regex mode expands $groups and unescapes \n in the template.
        let (out, n) = replace_all("ab", &re("(a)(b)"), true, "$2$1\\n");
        assert_eq!((out.as_str(), n), ("ba\n", 1));
    }

    #[test]
    fn replace_one_replaces_only_the_match_at_offset() {
        let text = "foo foo";
        // Match at char 4 → resume after the inserted text.
        let (out, resume) = replace_one(text, &re("foo"), false, "BAR", 4).unwrap();
        assert_eq!(out, "foo BAR");
        assert_eq!(resume, 7);
        // No match starts at char 1.
        assert_eq!(replace_one(text, &re("foo"), false, "BAR", 1), None);
    }

    #[test]
    fn unescape_handles_known_escapes_and_leaves_groups() {
        assert_eq!(unescape(r"a\nb\tc"), "a\nb\tc");
        assert_eq!(unescape(r"\\"), "\\");
        assert_eq!(unescape(r"$1\x"), r"$1\x"); // unknown escape kept verbatim
    }

    #[test]
    fn path_filter_include_and_exclude() {
        // No constraints: everything passes.
        let f = PathFilter::new("", "");
        assert!(f.allows("src/app.rs"));

        // Include only Rust files.
        let f = PathFilter::new(r"\.rs$", "");
        assert!(f.allows("src/app.rs"));
        assert!(!f.allows("README.md"));

        // Exclude anything under target/.
        let f = PathFilter::new("", r"^target/");
        assert!(f.allows("src/app.rs"));
        assert!(!f.allows("target/debug/x"));

        // Both: Rust files not under tests/.
        let f = PathFilter::new(r"\.rs$", r"(^|/)tests/");
        assert!(f.allows("src/app.rs"));
        assert!(!f.allows("tests/integration.rs"));
        assert!(!f.allows("docs/x.md"));
    }

    #[test]
    fn path_filter_ignores_invalid_regex() {
        // An unclosed group is treated as "no constraint" rather than matching
        // nothing, so a half-typed pattern never hides every file.
        let f = PathFilter::new("(", "");
        assert!(f.allows("anything"));
    }
}
