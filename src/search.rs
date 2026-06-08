//! Search / search-and-replace toolbar state.
//!
//! The actual searching uses `tui-textarea`'s regex search; replacement is
//! applied by `App` against the active buffer.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Query,
    Replace,
}

pub struct SearchBar {
    pub query: String,
    pub replace: String,
    /// Replace mode shows and uses the replacement field.
    pub replacing: bool,
    /// Interactive (query-replace) mode: Enter begins step-through y/n/!/q.
    pub interactive: bool,
    /// Which input field has focus (only meaningful while replacing).
    pub field: Field,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
    /// Last status, e.g. match count or "no matches".
    pub status: String,
}

impl SearchBar {
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

    pub fn active_field_mut(&mut self) -> &mut String {
        match self.field {
            Field::Query => &mut self.query,
            Field::Replace => &mut self.replace,
        }
    }

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
