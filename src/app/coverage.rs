//! Tools → Load Coverage File… / Toggle Coverage Gutter (T210): parse an
//! LCOV or Cobertura XML coverage report (`vix-coverage`) and paint
//! covered/uncovered/partial bars in the editor's gutter-sign column, reusing
//! the git diff gutter's `(line, Color)` mark mechanism
//! (`Editor::set_gutter_marks`). The two share that one column, so only one
//! shows at a time: while the coverage gutter is visible, `refresh_git_gutter`
//! is skipped for the active tab (see `src/ui.rs`'s call site).

use super::{App, Prompt, PromptKind};

impl App {
    /// Open the **Tools → Load Coverage File…** prompt, pre-filled from the
    /// `coverage_path` setting when one is configured.
    pub(super) fn open_load_coverage_prompt(&mut self) {
        let prefill = self.settings.coverage_path.clone();
        self.prompt = Some(
            Prompt::new(
                PromptKind::LoadCoverageFile,
                t!("prompt.load_coverage_file").to_string(),
            )
            .with_input(prefill),
        );
    }

    /// `PromptKind::LoadCoverageFile`'s accept handler: read and parse
    /// `input` (resolved relative to the workspace root), then show the
    /// coverage gutter for the active tab. Empty input is a no-op.
    pub(super) fn load_coverage_file(&mut self, input: &str) {
        if input.is_empty() {
            return;
        }
        let path = self.resolve(input);
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let report = vix_coverage::parse(&text);
                let files = report.file_count();
                self.coverage = Some(report);
                self.coverage_visible = true;
                self.refresh_coverage_gutter();
                self.status = t!("status.coverage_loaded", files = files).to_string();
            }
            Err(e) => {
                self.messages
                    .error(t!("msg.coverage_load_failed", error = e).to_string());
            }
        }
    }

    /// **Tools → Toggle Coverage Gutter**: show/hide the gutter for the
    /// already-loaded report without re-parsing it. A no-op status message
    /// when nothing has been loaded yet.
    pub(super) fn toggle_coverage_gutter(&mut self) {
        if self.coverage.is_none() {
            self.status = t!("status.coverage_not_loaded").to_string();
            return;
        }
        self.coverage_visible = !self.coverage_visible;
        if self.coverage_visible {
            self.refresh_coverage_gutter();
            self.status = t!("status.coverage_shown").to_string();
        } else {
            if let Some(t) = self.editor.active_tab_mut() {
                t.editor.clear_gutter_marks();
            }
            self.status = t!("status.coverage_hidden").to_string();
        }
    }

    /// Whether a coverage report is currently loaded (regardless of whether
    /// the gutter is shown or has been toggled off).
    #[must_use]
    pub fn has_coverage(&self) -> bool {
        self.coverage.is_some()
    }

    /// Whether the coverage gutter should be drawn right now: a report is
    /// loaded and it hasn't been toggled off. `src/ui.rs`'s per-frame refresh
    /// checks this to decide between the coverage gutter and the git diff
    /// gutter -- they share one gutter-sign column, so only one shows.
    #[must_use]
    pub fn coverage_gutter_active(&self) -> bool {
        self.coverage_visible && self.coverage.is_some()
    }

    /// Recompute the coverage gutter for the active tab from the loaded
    /// report. No-op when the gutter isn't visible (`src/ui.rs`'s call site
    /// only calls this when it is) or the active tab's path isn't in the
    /// report.
    pub fn refresh_coverage_gutter(&mut self) {
        if !self.coverage_visible {
            return;
        }
        let Some(report) = &self.coverage else {
            return;
        };
        let marks: Vec<(usize, &str)> = self
            .editor
            .active_tab()
            .filter(|t| !t.is_image())
            .and_then(|t| t.path.as_deref())
            .and_then(|p| report.lines_for(p))
            .map(|lines| {
                lines
                    .iter()
                    .filter_map(|(&line, &hit)| line.checked_sub(1).map(|i| (i, coverage_hex(hit))))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_gutter_marks(marks);
        }
    }
}

/// Hex color for a coverage-gutter line hit (green covered, red uncovered,
/// yellow partial) -- the same hex values the git diff gutter uses for
/// added/deleted/modified, so the two gutters share one palette.
fn coverage_hex(hit: vix_coverage::Hit) -> &'static str {
    match hit {
        vix_coverage::Hit::Covered => "#3fb950",
        vix_coverage::Hit::Uncovered => "#f85149",
        vix_coverage::Hit::Partial => "#d29922",
    }
}
