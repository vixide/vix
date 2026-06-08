//! Project-wide search (and replace) across every file under the project root.
//!
//! Open buffers are searched in their current (possibly unsaved) state; other
//! files are read from disk. `App` owns the buffers, so it drives the actual
//! scanning; this module just holds the panel state.

use std::path::PathBuf;

use crate::search::Field;

/// One matching line.
pub struct Hit {
    pub path: PathBuf,
    /// 1-based line number of the match.
    pub line: usize,
    /// 1-based column of the match start.
    pub col: usize,
    /// `relpath:line: text` shown in the results list.
    pub display: String,
}

pub struct ProjectSearch {
    pub query: String,
    pub replace: String,
    /// Whether the replacement field is shown and used.
    pub replacing: bool,
    pub field: Field,
    pub case_sensitive: bool,
    pub regex: bool,
    pub hits: Vec<Hit>,
    pub selected: usize,
    pub status: String,
    /// When set, the hit list is fixed (e.g. go-to-definition candidates) and
    /// typing does not re-run the search.
    pub static_results: bool,
}

impl ProjectSearch {
    pub fn new(replacing: bool) -> Self {
        ProjectSearch {
            query: String::new(),
            replace: String::new(),
            replacing,
            field: Field::Query,
            case_sensitive: false,
            regex: false,
            hits: Vec::new(),
            selected: 0,
            status: "Type to search the project (2+ characters).".to_string(),
            static_results: false,
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

    /// Effective regex pattern from the query and toggles (no whole-word here).
    pub fn pattern(&self) -> Option<String> {
        if self.query.len() < 2 {
            return None;
        }
        let mut core = if self.regex {
            self.query.clone()
        } else {
            regex::escape(&self.query)
        };
        if !self.case_sensitive {
            core = format!("(?i){core}");
        }
        Some(core)
    }

    pub fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn down(&mut self) {
        if self.selected + 1 < self.hits.len() {
            self.selected += 1;
        }
    }

    pub fn selected_hit(&self) -> Option<&Hit> {
        self.hits.get(self.selected)
    }
}
