//! Multi-cursor commands the App owns (T622): **Edit → Select → Split
//! Selection into Lines** (`edit.split_selection_into_lines`, Helix/Kakoune
//! `Alt-s`) and **Select Regex Matches in Selection**
//! (`edit.select_regex_in_selection`, Kakoune/Helix `s`), which prompts for
//! the pattern. The caret logic lives in `vix_editor_core::multicursor`; this
//! only wires the prompt, the regex and the status line.

use super::{App, Prompt, PromptKind};

impl App {
    /// Dispatch the App-level multi-cursor actions. Returns whether `action`
    /// was one of them.
    pub(super) fn run_multi_cursor_action(&mut self, action: &str) -> bool {
        match action {
            "edit.split_selection_into_lines" => {
                let split = self
                    .editor
                    .active_tab_mut()
                    .is_some_and(|t| t.editor.split_selection_into_lines());
                self.status = if split {
                    self.carets_status()
                } else {
                    t!("status.split_selection_none").to_string()
                };
            }
            "edit.select_regex_in_selection" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::SelectRegexInSelection,
                    t!("prompt.select_regex_in_selection").to_string(),
                ));
            }
            _ => return false,
        }
        true
    }

    /// Dispatch the multi-cursor ids of the named action catalog
    /// (`crates/vix-editor-core/spec/actions.tsv`). Returns whether `id` was
    /// one of them.
    pub(super) fn run_multi_cursor_catalog(&mut self, id: &str) -> bool {
        if id == "spawn_multi_cursor_select" {
            self.run_action("edit.split_selection_into_lines");
            return true;
        }
        let Some(t) = self.editor.active_tab_mut() else {
            return matches!(
                id,
                "spawn_multi_cursor"
                    | "spawn_multi_cursor_up"
                    | "spawn_multi_cursor_down"
                    | "skip_multi_cursor"
                    | "skip_multi_cursor_back"
                    | "remove_multi_cursor"
                    | "remove_all_multi_cursors"
            );
        };
        let e = &mut t.editor;
        match id {
            "spawn_multi_cursor" => e.add_next_occurrence(),
            "spawn_multi_cursor_up" => e.add_caret_above(),
            "spawn_multi_cursor_down" => e.add_caret_below(),
            "skip_multi_cursor" => e.skip_occurrence(true),
            "skip_multi_cursor_back" => e.skip_occurrence(false),
            "remove_multi_cursor" => {
                e.remove_last_caret();
            }
            "remove_all_multi_cursors" => e.clear_carets(),
            _ => return false,
        }
        true
    }

    /// The submitted `edit.select_regex_in_selection` pattern: replace the
    /// carets with one selection per match inside the current selections (the
    /// whole buffer when nothing is selected). A bad pattern or no match
    /// leaves the carets as they were and says so.
    pub(super) fn select_regex_in_selection(&mut self, pattern: &str) {
        if pattern.is_empty() {
            return;
        }
        let re = match regex::Regex::new(pattern) {
            Ok(re) => re,
            Err(e) => {
                self.status = t!("msg.bad_regex", error = e).to_string();
                return;
            }
        };
        let n = self.editor.active_tab_mut().map_or(0, |t| {
            t.editor
                .select_matches_in_selection(|text| re.find_iter(text).map(|m| m.range()).collect())
        });
        self.status = if n == 0 {
            t!("status.no_matches").to_string()
        } else {
            self.carets_status()
        };
    }

    /// "N selections" for the active editor.
    fn carets_status(&self) -> String {
        let count = self
            .editor
            .active_tab()
            .map_or(1, |t| t.editor.caret_count());
        t!("status.carets_selected", count = count).to_string()
    }
}
