//! Workspace-wide search (and replace) across every file under the workspace root.
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

/// State of the workspace-wide search/replace panel.
pub struct WorkspaceSearch {
    /// Search-pattern text.
    pub query: String,
    /// Replacement text.
    pub replace: String,
    /// Regex limiting the search to file paths that match it (empty = no limit).
    pub include_path: String,
    /// Regex excluding file paths that match it (empty = no exclusion).
    pub exclude_path: String,
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

impl WorkspaceSearch {
    /// A fresh panel; `replacing` selects search-and-replace mode.
    #[must_use]
    pub fn new(replacing: bool) -> Self {
        WorkspaceSearch {
            query: String::new(),
            replace: String::new(),
            include_path: String::new(),
            exclude_path: String::new(),
            replacing,
            field: Field::Query,
            case_sensitive: false,
            regex: false,
            hits: Vec::new(),
            selected: 0,
            status: t!("status.workspace_search_prompt").to_string(),
            static_results: false,
        }
    }

    /// Mutable access to the currently focused field's text.
    pub fn active_field_mut(&mut self) -> &mut String {
        match self.field {
            Field::Query => &mut self.query,
            Field::Replace => &mut self.replace,
            Field::IncludePath => &mut self.include_path,
            Field::ExcludePath => &mut self.exclude_path,
        }
    }

    /// Cycle focus across the visible fields: query → (replace, if replacing) →
    /// include-path → exclude-path → query.
    pub fn toggle_field(&mut self) {
        self.field = match self.field {
            Field::Query if self.replacing => Field::Replace,
            Field::Query | Field::Replace => Field::IncludePath,
            Field::IncludePath => Field::ExcludePath,
            Field::ExcludePath => Field::Query,
        };
    }

    /// The compiled path filter from the include/exclude regexes.
    #[must_use]
    pub fn path_filter(&self) -> crate::find_panel::PathFilter {
        crate::find_panel::PathFilter::new(&self.include_path, &self.exclude_path)
    }

    /// Effective regex pattern from the query and toggles (no whole-word here).
    #[must_use] 
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
