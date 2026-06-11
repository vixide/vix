//! State for the find / find-and-replace box.
//!
//! This crate owns the box's *state* — the query and replacement text, which
//! field has focus, the case / whole-word / regex toggles, and the builder that
//! turns the query plus toggles into an effective regex pattern. The host (the
//! `vix` app) renders the box and runs the search and replacement against the
//! active buffer; only `regex::escape` (for literal queries) is needed here.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Which input field of the box has focus.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    /// The search-pattern field.
    Query,
    /// The replacement field.
    Replace,
}

/// State of the find / find-and-replace box.
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
            whole_word: false,
            regex: false,
            status: String::new(),
        }
    }

    /// Mutable access to the currently focused field's text.
    pub fn active_field_mut(&mut self) -> &mut String {
        match self.field {
            Field::Query => &mut self.query,
            Field::Replace => &mut self.replace,
        }
    }

    /// Switch focus between the query and replace fields (replace mode only).
    pub fn toggle_field(&mut self) {
        if self.replacing {
            self.field = match self.field {
                Field::Query => Field::Replace,
                Field::Replace => Field::Query,
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
        if !self.case_sensitive {
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
}
