//! Search / search-and-replace toolbar state.
//!
//! The actual searching uses `tui-textarea`'s regex search; replacement is
//! applied by `App` against the active buffer.

/// Which input field of the search bar has focus.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// The search-pattern field.
    Query,
    /// The replacement field.
    Replace,
}

/// State of the find / find-and-replace toolbar.
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
    /// A fresh search bar; `replacing` selects find-and-replace mode.
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
