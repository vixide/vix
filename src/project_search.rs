//! Project-wide search (and replace) across every file under the project root.
//!
//! Open buffers are searched in their current (possibly unsaved) state; other
//! files are read from disk. `App` owns the buffers, so it drives the actual
//! scanning; this module just holds the panel state.

use std::path::PathBuf;

use crate::search::Field;

/// One matching line.
pub struct Hit {
    /// Absolute path of the file containing the match.
    pub path: PathBuf,
    /// 1-based line number of the match.
    pub line: usize,
    /// 1-based column of the match start.
    pub col: usize,
    /// `relpath:line: text` shown in the results list.
    pub display: String,
}

/// State of the project-wide search/replace panel.
pub struct ProjectSearch {
    /// Search-pattern text.
    pub query: String,
    /// Replacement text.
    pub replace: String,
    /// Whether the replacement field is shown and used.
    pub replacing: bool,
    /// Which input field has focus.
    pub field: Field,
    /// Match case exactly.
    pub case_sensitive: bool,
    /// Treat the query as a regular expression.
    pub regex: bool,
    /// Current matches.
    pub hits: Vec<Hit>,
    /// Index of the highlighted hit.
    pub selected: usize,
    /// Status/summary line shown under the inputs.
    pub status: String,
    /// When set, the hit list is fixed (e.g. go-to-definition candidates) and
    /// typing does not re-run the search.
    pub static_results: bool,
}

impl ProjectSearch {
    /// A fresh panel; `replacing` selects search-and-replace mode.
    #[must_use]
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
            status: t!("status.project_search_prompt").to_string(),
            static_results: false,
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

    /// Move the selection up one hit.
    pub fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Move the selection down one hit.
    pub fn down(&mut self) {
        if self.selected + 1 < self.hits.len() {
            self.selected += 1;
        }
    }

    /// The highlighted hit, if any.
    #[must_use]
    pub fn selected_hit(&self) -> Option<&Hit> {
        self.hits.get(self.selected)
    }
}
