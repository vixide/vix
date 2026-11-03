//! Find / find-and-replace for the editor.
//!
//! This crate owns both the box's *state* — the query and replacement text,
//! which field has focus, the case / whole-word / regex toggles, and the
//! [`SearchBar::pattern`] builder — and the *search/replace logic* over buffer
//! text: [`matches()`], [`next_match`], [`replace_all`], [`replace_one`], and the
//! replacement-template [`unescape`]. All operate on `&str` with **character**
//! offsets; the host owns the buffer and applies the returned text.
//!
//! # Character vs byte offsets
//!
//! The `regex` crate reports byte offsets; the editor addresses text by
//! character. Every public function here takes and returns *character*
//! offsets and converts internally ([`matches()`] in one pass, the others via
//! prefix counting), so callers never see byte positions and multi-byte text
//! cannot split a character.
//!
//! # Replacement templates
//!
//! In regex mode a replacement is a `regex` template: `$1` / `${name}` expand
//! to capture groups, and [`unescape`] first turns `\n`, `\t`, `\r`, `\\`
//! into the real characters. In literal mode the replacement is inserted
//! verbatim (no `$` expansion, no escapes).
//!
//! Spec: `crates/vix-find-panel/spec/index.md`.

use regex::Regex;

/// Byte offset of character index `ch` in `text` (or `text.len()` if past the end).
/// O(n) in `ch`; callers needing many conversions build a table instead (see
/// [`matches()`]).
fn char_to_byte(text: &str, ch: usize) -> usize {
    text.char_indices().nth(ch).map_or(text.len(), |(b, _)| b)
}

/// All matches of `re` in `text`, as `(char_start, char_end)` ranges in
/// ascending order. Byte→char conversion is done with one pass over the text.
///
/// Zero-width matches (e.g. `^`) yield empty ranges.
///
/// # Examples
///
/// ```
/// let re = regex::Regex::new("é").unwrap();
/// // Character offsets, not bytes: `é` is two bytes in UTF-8.
/// assert_eq!(vix_find_panel::matches("aéé", &re), [(1, 2), (2, 3)]);
/// ```
#[must_use]
pub fn matches(text: &str, re: &Regex) -> Vec<(usize, usize)> {
    // Byte offset where each character starts; `partition_point` maps byte→char.
    let starts: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
    let to_char = |byte: usize| starts.partition_point(|&b| b < byte);
    re.find_iter(text)
        .map(|m| (to_char(m.start()), to_char(m.end())))
        .collect()
}

/// The first match of `re` at or after character offset `from_char`, as a
/// `(char_start, char_end)` range, or `None` if there is none.
///
/// "At or after" compares the match *start*: a match straddling `from_char` is
/// skipped. No wrap-around; the host retries from 0 for wrapping search.
///
/// # Examples
///
/// ```
/// let re = regex::Regex::new("ab").unwrap();
/// assert_eq!(vix_find_panel::next_match("ab ab", &re, 1), Some((3, 5)));
/// assert_eq!(vix_find_panel::next_match("ab ab", &re, 4), None);
/// ```
#[must_use]
pub fn next_match(text: &str, re: &Regex, from_char: usize) -> Option<(usize, usize)> {
    let from_byte = char_to_byte(text, from_char);
    let m = re.find_iter(text).find(|m| m.start() >= from_byte)?;
    Some((
        text[..m.start()].chars().count(),
        text[..m.end()].chars().count(),
    ))
}

/// Replace every match of `re` in `text`. In `regex_mode` the `replacement` is
/// [`unescape`]d and `$group` references expand; otherwise it is inserted
/// literally. Returns the new text and the number of replacements.
///
/// # Examples
///
/// ```
/// let re = regex::Regex::new("a").unwrap();
/// let (out, n) = vix_find_panel::replace_all("banana", &re, false, "$1");
/// assert_eq!((out.as_str(), n), ("b$1n$1n$1", 3)); // literal: no expansion
/// ```
#[must_use]
pub fn replace_all(text: &str, re: &Regex, regex_mode: bool, replacement: &str) -> (String, usize) {
    // Counted separately because `Regex::replace_all` does not report how many
    // substitutions it made.
    let count = re.find_iter(text).count();
    let new = if regex_mode {
        let rep = unescape(replacement);
        re.replace_all(text, rep.as_str()).into_owned()
    } else {
        re.replace_all(text, regex::NoExpand(replacement))
            .into_owned()
    };
    (new, count)
}

/// Replace the single match of `re` whose start is exactly character offset
/// `at_char`. `template` is assumed already [`unescape`]d; in `regex_mode`
/// `$group` references expand, otherwise it is inserted literally. Returns the
/// new text and the char offset just past the inserted text (where searching
/// should resume), or `None` if no match starts at `at_char`.
///
/// The resume offset is measured in the *new* text and lands after the
/// replacement, so replacing with text that itself contains the pattern does
/// not loop forever.
///
/// # Examples
///
/// ```
/// let re = regex::Regex::new("o").unwrap();
/// let (out, resume) = vix_find_panel::replace_one("foo", &re, false, "00", 1).unwrap();
/// assert_eq!((out.as_str(), resume), ("f00o", 3));
/// assert!(vix_find_panel::replace_one("foo", &re, false, "x", 0).is_none());
/// ```
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
            // `caps.expand` is what makes `$1` work; it needs the captures, which
            // is why this walks `captures_iter` rather than `find_iter`.
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
        // Matches are ordered by start, so once past `at_byte` none can match.
        if m.start() > at_byte {
            break;
        }
    }
    None
}

/// Interpret `\n`, `\t`, `\r`, `\\` escapes in a regex replacement template,
/// leaving `$group` references intact for the regex engine to expand.
///
/// An unrecognised escape (`\q`) or a trailing lone backslash keeps the
/// backslash as-is.
///
/// # Examples
///
/// ```
/// assert_eq!(vix_find_panel::unescape(r"a\tb\n$1\q"), "a\tb\n$1\\q");
/// ```
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

/// Where a search looks. The find box carries this so one dialog covers what
/// used to be three menu items: widening the scope hands the query, the
/// replacement, and the toggles to the surface that can show that many results,
/// instead of making the user close the box and start again somewhere else.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Scope {
    /// The active buffer, matched in place (the find box's own hit stepping).
    #[default]
    Buffer,
    /// Every file in the project, listed in the workspace search panel.
    Files,
    /// Every file in the project, listed as `path:line:col` in the bottom dock.
    Workspace,
}

impl Scope {
    /// The next scope in the cycle: buffer → files → workspace → buffer.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Scope::Buffer => Scope::Files,
            Scope::Files => Scope::Workspace,
            Scope::Workspace => Scope::Buffer,
        }
    }

    /// The i18n key naming this scope, for the box's scope indicator.
    #[must_use]
    pub fn label_key(self) -> &'static str {
        match self {
            Scope::Buffer => "ui.find_scope_buffer",
            Scope::Files => "ui.find_scope_files",
            Scope::Workspace => "ui.find_scope_workspace",
        }
    }
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
///
/// # Examples
///
/// ```
/// let f = vix_find_panel::PathFilter::new(r"\.rs$", "target/");
/// assert!(f.allows("src/main.rs"));
/// assert!(!f.allows("target/debug/x.rs"));
/// assert!(!f.allows("README.md"));
/// ```
pub struct PathFilter {
    /// Paths must match this to pass; `None` when unset or invalid.
    include: Option<Regex>,
    /// Paths matching this are rejected; `None` when unset or invalid.
    exclude: Option<Regex>,
}

impl PathFilter {
    /// Compile a filter from `include` / `exclude` patterns. An empty string
    /// disables that side. Invalid patterns are ignored (treated as empty), so a
    /// half-typed regex never hides every file.
    #[must_use]
    pub fn new(include: &str, exclude: &str) -> Self {
        // Regex::new(..).ok() swallows the error on purpose (see doc above).
        let compile = |p: &str| (!p.is_empty()).then(|| Regex::new(p).ok()).flatten();
        PathFilter {
            include: compile(include),
            exclude: compile(exclude),
        }
    }

    /// Whether `path` passes the filter: it must match the include regex (when
    /// set) and must not match the exclude regex (when set).
    #[must_use]
    pub fn allows(&self, path: &str) -> bool {
        if let Some(inc) = &self.include
            && !inc.is_match(path)
        {
            return false;
        }
        if let Some(exc) = &self.exclude
            && exc.is_match(path)
        {
            return false;
        }
        true
    }
}

bitflags::bitflags! {
    /// [`SearchBar`]'s independent search toggles (T149): each bit is a
    /// distinct, freely-combinable preference, not one of several
    /// mutually-exclusive modes — grouping them into a plain sub-struct would
    /// only relocate `clippy::struct_excessive_bools`, since the lint counts
    /// `bool` fields anywhere, not just on `SearchBar` itself.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Flags: u8 {
        /// Replace mode shows and uses the replacement field.
        const REPLACING = 1 << 0;
        /// Interactive (query-replace) mode: Enter begins step-through y/n/!/q.
        const INTERACTIVE = 1 << 1;
        /// Match case exactly.
        const CASE_SENSITIVE = 1 << 2;
        /// Smart case: when `CASE_SENSITIVE` is off, match case-insensitively
        /// only if the query has no uppercase letter; an uppercase letter
        /// makes it sensitive.
        const SMART_CASE = 1 << 3;
        /// Match whole words only.
        const WHOLE_WORD = 1 << 4;
        /// Treat the query as a regular expression.
        const REGEX = 1 << 5;
    }
}

/// State of the find / find-and-replace box.
pub struct SearchBar {
    /// Search-pattern text.
    pub query: String,
    /// Replacement text.
    pub replace: String,
    /// Which input field has focus (only meaningful while replacing).
    pub field: Field,
    /// Independent display/mode toggles (replace/interactive/case/smart
    /// case/whole word/regex).
    pub flags: Flags,
    /// Where the search looks: this buffer, all files, or the workspace dock.
    /// The host acts on a change by handing this box's state to that surface.
    pub scope: Scope,
    /// Last status, e.g. match count or "no matches".
    pub status: String,
}

impl SearchBar {
    /// A fresh box; `replacing` selects find-and-replace mode.
    #[must_use]
    pub fn new(replacing: bool) -> Self {
        let mut flags = Flags::SMART_CASE;
        flags.set(Flags::REPLACING, replacing);
        SearchBar {
            query: String::new(),
            replace: String::new(),
            field: Field::Query,
            flags,
            scope: Scope::Buffer,
            status: String::new(),
        }
    }

    /// Turn replace on or off without closing the box, so a find becomes a
    /// find-and-replace in place — every "Find…" dialog can replace, rather than
    /// replace being a separate command you reopen the box for.
    ///
    /// Enabling focuses the replacement field, since that is what the user just
    /// asked for; disabling returns focus to the query and leaves the
    /// replacement text alone, so toggling back is not lossy. Turning replace
    /// off also ends interactive (step-through) mode, which has no meaning
    /// without a replacement.
    pub fn toggle_replace(&mut self) {
        self.flags.toggle(Flags::REPLACING);
        if self.flags.contains(Flags::REPLACING) {
            self.field = Field::Replace;
        } else {
            self.field = Field::Query;
            self.flags.remove(Flags::INTERACTIVE);
        }
    }

    /// Move to the next search scope (buffer → files → workspace → buffer) and
    /// return it. The host reads the new scope and, when it is not
    /// [`Scope::Buffer`], opens the matching surface seeded with this box.
    pub fn cycle_scope(&mut self) -> Scope {
        self.scope = self.scope.next();
        self.scope
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
        if self.flags.contains(Flags::REPLACING) {
            self.field = match self.field {
                Field::Query => Field::Replace,
                _ => Field::Query,
            };
        }
    }

    /// Build the effective regex pattern from the query and the toggles.
    /// Returns `None` for an empty query.
    ///
    /// Literal queries are `regex::escape`d; regex queries are used as typed
    /// (and may fail to compile while half-typed -- compiling is the host's
    /// job). Whole-word wraps the result in `\b..\b`, and case-insensitivity is
    /// applied last as a `(?i)` prefix.
    ///
    /// # Examples
    ///
    /// ```
    /// use vix_find_panel::{Flags, SearchBar};
    ///
    /// let mut bar = SearchBar::new(false);
    /// bar.query = "a.b".into();
    /// assert_eq!(bar.pattern().as_deref(), Some(r"(?i)a\.b")); // smart-case, no capitals
    /// bar.query = "A.b".into();
    /// bar.flags.insert(Flags::WHOLE_WORD);
    /// assert_eq!(bar.pattern().as_deref(), Some(r"\bA\.b\b")); // capital -> sensitive
    /// ```
    #[must_use]
    pub fn pattern(&self) -> Option<String> {
        if self.query.is_empty() {
            return None;
        }
        let mut core = if self.flags.contains(Flags::REGEX) {
            self.query.clone()
        } else {
            regex::escape(&self.query)
        };
        if self.flags.contains(Flags::WHOLE_WORD) {
            core = format!(r"\b{core}\b");
        }
        // Case-insensitive unless the case toggle is on, or smart-case is enabled
        // and the query contains an uppercase letter.
        let has_upper = self.query.chars().any(char::is_uppercase);
        let insensitive = !(self.flags.contains(Flags::CASE_SENSITIVE)
            || self.flags.contains(Flags::SMART_CASE) && has_upper);
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
        assert!(s.flags.contains(Flags::REPLACING));
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
        s.flags
            .insert(Flags::REGEX | Flags::CASE_SENSITIVE | Flags::WHOLE_WORD);
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
        assert_eq!(
            s.pattern().as_deref(),
            Some("(?i)foo"),
            "all-lowercase → insensitive"
        );
        s.query = "Foo".to_string();
        assert_eq!(
            s.pattern().as_deref(),
            Some("Foo"),
            "uppercase → case-sensitive"
        );
        // Turning smart-case off reverts to always-insensitive (unless case toggle).
        s.flags.remove(Flags::SMART_CASE);
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

        // Both: Rust files not under tui/tests/.
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
