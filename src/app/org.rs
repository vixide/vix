//! Org core: the `org.*`/`org.edit.*` action dispatchers, headline/
//! subtree editing (priority, move, close-note, new heading, cut/copy/
//! paste subtree, export), refile, sparse trees, edit-src, footnotes,
//! internal links (store/insert/follow), archive, tags/properties,
//! timestamps/planning (SCHEDULED/DEADLINE), emphasis markup, capture
//! end-to-end (chooser, template fields, review, filing, clock), and the
//! built-in agenda (file scoping, view building, the agenda buffer's own
//! key handling).
//!
//! Moved out of `app.rs` verbatim (T141, slice 9 -- the largest single
//! slice of this epic, but unlike `git`/`scripts`/`lsp_dap` it turned out to
//! be one genuinely contiguous ~1600-line block, closer in shape to the
//! keymap-dispatch slice). Also carries `accept_roam_prompt`: despite
//! the name it's really the shared accept-handler for capture/roam/goto/
//! workspace prompts, and it dispatches into both this module's own
//! capture methods and `src/app/roam.rs`'s -- grouping it here (where
//! its sibling `accept_org_prompt`/`accept_org_agenda_prompt` already
//! live) avoided adding a needless cross-module dependency the other way.
//! A handful of generic editor transforms (`new_scratch_buffer`,
//! `rewrite_at_cursor`, `bump_number`, `transpose`, `delete_unit`,
//! `wrap_text`, `smart_toggle`) sit interleaved in the original range but
//! aren't org-specific, so they stayed in `app.rs`.

#![warn(clippy::pedantic)]

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, MouseEvent};

use super::{
    AgendaKind, AgendaView, App, CaptureChooser, Focus, PendingCapture, Prompt, PromptKind,
    RefileChooser, SrcEdit,
};

impl App {
    /// Dispatch an `org.*` action against the active buffer. Returns `true` if
    /// `action` was an Org command.
    pub(super) fn org_action(&mut self, action: &str) -> bool {
        match action {
            "org.cycle_visibility" => self.run_action("editor.fold_toggle"),
            "org.promote" => self.org_rewrite_line(crate::org::promote),
            "org.demote" => self.org_rewrite_line(crate::org::demote),
            "org.cycle_todo" => {
                self.org_rewrite_line(crate::org::cycle_todo);
                self.org_refresh_statistics();
            }
            "org.priority.up" => self.org_priority(true),
            "org.priority.down" => self.org_priority(false),
            "org.toggle_checkbox" => {
                self.org_rewrite_line(crate::org::toggle_checkbox);
                self.org_refresh_statistics();
            }
            "org.ctrl_c_ctrl_c" => self.org_ctrl_c_ctrl_c(),
            "org.close_note" => self.org_begin_close_note(),
            "org.update_statistics" => self.org_refresh_statistics(),
            "org.move_up" => self.org_move_subtree(crate::org::move_subtree_up),
            "org.move_down" => self.org_move_subtree(crate::org::move_subtree_down),
            "org.export_markdown" => self.org_export(crate::org::to_markdown, "md"),
            "org.export_html" => self.org_export(crate::org::to_html, "html"),
            "org.capture" => self.start_capture_by_key("a"),
            "org.capture.task" => self.start_capture_by_key("t"),
            "org.capture.babel" => self.start_capture_by_key("b"),
            "org.capture.note" => self.start_capture_by_key("n"),
            "org.capture.select" => self.open_capture_chooser(),
            "org.clock_in" => self.org_clock_in(),
            "org.clock_out" => self.org_clock_out(),
            "org.agenda" => self.org_agenda(),
            "org.agenda.todo" => self.open_view(&AgendaKind::Todo),
            "org.agenda.stuck" => self.open_view(&AgendaKind::Stuck),
            "org.agenda.match" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::OrgAgendaMatch,
                    t!("prompt.org_agenda_match").to_string(),
                ));
            }
            "org.agenda.search" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::OrgAgendaSearch,
                    t!("prompt.org_agenda_search").to_string(),
                ));
            }
            "org.time_report" => self.org_time_report(),
            _ => {
                return self.org_edit_action(action)
                    || self.roam_action(action)
                    || self.org_table_action(action);
            }
        }
        true
    }

    /// Dispatch the Org structure / date / link / tag editing actions (the
    /// Emacs Org-menu parity set). Returns `true` if `action` was handled.
    /// Extracted to keep [`App::org_action`] within the line limit.
    fn org_edit_action(&mut self, action: &str) -> bool {
        match action {
            "org.new_heading" => self.org_new_heading(),
            "org.nav.up" => self.org_goto(crate::org::nav_parent),
            "org.nav.next" => self.org_goto(crate::org::nav_next),
            "org.nav.previous" => self.org_goto(crate::org::nav_prev),
            "org.nav.forward" => self.org_goto(crate::org::nav_forward_same),
            "org.nav.backward" => self.org_goto(crate::org::nav_backward_same),
            "org.subtree.copy" => self.org_subtree_clip(false),
            "org.subtree.cut" => self.org_subtree_clip(true),
            "org.subtree.paste" => self.org_paste_subtree(),
            "org.sort_children" => self.org_rewrite_line(crate::org::sort_children),
            "org.refile" => self.open_refile_chooser(),
            "org.sparse.todo" => self.org_sparse_todo(),
            "org.sparse.match" => {
                if self.editor.active_tab().is_some() {
                    self.prompt = Some(Prompt::new(
                        PromptKind::OrgSparseMatch,
                        t!("prompt.org_sparse_match").to_string(),
                    ));
                }
            }
            "org.footnote" => self.org_footnote(),
            "org.edit_src" => self.org_edit_src(),
            "org.column_view" => self.open_column_view(),
            "org.column_view_export" => self.org_export(crate::org::column_view, "org"),
            "org.columns.insert_dblock" => self.org_columns_insert_dblock_prompt(),
            "org.columns.update_dblock" => self.org_columns_update_dblock(),
            "org.columns.update_all_dblocks" => self.org_columns_update_all_dblocks(),
            "org.archive.subtree" => self.org_archive_subtree(),
            "org.archive.tag" => {
                self.org_rewrite_line(|t, l| crate::org::toggle_tag(t, l, "ARCHIVE"));
            }
            "org.set_tags" => self.org_set_tags_prompt(),
            "org.set_property" => {
                if self.editor.active_tab().is_some() {
                    self.prompt = Some(Prompt::new(
                        PromptKind::OrgSetProperty,
                        t!("prompt.org_set_property").to_string(),
                    ));
                }
            }
            "org.timestamp" => self.org_insert_timestamp(true),
            "org.timestamp_inactive" => self.org_insert_timestamp(false),
            "org.schedule" => self.org_plan_prompt(PromptKind::OrgSchedule),
            "org.deadline" => self.org_plan_prompt(PromptKind::OrgDeadline),
            "org.date_up" => self.org_shift_date(1),
            "org.date_down" => self.org_shift_date(-1),
            "org.agenda.lock" => self.org_agenda_lock(),
            "org.agenda.unlock" => self.org_agenda_unlock(),
            "org.agenda.file_add" => self.org_agenda_file_add(),
            "org.agenda.file_remove" => self.org_agenda_file_remove(),
            "org.agenda.file_clear" => self.org_agenda_file_clear(),
            "org.agenda.file_list" => self.org_agenda_file_show(),
            "org.link.store" => self.org_store_link(),
            "org.link.insert" => self.org_insert_link_prompt(),
            "org.link.follow" => self.org_follow_link(),
            "org.link.next" => self.org_goto_link(true),
            "org.link.prev" => self.org_goto_link(false),
            "org.export_latex" => self.org_export(crate::org::to_latex, "tex"),
            "org.export_ics" => self.org_export_ics(),
            a if a.starts_with("org.emphasis.") => return self.org_emphasis(a),
            a if a.starts_with("org.block.") => return self.org_insert_block(a),
            _ => return false,
        }
        true
    }

    /// Run an Org transform that rewrites the buffer based on the cursor line
    /// (promote/demote/cycle-todo/toggle-checkbox), keeping the cursor's line.
    fn org_rewrite_line(&mut self, f: fn(&str, usize) -> Option<String>) {
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let line = tab.editor.cursor_line();
        let text = tab.editor.get_content();
        if let Some(new) = f(&text, line) {
            tab.editor.set_content(&new);
            tab.editor.set_cursor_line(line);
            tab.dirty = true;
        } else {
            self.status = t!("status.org_not_headline").to_string();
        }
    }

    /// Move the headline at the cursor's `[#X]` priority cookie one step
    /// toward `highest` (`up`) or `lowest`, using the
    /// `org_priority_highest`/`_lowest`/`_default` settings
    /// (`org.priority.up` / `org.priority.down`).
    fn org_priority(&mut self, up: bool) {
        let highest = self.settings.org_priority_highest;
        let lowest = self.settings.org_priority_lowest;
        let default = self.settings.org_priority_default;
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let line = tab.editor.cursor_line();
        let text = tab.editor.get_content();
        let result = if up {
            crate::org::priority_up(&text, line, highest, lowest, default)
        } else {
            crate::org::priority_down(&text, line, highest, lowest, default)
        };
        if let Some(new) = result {
            tab.editor.set_content(&new);
            tab.editor.set_cursor_line(line);
            tab.dirty = true;
        } else {
            self.status = t!("status.org_not_headline").to_string();
        }
    }

    /// Recompute every checkbox parent state and statistics cookie in the active
    /// buffer, keeping the cursor line. Runs after a checkbox toggle or TODO cycle
    /// (so children update their parents and cookies) and from Org → Update
    /// Statistics. No-op when nothing changes.
    fn org_refresh_statistics(&mut self) {
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let line = tab.editor.cursor_line();
        let text = tab.editor.get_content();
        let new = crate::org::update_statistics(&text);
        if new != text {
            tab.editor.set_content(&new);
            tab.editor.set_cursor_line(line);
            tab.dirty = true;
        }
    }

    /// Org `C-c C-c`: the context action on the cursor line. On a pipe table
    /// (or its `#+TBLFM:` line), realign it and apply any formulas
    /// ([`Self::org_table_recalc`]) — a table row is never simultaneously a
    /// checkbox line, so this never shadows the checkbox case below. On a
    /// list item with a checkbox, toggle it; otherwise recompute the buffer's
    /// statistics cookies and checkbox parents (matching how Org's `C-c C-c`
    /// "does the right thing" for the common editing cases).
    fn org_ctrl_c_ctrl_c(&mut self) {
        if self.org_table_recalc() {
            return;
        }
        let on_checkbox = self.editor.active_tab().is_some_and(|t| {
            let text = t.editor.get_content();
            let line = t.editor.cursor_line();
            text.split('\n')
                .nth(line)
                .is_some_and(crate::org::has_checkbox)
        });
        if on_checkbox {
            self.org_rewrite_line(crate::org::toggle_checkbox);
        }
        self.org_refresh_statistics();
    }

    /// Org `C-u C-c C-t`: begin closing the headline at the cursor with a note.
    /// Opens a note prompt (submitted with Enter, Alt+Enter inserts a newline);
    /// on submit, [`Self::org_close_note`] marks it DONE with a `CLOSED:` stamp
    /// and a `:LOGBOOK:` entry. A no-op with a status note off a headline.
    fn org_begin_close_note(&mut self) {
        let on_headline = self.editor.active_tab().is_some_and(|t| {
            let text = t.editor.get_content();
            let line = t.editor.cursor_line();
            text.split('\n')
                .nth(line)
                .and_then(crate::org::headline_level)
                .is_some()
        });
        if !on_headline {
            self.status = t!("status.org_not_headline").to_string();
            return;
        }
        self.prompt = Some(Prompt::new(
            PromptKind::OrgCloseNote,
            t!("prompt.org_close_note").to_string(),
        ));
    }

    /// Complete an Org close-with-note: mark the headline at the cursor DONE,
    /// stamp `CLOSED: [now]`, and log `note` into its `:LOGBOOK:` drawer.
    pub(super) fn org_close_note(&mut self, note: &str) {
        let now = Self::org_timestamp();
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let line = tab.editor.cursor_line();
        let text = tab.editor.get_content();
        if let Some(new) = crate::org::close_headline(&text, line, &now, note) {
            tab.editor.set_content(&new);
            tab.editor.set_cursor_line(line);
            tab.dirty = true;
            self.org_refresh_statistics();
            self.status = t!("status.org_closed").to_string();
        } else {
            self.status = t!("status.org_not_headline").to_string();
        }
    }

    /// Run an Org subtree move, following the cursor to the subtree's new line.
    fn org_move_subtree(&mut self, f: fn(&str, usize) -> Option<(String, usize)>) {
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let line = tab.editor.cursor_line();
        let text = tab.editor.get_content();
        if let Some((new, new_line)) = f(&text, line) {
            tab.editor.set_content(&new);
            tab.editor.set_cursor_line(new_line);
            tab.dirty = true;
        } else {
            self.status = t!("status.org_no_sibling").to_string();
        }
    }

    /// Export the active buffer with `f` into a new untitled tab named with `ext`.
    fn org_export(&mut self, f: impl Fn(&str) -> String, ext: &str) {
        let Some(text) = self.editor.active_tab().map(crate::editor::Tab::text) else {
            return;
        };
        let converted = f(&text);
        self.editor.new_tab_with_content(&converted);
        self.status = t!("status.org_exported", ext = ext).to_string();
    }

    /// Export the active buffer's `SCHEDULED:`/`DEADLINE:` entries as an
    /// iCalendar document in a new tab (Org → Export → iCalendar).
    fn org_export_ics(&mut self) {
        let name = self.active_tab_name();
        let now = jiff::Timestamp::now()
            .strftime("%Y%m%dT%H%M%SZ")
            .to_string();
        self.org_export(move |t| crate::org::to_ics(t, &name, &now), "ics");
    }

    /// Insert a sibling headline below the cursor line (Org `M-RET`), leaving
    /// the cursor after its stars, ready for the title.
    fn org_new_heading(&mut self) {
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let line = tab.editor.cursor_line();
        let text = tab.editor.get_content();
        let (new, new_line) = crate::org::new_heading(&text, line);
        let offset = new
            .split('\n')
            .take(new_line)
            .map(|l| l.chars().count() + 1)
            .sum::<usize>()
            + new
                .split('\n')
                .nth(new_line)
                .map_or(0, |l| l.chars().count());
        tab.editor.set_content(&new);
        tab.editor.set_cursor(offset);
        tab.dirty = true;
    }

    /// Move the cursor to the headline chosen by an `org.nav.*` motion, with a
    /// status note when there is no heading that way.
    fn org_goto(&mut self, f: fn(&str, usize) -> Option<usize>) {
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let line = tab.editor.cursor_line();
        let text = tab.editor.get_content();
        if let Some(target) = f(&text, line) {
            tab.editor.set_cursor_line(target);
        } else {
            self.status = t!("status.org_no_heading").to_string();
        }
    }

    /// Copy — or with `cut`, remove — the subtree governing the cursor to the
    /// system clipboard (Org `C-c C-x M-w` / `C-c C-x C-w`).
    fn org_subtree_clip(&mut self, cut: bool) {
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let line = tab.editor.cursor_line();
        let text = tab.editor.get_content();
        let Some((start, end)) = crate::org::governing_subtree(&text, line) else {
            self.status = t!("status.org_not_headline").to_string();
            return;
        };
        let lines: Vec<&str> = text.split('\n').collect();
        let _ = vix_clipboard::set(&format!("{}\n", lines[start..end].join("\n")));
        if cut {
            let rest: Vec<&str> = lines
                .iter()
                .enumerate()
                .filter(|(i, _)| !(start..end).contains(i))
                .map(|(_, s)| *s)
                .collect();
            tab.editor.set_content(&rest.join("\n"));
            tab.editor
                .set_cursor_line(start.min(rest.len().saturating_sub(1)));
            tab.dirty = true;
            self.status = t!("status.org_subtree_cut").to_string();
        } else {
            self.status = t!("status.org_subtree_copied").to_string();
        }
    }

    /// Paste the clipboard as a sibling of the subtree governing the cursor
    /// (Org `C-c C-x C-y`), releveled to match.
    fn org_paste_subtree(&mut self) {
        let clip = vix_clipboard::get().unwrap_or_default();
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let line = tab.editor.cursor_line();
        let text = tab.editor.get_content();
        if let Some((new, new_line)) = crate::org::paste_subtree(&text, line, &clip) {
            tab.editor.set_content(&new);
            tab.editor.set_cursor_line(new_line);
            tab.dirty = true;
        } else {
            self.status = t!("status.org_no_subtree_clip").to_string();
        }
    }

    /// Open the refile-target chooser (Org `C-c C-w`): every headline outside
    /// the subtree being moved, indented by level.
    fn open_refile_chooser(&mut self) {
        let Some(tab) = self.editor.active_tab() else {
            return;
        };
        let line = tab.editor.cursor_line();
        let text = tab.editor.get_content();
        let Some((start, end)) = crate::org::governing_subtree(&text, line) else {
            self.status = t!("status.org_not_headline").to_string();
            return;
        };
        let targets: Vec<(usize, String)> = crate::org::headlines(&text)
            .into_iter()
            .filter(|(l, _, _)| !(start..end).contains(l))
            .map(|(l, level, title)| (l, format!("{}{title}", "  ".repeat(level - 1))))
            .collect();
        if targets.is_empty() {
            self.status = t!("status.org_no_refile_target").to_string();
            return;
        }
        self.refile_chooser = Some(RefileChooser {
            targets,
            selected: 0,
        });
    }

    pub(super) fn refile_chooser_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(c) = self.refile_chooser.as_mut() {
                    let n = c.targets.len();
                    c.selected = (c.selected + n - 1) % n;
                }
            }
            KeyCode::Down => {
                if let Some(c) = self.refile_chooser.as_mut() {
                    c.selected = (c.selected + 1) % c.targets.len();
                }
            }
            KeyCode::Enter => self.accept_refile_chooser(),
            KeyCode::Esc => {
                self.refile_chooser = None;
            }
            _ => {}
        }
    }

    pub(super) fn refile_chooser_mouse(&mut self, mouse: MouseEvent) {
        if let Some(idx) = self.chooser_row(mouse)
            && let Some(c) = self.refile_chooser.as_mut()
            && idx < c.targets.len()
        {
            c.selected = idx;
            self.accept_refile_chooser();
        }
    }

    /// Refile the subtree under the chooser's highlighted headline.
    fn accept_refile_chooser(&mut self) {
        let Some(c) = self.refile_chooser.take() else {
            return;
        };
        let Some(&(target, _)) = c.targets.get(c.selected) else {
            return;
        };
        self.org_refile_apply(target);
    }

    /// Move the subtree at the cursor to the end of the subtree at
    /// `target_line`, like Org `C-c C-w`.
    fn org_refile_apply(&mut self, target_line: usize) {
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let line = tab.editor.cursor_line();
        let text = tab.editor.get_content();
        if let Some((new, new_line)) = crate::org::refile(&text, line, target_line) {
            tab.editor.set_content(&new);
            tab.editor.set_cursor_line(new_line);
            tab.dirty = true;
            self.status = t!("status.org_refiled").to_string();
        } else {
            self.status = t!("status.org_no_refile_target").to_string();
        }
    }

    /// Fold the buffer into a sparse tree showing only TODO entries
    /// (Org `C-c / t`).
    fn org_sparse_todo(&mut self) {
        let Some(tab) = self.editor.active_tab() else {
            return;
        };
        let folds = crate::org::todo_tree_folds(&tab.editor.get_content());
        self.org_apply_sparse(&folds);
    }

    /// Fold the buffer into a sparse tree showing only subtrees containing
    /// `query` (Org `C-c /`'s occur view).
    fn org_sparse_match(&mut self, query: &str) {
        if query.is_empty() {
            return;
        }
        let Some(tab) = self.editor.active_tab() else {
            return;
        };
        let folds = crate::org::occur_folds(&tab.editor.get_content(), query);
        self.org_apply_sparse(&folds);
    }

    /// Apply sparse-tree `folds` (clearing existing folds first) and report
    /// how many subtrees were hidden. Show All (`editor.unfold_all`) clears.
    fn org_apply_sparse(&mut self, folds: &[(usize, usize)]) {
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        tab.editor.unfold_all();
        for &(start, end) in folds {
            tab.editor.toggle_manual_fold(start, end);
        }
        self.status = t!("status.org_sparse", count = folds.len()).to_string();
    }

    /// Org `C-c '`: with the cursor in a `#+begin_src` block, open its body in
    /// a dedicated tab; from that tab, the same action writes the (possibly
    /// edited) body back into the block and closes the tab. Switching to a
    /// file-backed tab first abandons the pending edit.
    fn org_edit_src(&mut self) {
        if self.src_edit.is_some() && self.editor.active_tab().is_some_and(|t| t.path.is_none()) {
            self.org_edit_src_finish();
            return;
        }
        self.src_edit = None; // any stale session is abandoned
        let Some(tab) = self.editor.active_tab() else {
            return;
        };
        let line = tab.editor.cursor_line();
        let text = tab.editor.get_content();
        let Some((begin, end, _lang)) = crate::org::src_block_at(&text, line) else {
            self.status = t!("status.org_no_src_block").to_string();
            return;
        };
        let body: Vec<&str> = text
            .split('\n')
            .skip(begin + 1)
            .take(end - begin - 1)
            .collect();
        self.src_edit = Some(SrcEdit {
            source: tab.path.clone(),
            source_index: self.editor.active,
            begin_line: begin,
        });
        self.editor
            .new_tab_with_content(&format!("{}\n", body.join("\n")));
        self.focus = Focus::Editor;
        self.status = t!("status.org_src_editing").to_string();
    }

    /// Write the dedicated-buffer body back into its source block, close the
    /// dedicated tab, and return to the source buffer.
    fn org_edit_src_finish(&mut self) {
        let Some(se) = self.src_edit.take() else {
            return;
        };
        let Some(body) = self.editor.active_tab().map(crate::editor::Tab::text) else {
            return;
        };
        let _ = self.editor.close_active();
        // Locate the source tab again: by path when saved, else by its index.
        let idx = se
            .source
            .as_ref()
            .and_then(|p| {
                self.editor
                    .tabs
                    .iter()
                    .position(|t| t.path.as_ref() == Some(p))
            })
            .or_else(|| (se.source_index < self.editor.tabs.len()).then_some(se.source_index));
        let Some(idx) = idx else {
            self.status = t!("status.org_src_gone").to_string();
            return;
        };
        self.editor.active = idx;
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let text = tab.editor.get_content();
        if let Some(new) = crate::org::replace_src_body(&text, se.begin_line, &body) {
            tab.editor.set_content(&new);
            tab.editor.set_cursor_line(se.begin_line);
            tab.dirty = true;
            self.status = t!("status.org_src_applied").to_string();
        } else {
            self.status = t!("status.org_src_gone").to_string();
        }
        self.focus = Focus::Editor;
    }

    /// Org's footnote action (`C-c C-x f`): jump reference ⇄ definition, or
    /// create the next numbered footnote at the cursor.
    fn org_footnote(&mut self) {
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let cursor = tab.editor.get_cursor();
        let text = tab.editor.get_content();
        let (new, pos) = crate::org::footnote(&text, cursor);
        if new != text {
            tab.editor.set_content(&new);
            tab.dirty = true;
        }
        tab.editor.set_cursor(pos);
    }

    /// Follow an `id:` link: open the project `.org` file whose `:ID:`
    /// property matches, at its headline.
    fn org_follow_id(&mut self, id: &str) {
        for path in self.file_index.clone() {
            if !path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("org"))
            {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Some(line) = crate::org::id_location(&content, id) {
                self.with_jump(|s| {
                    s.open_path(&path, false);
                    if let Some(tab) = s.editor.active_tab_mut() {
                        tab.editor.set_cursor_line(line);
                    }
                    s.focus = Focus::Editor;
                });
                return;
            }
        }
        self.status = t!("status.org_no_link_target").to_string();
    }

    /// Move the subtree at the cursor into the sibling `<file>_archive` file
    /// (Org's default archive location), stamped with `:ARCHIVE_TIME:`.
    fn org_archive_subtree(&mut self) {
        let Some(path) = self.editor.active_tab().and_then(|t| t.path.clone()) else {
            self.status = t!("status.org_archive_unsaved").to_string();
            return;
        };
        let now = Self::org_timestamp();
        let (line, text) = match self.editor.active_tab() {
            Some(t) => (t.editor.cursor_line(), t.editor.get_content()),
            None => return,
        };
        let Some((rest, block)) = crate::org::archive_subtree(&text, line, &now) else {
            self.status = t!("status.org_not_headline").to_string();
            return;
        };
        let mut archive = path.into_os_string();
        archive.push("_archive");
        let archive = PathBuf::from(archive);
        let mut existing = std::fs::read_to_string(&archive).unwrap_or_default();
        if !existing.is_empty() && !existing.ends_with('\n') {
            existing.push('\n');
        }
        existing.push_str(&block);
        existing.push('\n');
        if let Err(e) = std::fs::write(&archive, existing) {
            self.messages
                .error(t!("msg.save_failed", error = e).to_string());
            return;
        }
        if let Some(tab) = self.editor.active_tab_mut() {
            let target = line.min(rest.split('\n').count().saturating_sub(1));
            tab.editor.set_content(&rest);
            tab.editor.set_cursor_line(target);
            tab.dirty = true;
        }
        self.status = t!("status.org_archived", path = archive.display()).to_string();
    }

    /// Open the Set Tags… prompt seeded with the governing headline's tags.
    fn org_set_tags_prompt(&mut self) {
        let Some(tab) = self.editor.active_tab() else {
            return;
        };
        let seed = crate::org::get_tags(&tab.editor.get_content(), tab.editor.cursor_line())
            .unwrap_or_default();
        self.prompt = Some(
            Prompt::new(
                PromptKind::OrgSetTags,
                t!("prompt.org_set_tags").to_string(),
            )
            .with_input(seed),
        );
    }

    /// Apply the Set Tags… prompt: replace the governing headline's tags.
    pub(super) fn org_set_tags(&mut self, tags: &str) {
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let line = tab.editor.cursor_line();
        let text = tab.editor.get_content();
        if let Some(new) = crate::org::set_tags(&text, line, tags) {
            tab.editor.set_content(&new);
            tab.editor.set_cursor_line(line);
            tab.dirty = true;
        } else {
            self.status = t!("status.org_not_headline").to_string();
        }
    }

    /// Apply the Set Property… prompt (`NAME VALUE`) to the governing
    /// headline's `:PROPERTIES:` drawer.
    pub(super) fn org_set_property(&mut self, input: &str) {
        let (name, value) = input.split_once(char::is_whitespace).unwrap_or((input, ""));
        if name.is_empty() {
            return;
        }
        let (name, value) = (name.trim().to_string(), value.trim().to_string());
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let line = tab.editor.cursor_line();
        let text = tab.editor.get_content();
        if let Some(new) = crate::org::set_property(&text, line, &name, &value) {
            tab.editor.set_content(&new);
            tab.editor.set_cursor_line(line);
            tab.dirty = true;
        } else {
            self.status = t!("status.org_not_headline").to_string();
        }
    }

    /// Insert an active `<…>` or inactive `[…]` timestamp for today at the
    /// cursor (Org `C-c .` / `C-c !`).
    fn org_insert_timestamp(&mut self, active: bool) {
        let today = jiff::Zoned::now().strftime("%Y-%m-%d").to_string();
        if let Some(stamp) = crate::org::timestamp_for(&today, active) {
            self.insert_content(&stamp);
        }
    }

    /// Open the Schedule…/Deadline… date prompt seeded with today.
    fn org_plan_prompt(&mut self, kind: PromptKind) {
        if self.editor.active_tab().is_none() {
            return;
        }
        let key = if matches!(kind, PromptKind::OrgSchedule) {
            "prompt.org_schedule"
        } else {
            "prompt.org_deadline"
        };
        let today = jiff::Zoned::now().strftime("%Y-%m-%d").to_string();
        self.prompt = Some(Prompt::new(kind, t!(key).to_string()).with_input(today));
    }

    /// Apply a Schedule…/Deadline… prompt: set the governing headline's
    /// planning entry to the entered date.
    pub(super) fn org_plan(&mut self, keyword: &str, date: &str) {
        let Some(stamp) = crate::org::timestamp_for(date, true) else {
            self.status = t!("status.org_bad_date").to_string();
            return;
        };
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let line = tab.editor.cursor_line();
        let text = tab.editor.get_content();
        if let Some(new) = crate::org::plan(&text, line, keyword, &stamp) {
            tab.editor.set_content(&new);
            tab.editor.set_cursor_line(line);
            tab.dirty = true;
        } else {
            self.status = t!("status.org_not_headline").to_string();
        }
    }

    /// Shift the date under the cursor by `delta` days, rewriting its weekday
    /// (Org `S-↑`/`S-↓` on a timestamp).
    fn org_shift_date(&mut self, delta: i64) {
        self.rewrite_at_cursor(
            move |t, c| crate::org::shift_timestamp_at(t, c, delta),
            Some("status.org_no_timestamp"),
        );
    }

    /// The active file's workspace-relative path (the form stored in the
    /// agenda file list), or `None` with a status note for an unsaved buffer.
    fn active_rel_path(&mut self) -> Option<String> {
        let Some(path) = self.active_path() else {
            self.status = t!("status.org_archive_unsaved").to_string();
            return None;
        };
        Some(
            path.strip_prefix(&self.root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned(),
        )
    }

    /// Lock the agenda to the active file (Org `C-c C-x <`). Session-only;
    /// while locked, every agenda view scans just this file.
    fn org_agenda_lock(&mut self) {
        let Some(path) = self.active_path() else {
            self.status = t!("status.org_archive_unsaved").to_string();
            return;
        };
        let rel = path
            .strip_prefix(&self.root)
            .unwrap_or(&path)
            .display()
            .to_string();
        self.status = t!("status.org_agenda_locked", path = rel).to_string();
        self.agenda_restriction = Some(path);
    }

    /// Remove the agenda restriction lock (Org `C-c C-x >`).
    fn org_agenda_unlock(&mut self) {
        self.agenda_restriction = None;
        self.status = t!("status.org_agenda_unlocked").to_string();
    }

    /// Add the active file to the persisted agenda file list.
    fn org_agenda_file_add(&mut self) {
        let Some(rel) = self.active_rel_path() else {
            return;
        };
        if self.settings.org_agenda_files.contains(&rel) {
            self.status = t!("status.org_agenda_file_present", path = rel).to_string();
            return;
        }
        self.settings.org_agenda_files.push(rel.clone());
        let _ = self.store_settings();
        self.status = t!("status.org_agenda_file_added", path = rel).to_string();
    }

    /// Remove the active file from the persisted agenda file list.
    fn org_agenda_file_remove(&mut self) {
        let Some(rel) = self.active_rel_path() else {
            return;
        };
        let before = self.settings.org_agenda_files.len();
        self.settings.org_agenda_files.retain(|p| p != &rel);
        if self.settings.org_agenda_files.len() == before {
            self.status = t!("status.org_agenda_file_absent", path = rel).to_string();
        } else {
            let _ = self.store_settings();
            self.status = t!("status.org_agenda_file_removed", path = rel).to_string();
        }
    }

    /// Clear the agenda file list, restoring the every-project-file default.
    fn org_agenda_file_clear(&mut self) {
        self.settings.org_agenda_files.clear();
        let _ = self.store_settings();
        self.status = t!("status.org_agenda_files_cleared").to_string();
    }

    /// Show the agenda's current scope in the status bar: the restriction
    /// lock, the explicit file list, or the all-project-files default.
    fn org_agenda_file_show(&mut self) {
        if let Some(locked) = self.agenda_restriction.clone() {
            let rel = locked
                .strip_prefix(&self.root)
                .unwrap_or(&locked)
                .display()
                .to_string();
            self.status = t!("status.org_agenda_locked", path = rel).to_string();
        } else if self.settings.org_agenda_files.is_empty() {
            self.status = t!("status.org_agenda_files_all").to_string();
        } else {
            self.status = t!(
                "status.org_agenda_files_list",
                files = self.settings.org_agenda_files.join(", ")
            )
            .to_string();
        }
    }

    /// Store an Org link to the cursor's file and line (Org `C-c l`); it seeds
    /// the next Insert Link… prompt.
    fn org_store_link(&mut self) {
        let Some(path) = self.active_path() else {
            self.status = t!("status.org_archive_unsaved").to_string();
            return;
        };
        let line = self
            .editor
            .active_tab()
            .map_or(0, |t| t.editor.cursor_line());
        let rel = path
            .strip_prefix(&self.root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        let stored = format!("[[file:{rel}::{}][{rel}:{}]]", line + 1, line + 1);
        self.status = t!("status.org_link_stored", link = stored.clone()).to_string();
        self.stored_org_link = Some(stored);
    }

    /// Open the Insert Link… target prompt, seeded with the stored link.
    fn org_insert_link_prompt(&mut self) {
        if self.editor.active_tab().is_none() {
            return;
        }
        let seed = self.stored_org_link.clone().unwrap_or_default();
        self.prompt = Some(
            Prompt::new(
                PromptKind::OrgLinkTarget,
                t!("prompt.org_link_target").to_string(),
            )
            .with_input(seed),
        );
    }

    /// Insert the pending `[[target][description]]` link at the cursor (the
    /// second half of Insert Link…; empty description gives `[[target]]`).
    fn org_insert_link(&mut self, desc: &str) {
        let Some(target) = self.pending_link_target.take() else {
            return;
        };
        let link = if desc.is_empty() {
            format!("[[{target}]]")
        } else {
            format!("[[{target}][{desc}]]")
        };
        self.insert_content(&link);
    }

    /// Move the cursor to the next/previous Org link in the buffer.
    fn org_goto_link(&mut self, forward: bool) {
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let cursor = tab.editor.get_cursor();
        let text = tab.editor.get_content();
        if let Some(pos) = crate::org::link_pos(&text, cursor, forward) {
            tab.editor.set_cursor(pos);
        } else {
            self.status = t!("status.org_no_link").to_string();
        }
    }

    /// Follow the Org link under the cursor (Org `C-c C-o`): open `file:` links
    /// in the editor (honoring a `::line` suffix), copy web/mail URLs to the
    /// clipboard (a TUI has no browser), and jump to a matching headline for
    /// internal `*Headline` targets.
    fn org_follow_link(&mut self) {
        let Some(tab) = self.editor.active_tab() else {
            return;
        };
        let cursor = tab.editor.get_cursor();
        let text = tab.editor.get_content();
        let Some((target, _)) = crate::org::link_at(&text, cursor) else {
            self.status = t!("status.org_no_link").to_string();
            return;
        };
        if target.starts_with("http://")
            || target.starts_with("https://")
            || target.starts_with("mailto:")
        {
            let _ = vix_clipboard::set(&target);
            self.status = t!("status.org_link_copied", url = target).to_string();
        } else if let Some(id) = target.strip_prefix("id:") {
            let id = id.to_string();
            self.org_follow_id(&id);
        } else if let Some(rest) = target.strip_prefix("file:") {
            let (path, line_no) = rest
                .split_once("::")
                .map_or((rest, None), |(p, n)| (p, n.parse::<usize>().ok()));
            let path = self.resolve(path);
            self.with_jump(|s| {
                s.open_path(&path, false);
                if let Some(n) = line_no {
                    let area = s.editor_view();
                    s.editor.goto(n, None, area);
                }
                s.focus = Focus::Editor;
            });
        } else {
            let name = target.trim_start_matches('*').trim().to_lowercase();
            let hit = crate::org::headlines(&text)
                .into_iter()
                .find(|(_, _, title)| title.to_lowercase().contains(&name))
                .map(|(l, _, _)| l);
            if let Some(l) = hit {
                if let Some(tab) = self.editor.active_tab_mut() {
                    tab.editor.set_cursor_line(l);
                }
            } else {
                self.status = t!("status.org_no_link_target").to_string();
            }
        }
    }

    /// Wrap the selection in an Org emphasis marker pair for `org.emphasis.*`
    /// (Org `C-c C-x C-f`). Returns `true` if `action` matched.
    fn org_emphasis(&mut self, action: &str) -> bool {
        let marker = match action {
            "org.emphasis.bold" => "*",
            "org.emphasis.italic" => "/",
            "org.emphasis.underline" => "_",
            "org.emphasis.code" => "~",
            "org.emphasis.verbatim" => "=",
            "org.emphasis.strike" => "+",
            _ => return false,
        };
        self.toggle_wrap(marker, marker);
        true
    }

    /// Dispatch a submitted Org prompt (schedule / deadline / refile / link /
    /// tags / property). Extracted to keep [`App::accept_prompt`] within the
    /// line limit.
    pub(super) fn accept_org_prompt(&mut self, kind: PromptKind, input: &str) {
        match kind {
            PromptKind::OrgSchedule => self.org_plan("SCHEDULED", input),
            PromptKind::OrgDeadline => self.org_plan("DEADLINE", input),
            PromptKind::OrgSparseMatch => self.org_sparse_match(input),
            PromptKind::OrgLinkTarget => {
                if !input.is_empty() {
                    self.pending_link_target = Some(input.to_string());
                    self.prompt = Some(Prompt::new(
                        PromptKind::OrgLinkDesc,
                        t!("prompt.org_link_desc").to_string(),
                    ));
                }
            }
            PromptKind::OrgLinkDesc => self.org_insert_link(input),
            PromptKind::OrgSetTags => self.org_set_tags(input),
            PromptKind::OrgSetProperty => self.org_set_property(input),
            PromptKind::OrgTableSort => self.accept_org_table_sort(input),
            PromptKind::OrgColumnsInsertColumn => self.org_columns_insert_column(input),
            PromptKind::OrgColumnsInsertDblock => self.org_columns_insert_dblock(input),
            _ => {}
        }
    }

    /// Handle a completed agenda-restriction prompt (`raw` is the trimmed
    /// input). Grouped out of [`App::accept_prompt`] to keep it within the
    /// line limit.
    pub(super) fn accept_org_agenda_prompt(&mut self, kind: PromptKind, raw: &str) {
        match kind {
            PromptKind::OrgAgendaMatch => self.open_view(&AgendaKind::Match(raw.to_string())),
            PromptKind::OrgAgendaSearch => self.open_view(&AgendaKind::Search(raw.to_string())),
            _ => {}
        }
    }

    /// Insert an empty `#+begin_…`/`#+end_…` block at the cursor for
    /// `org.block.*` (Org `C-c C-,`). Returns `true` if `action` matched.
    fn org_insert_block(&mut self, action: &str) -> bool {
        let kind = match action {
            "org.block.src" => "src",
            "org.block.example" => "example",
            "org.block.quote" => "quote",
            "org.block.center" => "center",
            "org.block.verse" => "verse",
            "org.block.comment" => "comment",
            _ => return false,
        };
        self.insert_content(&format!("#+begin_{kind}\n\n#+end_{kind}\n"));
        true
    }

    /// Start capturing with the `org_capture_templates` entry whose `key`
    /// matches (the fixed **Capture → Anything…/Todo…/Contact…** menu items
    /// resolve to the built-in `"a"`/`"t"`/`"c"` templates this way). Reports
    /// a status message instead if no such template is configured.
    pub(super) fn start_capture_by_key(&mut self, key: &str) {
        let Some(template) = self
            .settings
            .org_capture_templates
            .iter()
            .find(|t| t.key == key)
            .cloned()
        else {
            self.status = t!("status.org_capture_missing_key", key = key).to_string();
            return;
        };
        self.start_capture(template);
    }

    /// Open the Org-capture chooser (**Capture → Choose Template…**): every
    /// configured template, selectable by arrow keys/click.
    fn open_capture_chooser(&mut self) {
        if self.settings.org_capture_templates.is_empty() {
            self.status = t!("status.org_capture_no_templates").to_string();
            return;
        }
        self.capture_chooser = Some(CaptureChooser { selected: 0 });
    }

    /// Begin capturing with `template`: queue its `%^{}` field prompts (and a
    /// tag prompt, if it uses `%^g`/`%^G`), then open the first one.
    fn start_capture(&mut self, template: vix_org_capture::CaptureTemplate) {
        let prompts = vix_org_capture::extract_prompts(&template.template);
        let wants_tags = vix_org_capture::wants_tags(&template.template);
        self.pending_capture = Some(PendingCapture {
            template,
            prompts,
            answers: Vec::new(),
            next: 0,
            wants_tags,
            tags: None,
        });
        self.advance_capture();
    }

    /// Open the next unanswered prompt in the active capture wizard, or
    /// finish the capture once every field (and the tag prompt, if any) has
    /// been answered. Each prompt carries a live preview of the template in
    /// progress (see [`App::capture_preview`]).
    fn advance_capture(&mut self) {
        let Some(pc) = self.pending_capture.as_ref() else {
            return;
        };
        let next_idx = pc.next;
        let field = pc.prompts.get(next_idx).cloned();
        let show_tags_prompt = field.is_none() && pc.wants_tags && pc.tags.is_none();
        if let Some(field) = field {
            let mut prompt = Prompt::new(PromptKind::OrgCaptureField, field.label.clone())
                .with_input(field.default.clone());
            if let Some(preview) = self.capture_preview(Some(next_idx), false) {
                prompt = prompt.with_preview(preview);
            }
            self.prompt = Some(prompt);
        } else if show_tags_prompt {
            let mut prompt = Prompt::new(
                PromptKind::OrgCaptureField,
                t!("prompt.org_capture_tags").to_string(),
            );
            if let Some(preview) = self.capture_preview(None, true) {
                prompt = prompt.with_preview(preview);
            }
            self.prompt = Some(prompt);
        } else {
            self.finish_capture();
        }
    }

    /// A live preview of the active capture's template: answered fields
    /// substituted, the field at `current` (if any) marked `‹Label›`
    /// (`tags_current` does the same for the trailing tag prompt), every
    /// other placeholder (`%t`, `%a`, …) expanded immediately from
    /// [`App::capture_context`]. `None` if no capture is in progress.
    fn capture_preview(&mut self, current: Option<usize>, tags_current: bool) -> Option<String> {
        let pc = self.pending_capture.as_ref()?;
        let template = pc.template.template.clone();
        let prompts = pc.prompts.clone();
        let answers = pc.answers.clone();
        let tags = pc.tags.clone();
        let ctx = self.capture_context();
        let tags = tags.as_deref().map(Self::format_capture_tags);
        Some(vix_org_capture::preview(
            &template,
            &prompts,
            &answers,
            current,
            tags.as_deref(),
            tags_current,
            &ctx,
        ))
    }

    /// Accept one answer in the active capture wizard: a `%^{}` field until
    /// they're exhausted, then (if the template wants tags) the tag prompt.
    fn accept_capture_field(&mut self, input: &str) {
        let Some(pc) = self.pending_capture.as_mut() else {
            return;
        };
        if pc.next < pc.prompts.len() {
            pc.answers.push(input.to_string());
            pc.next += 1;
        } else {
            pc.tags = Some(input.to_string());
        }
        self.advance_capture();
    }

    /// Expand and wrap the active capture's template. Files it immediately
    /// when the template sets `immediate_finish`; otherwise opens a final
    /// review buffer (multiline, Alt+Enter = newline) the user can edit
    /// before filing.
    fn finish_capture(&mut self) {
        let Some(pc) = self.pending_capture.take() else {
            return;
        };
        let ctx = self.capture_context();
        let tags = pc.tags.as_deref().map(Self::format_capture_tags);
        let expansion =
            vix_org_capture::expand(&pc.template.template, &pc.answers, tags.as_deref(), &ctx);
        let wrapped = vix_org_capture::wrap_entry(pc.template.entry_type, &expansion.text);
        if pc.template.immediate_finish {
            self.file_capture(&pc.template, &wrapped);
        } else {
            self.prompt = Some(
                Prompt::new(
                    PromptKind::OrgCaptureReview,
                    t!(
                        "prompt.org_capture_review",
                        description = pc.template.description.clone()
                    )
                    .to_string(),
                )
                .with_input(wrapped),
            );
            self.capture_review = Some(pc.template);
        }
    }

    /// Accept the (possibly hand-edited) review buffer and file it.
    fn accept_capture_review(&mut self, input: &str) {
        if let Some(template) = self.capture_review.take() {
            self.file_capture(&template, input);
        }
    }

    /// Turn raw whitespace-separated tag text (`"work urgent"`) into Org's
    /// trailing tag syntax (`":work:urgent:"`); empty input yields no tags.
    fn format_capture_tags(raw: &str) -> String {
        let tags: Vec<&str> = raw.split_whitespace().collect();
        if tags.is_empty() {
            String::new()
        } else {
            format!(":{}:", tags.join(":"))
        }
    }

    /// Build the placeholder-expansion [`vix_org_capture::Context`] from the
    /// current date/time, active buffer, selection, and clipboard.
    fn capture_context(&mut self) -> vix_org_capture::Context {
        let now = jiff::Zoned::now();
        let path = self.active_path();
        let line = self
            .editor
            .active_tab()
            .map_or(0, |t| t.editor.cursor_line());
        let annotation = path.as_ref().map_or_else(String::new, |p| {
            let rel = p
                .strip_prefix(&self.root)
                .unwrap_or(p)
                .to_string_lossy()
                .into_owned();
            format!("[[file:{rel}::{}][{rel}:{}]]", line + 1, line + 1)
        });
        let initial = self
            .editor
            .active_tab_mut()
            .and_then(|t| t.editor.get_selection_text())
            .unwrap_or_default();
        let file_name = path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let file_path = path
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let clipboard = vix_clipboard::get().unwrap_or_default();
        vix_org_capture::Context {
            year: i32::from(now.year()),
            month: u32::from(now.month().unsigned_abs()),
            day: u32::from(now.day().unsigned_abs()),
            hour: u32::from(now.hour().unsigned_abs()),
            minute: u32::from(now.minute().unsigned_abs()),
            weekday: now.strftime("%a").to_string(),
            annotation,
            initial,
            file_name,
            file_path,
            clipboard,
        }
    }

    /// File `wrapped` (already expanded and entry-wrapped) per `template`'s
    /// target, prepend/empty-lines/clock-in properties.
    fn file_capture(&mut self, template: &vix_org_capture::CaptureTemplate, wrapped: &str) {
        let wrapped = if template.clock_in {
            Self::insert_capture_clock(wrapped, &crate::org::clock_in(&Self::org_timestamp()))
        } else {
            wrapped.to_string()
        };
        let prepend = template.prepend;
        let empty_lines = template.empty_lines;
        let filed = match vix_org_capture::Target::parse(&template.target) {
            vix_org_capture::Target::Cursor => {
                self.insert_content(&format!("{wrapped}\n"));
                true
            }
            vix_org_capture::Target::File(rel) => {
                self.file_capture_write(&rel, |content| {
                    vix_org_capture::insert_top_level(content, &wrapped, prepend, empty_lines)
                });
                true
            }
            vix_org_capture::Target::FileHeadline(rel, headline) => {
                self.file_capture_write(&rel, |content| {
                    vix_org_capture::insert_under_headline(
                        content,
                        &headline,
                        &wrapped,
                        prepend,
                        empty_lines,
                    )
                });
                true
            }
            vix_org_capture::Target::FileDatetree(rel) => {
                let ctx = self.capture_context();
                self.file_capture_write(&rel, |content| {
                    vix_org_capture::insert_datetree(content, &ctx, &wrapped, prepend, empty_lines)
                });
                true
            }
            vix_org_capture::Target::Id(id) => {
                self.file_capture_by_id(&id, &wrapped, prepend, empty_lines)
            }
        };
        if filed {
            self.status = t!(
                "status.org_captured",
                description = template.description.clone()
            )
            .to_string();
        }
    }

    /// Read the project file at `rel` (relative to the workspace root, empty
    /// content if it doesn't exist yet), run `build` over its content, then
    /// write the result back and open it.
    fn file_capture_write(&mut self, rel: &str, build: impl FnOnce(&str) -> String) {
        let path = self.root.join(rel);
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let new = build(&content);
        self.roam_write_and_open(&path, &new);
    }

    /// File `wrapped` under the headline/node carrying `:ID: id`, searching
    /// every project `.org` file. Reports and returns `false` if no such id
    /// is found.
    fn file_capture_by_id(
        &mut self,
        id: &str,
        wrapped: &str,
        prepend: bool,
        empty_lines: u8,
    ) -> bool {
        for path in self.file_index.clone() {
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("org"))
                && let Ok(content) = std::fs::read_to_string(&path)
                && let Some(new) =
                    vix_org_capture::insert_under_id(&content, id, wrapped, prepend, empty_lines)
            {
                self.roam_write_and_open(&path, &new);
                return true;
            }
        }
        self.status = t!("status.org_capture_id_not_found", id = id).to_string();
        false
    }

    /// Splice a `CLOCK:` line right after `wrapped`'s first (headline) line.
    fn insert_capture_clock(wrapped: &str, clock_line: &str) -> String {
        match wrapped.split_once('\n') {
            Some((first, rest)) => format!("{first}\n  {clock_line}\n{rest}"),
            None => format!("{wrapped}\n  {clock_line}"),
        }
    }

    /// The current local time as an Org timestamp (`YYYY-MM-DD Day HH:MM`).
    fn org_timestamp() -> String {
        jiff::Zoned::now().strftime("%Y-%m-%d %a %H:%M").to_string()
    }

    /// Org clock-in: insert a `CLOCK: [now]` entry at the cursor.
    fn org_clock_in(&mut self) {
        let line = crate::org::clock_in(&Self::org_timestamp());
        self.insert_content(&format!("{line}\n"));
    }

    /// Org clock-out: close the most recent open `CLOCK:` entry in the buffer.
    fn org_clock_out(&mut self) {
        let now = Self::org_timestamp();
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let line = tab.editor.cursor_line();
        let text = tab.editor.get_content();
        if let Some(new) = crate::org::clock_out(&text, &now) {
            tab.editor.set_content(&new);
            tab.editor.set_cursor_line(line);
            tab.dirty = true;
            self.status = t!("status.org_clocked_out").to_string();
        } else {
            self.status = t!("status.org_no_clock").to_string();
        }
    }

    /// Gather every project `.org` file as `(absolute path, display name,
    /// contents)`, skipping any that fail to read. Shared by the agenda builder.
    pub(super) fn org_agenda_files(&self) -> Vec<(PathBuf, String, String)> {
        // Scope: the restriction lock wins, then the explicit agenda file
        // list, else every project `.org` file.
        let paths: Vec<PathBuf> = if let Some(locked) = &self.agenda_restriction {
            vec![locked.clone()]
        } else if self.settings.org_agenda_files.is_empty() {
            self.file_index
                .iter()
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("org")))
                .cloned()
                .collect()
        } else {
            self.settings
                .org_agenda_files
                .iter()
                .map(|p| self.resolve(p))
                .collect()
        };
        let mut out = Vec::new();
        for path in paths {
            if let Ok(content) = std::fs::read_to_string(&path) {
                let name = path
                    .strip_prefix(&self.root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();
                out.push((path, name, content));
            }
        }
        out
    }

    /// Build the buffer text and backing [`AgendaView`] for the built-in agenda
    /// `kind` from the project's `.org` files, plus the number of listed items.
    fn build_view(&self, kind: &AgendaKind) -> (String, AgendaView, usize) {
        let gathered = self.org_agenda_files();
        let files: Vec<(String, String)> = gathered
            .iter()
            .map(|(_, n, c)| (n.clone(), c.clone()))
            .collect();
        let (items, text, line_map) = match kind {
            AgendaKind::Weekly => {
                let items = crate::org::agenda_items(&files);
                let (text, map) = crate::org::render_agenda(&items);
                (items, text, map)
            }
            AgendaKind::Todo => {
                Self::list_view(&t!("agenda.todo.title"), crate::org::todo_list(&files))
            }
            AgendaKind::Stuck => Self::list_view(
                &t!("agenda.stuck.title"),
                crate::org::stuck_projects(&files),
            ),
            AgendaKind::Match(q) => Self::list_view(
                &t!("agenda.match.title", query = q),
                crate::org::tags_match(&files, q),
            ),
            AgendaKind::Search(q) => Self::list_view(
                &t!("agenda.search.title", query = q),
                crate::org::search(&files, q),
            ),
        };
        let count = items.len();
        let by_name: std::collections::HashMap<&str, &Path> = gathered
            .iter()
            .map(|(p, n, _)| (n.as_str(), p.as_path()))
            .collect();
        let item_locs: Vec<(PathBuf, usize)> = items
            .iter()
            .map(|it| {
                let path = by_name
                    .get(it.file.as_str())
                    .map(|p| p.to_path_buf())
                    .unwrap_or_default();
                (path, it.line)
            })
            .collect();
        let view = AgendaView {
            kind: kind.clone(),
            items: item_locs,
            line_map,
            rendered: text.clone(),
        };
        (text, view, count)
    }

    /// Render a flat list view (`title` + entries) to `(items, text, line_map)`.
    fn list_view(
        title: &str,
        items: Vec<crate::org::AgendaItem>,
    ) -> (Vec<crate::org::AgendaItem>, String, Vec<Option<usize>>) {
        let (text, map) = crate::org::render_list(title, &items);
        (items, text, map)
    }

    /// Open a built-in agenda `kind` in a read-only tab. The buffer is
    /// interactive: pressing `t` on a task line cycles that task's TODO state in
    /// its source file (see [`Self::agenda_todo_at_cursor`]).
    fn open_view(&mut self, kind: &AgendaKind) {
        self.build_file_index(); // pick up any newly added .org files
        let (text, view, count) = self.build_view(kind);
        self.editor.new_tab_with_content(&text);
        if let Some(tab) = self.editor.active_tab_mut() {
            tab.read_only = true;
        }
        self.agenda = Some(view);
        self.focus = Focus::Editor;
        self.status = t!("status.org_agenda_view", count = count).to_string();
    }

    /// Weekly/daily agenda (Org agenda `a`).
    fn org_agenda(&mut self) {
        self.open_view(&AgendaKind::Weekly);
    }

    /// Rebuild the open agenda buffer in place after a task changed, keeping the
    /// cursor on `cursor_line` and preserving which view is shown.
    fn rebuild_agenda(&mut self, cursor_line: usize) {
        let Some(kind) = self.agenda.as_ref().map(|v| v.kind.clone()) else {
            return;
        };
        let (text, view, _) = self.build_view(&kind);
        if let Some(tab) = self.editor.active_tab_mut() {
            tab.editor.set_content(&text);
            let last = text.split('\n').count().saturating_sub(1);
            tab.editor.set_cursor_line(cursor_line.min(last));
            tab.dirty = false;
            tab.read_only = true;
        }
        self.agenda = Some(view);
    }

    /// Org view-only keys handled before the read-only guard in the editor: a
    /// plain Tab folds/unfolds the drawer under the cursor, and a plain `t` in an
    /// agenda buffer cycles the task there. Returns `true` if the key was
    /// consumed (so it neither indents nor types).
    pub(super) fn org_view_key(&mut self, key: KeyEvent) -> bool {
        let plain = !Self::ctrl(&key) && !Self::alt(&key);
        if plain && !Self::shift(&key) && key.code == KeyCode::Tab && self.org_toggle_drawer_fold()
        {
            return true;
        }
        if plain && key.code == KeyCode::Char('t') && self.agenda_todo_at_cursor() {
            return true;
        }
        false
    }

    /// Org agenda `t`: cycle the TODO state of the task on the cursor line in its
    /// source `.org` file, then reload any open buffer for that file and rebuild
    /// the agenda. Returns `true` if the key was consumed — only when the active
    /// buffer is the current agenda view (so `t` types normally elsewhere).
    fn agenda_todo_at_cursor(&mut self) -> bool {
        if self.agenda.is_none() {
            return false;
        }
        let Some(tab) = self.editor.active_tab() else {
            return false;
        };
        let cur_text = tab.text();
        let line = tab.editor.cursor_line();
        // Confirm the active buffer is still the agenda, and locate the task.
        let target = match &self.agenda {
            Some(view) if view.rendered == cur_text => view
                .line_map
                .get(line)
                .copied()
                .flatten()
                .map(|idx| view.items[idx].clone()),
            _ => return false,
        };
        let Some((path, src_line)) = target else {
            self.status = t!("status.org_agenda_no_task").to_string();
            return true;
        };
        let Ok(content) = std::fs::read_to_string(&path) else {
            self.status = t!("status.org_agenda_error").to_string();
            return true;
        };
        let Some(new) = crate::org::cycle_todo(&content, src_line) else {
            self.status = t!("status.org_not_headline").to_string();
            return true;
        };
        if std::fs::write(&path, &new).is_err() {
            self.status = t!("status.org_agenda_error").to_string();
            return true;
        }
        // Refresh any open, clean buffer for the edited file, then rebuild.
        self.editor.reload_clean_from_disk();
        self.rebuild_agenda(line);
        self.status = t!("status.org_agenda_toggled").to_string();
        true
    }

    /// Build a clock-time report from the active buffer into a new tab.
    fn org_time_report(&mut self) {
        let Some(text) = self.editor.active_tab().map(crate::editor::Tab::text) else {
            return;
        };
        let report = crate::org::time_report(&text);
        self.editor.new_tab_with_content(&report);
        self.status = t!("status.org_time_report").to_string();
    }

    /// Dispatch a submitted Org-roam / Org-node prompt to the matching action.
    pub(super) fn accept_roam_prompt(&mut self, kind: PromptKind, input: &str) {
        match kind {
            PromptKind::OrgCaptureField => self.accept_capture_field(input),
            PromptKind::OrgCaptureReview => self.accept_capture_review(input),
            PromptKind::RoamFind | PromptKind::RoamCapture => {
                self.roam_visit_or_create(input);
            }
            PromptKind::RoamInsert => self.roam_insert_link(input),
            PromptKind::NodeTransclusion => self.node_insert_transclusion(input),
            PromptKind::WorkspaceOpen => self.workspace_open(input),
            PromptKind::WorkspaceSave => self.workspace_save(input),
            PromptKind::WorkspaceAddFolder => self.workspace_add_folder(input),
            PromptKind::GotoParagraph
            | PromptKind::GotoSection
            | PromptKind::GotoSentence
            | PromptKind::GotoWord
            | PromptKind::GotoPercent
            | PromptKind::GotoByte => self.accept_goto_number(kind, input),
            PromptKind::RoamDailyCapture => self.roam_daily_capture(input),
            PromptKind::RoamDailyDate if !input.is_empty() => self.roam_open_daily(input),
            PromptKind::RoamTag if !input.is_empty() => {
                let tag = input.to_string();
                self.roam_rewrite_active(|t| crate::roam::add_filetag(t, &tag));
            }
            PromptKind::RoamAlias if !input.is_empty() => {
                let alias = input.to_string();
                self.roam_rewrite_active(|t| {
                    crate::roam::append_property(t, "ROAM_ALIASES", &format!("\"{alias}\""))
                });
            }
            PromptKind::RoamRef if !input.is_empty() => {
                let r = input.to_string();
                self.roam_rewrite_active(|t| crate::roam::append_property(t, "ROAM_REFS", &r));
            }
            _ => {}
        }
    }

    /// Toggle the fold whose range starts at the cursor's line (LSP-provided
    /// ranges). Reports when there is nothing foldable there.
    /// In an `.org` buffer, fold or unfold the drawer whose `:NAME:` header the
    /// cursor sits on (hiding its body through `:END:`, or revealing it again),
    /// mirroring code folding. Returns `true` if it handled the key — so the
    /// caller lets Tab fall through to indentation everywhere else. A no-op (and
    /// `false`) when the buffer is not Org, a selection is active, or the cursor
    /// is not on a drawer header.
    fn org_toggle_drawer_fold(&mut self) -> bool {
        if !self.active_is_org() {
            return false;
        }
        let Some(t) = self.editor.active_tab_mut() else {
            return false;
        };
        // With a selection, Tab means "indent"; leave drawer folding for a plain
        // caret on the header line.
        if t.editor.get_selection().is_some_and(|s| !s.is_empty()) {
            return false;
        }
        let range = {
            let code = t.editor.code_ref();
            let line = code.char_to_line(t.editor.get_cursor());
            let text = code.get_content();
            let lines: Vec<&str> = text.split('\n').collect();
            crate::org::drawer_range(&lines, line)
        };
        let Some((start, end)) = range else {
            return false;
        };
        t.editor.toggle_manual_fold(start, end)
    }

    /// When the cursor sits right after a freshly typed `[[mailto:` or
    /// `[[contact:` in an `.org` file, open the Org-contacts completion popup
    /// (email addresses for `mailto:`, contact names for `contact:`). Returns
    /// `true` when the popup was opened, so the key that triggered it (Tab or
    /// Alt+Tab) is consumed instead of indenting.
    pub(super) fn maybe_complete_org_contact_link(&mut self) -> bool {
        if self.completion.is_some() {
            return false;
        }
        let Some(path) = self.active_path() else {
            return false;
        };
        if path.extension().and_then(|e| e.to_str()) != Some("org") {
            return false;
        }
        let Some(tab) = self.editor.active_tab() else {
            return false;
        };
        let code = tab.editor.code_ref();
        let cur = tab.editor.get_cursor();
        let line = code.char_to_line(cur);
        let line_start = code.line_to_char(line);
        let before = code.slice(line_start, cur);
        let Some(&(_, mailto)) = [("[[mailto:", true), ("[[contact:", false)]
            .iter()
            .find(|(marker, _)| before.ends_with(marker))
        else {
            return false;
        };
        self.open_org_contact_link_completion(mailto);
        true
    }

    pub(super) fn capture_chooser_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                let n = self.settings.org_capture_templates.len();
                if let Some(cc) = self.capture_chooser.as_mut() {
                    cc.selected = (cc.selected + n - 1) % n;
                }
            }
            KeyCode::Down => {
                let n = self.settings.org_capture_templates.len();
                if let Some(cc) = self.capture_chooser.as_mut() {
                    cc.selected = (cc.selected + 1) % n;
                }
            }
            KeyCode::Enter => self.accept_capture_chooser(),
            KeyCode::Esc => {
                self.capture_chooser = None;
            }
            _ => {}
        }
    }

    /// Start capturing with the highlighted template and close the chooser.
    fn accept_capture_chooser(&mut self) {
        if let Some(cc) = self.capture_chooser.take()
            && let Some(template) = self
                .settings
                .org_capture_templates
                .get(cc.selected)
                .cloned()
        {
            self.start_capture(template);
        }
    }

    pub(super) fn capture_chooser_mouse(&mut self, mouse: MouseEvent) {
        if let Some(idx) = self.chooser_row(mouse)
            && let Some(cc) = self.capture_chooser.as_mut()
            && idx < self.settings.org_capture_templates.len()
        {
            cc.selected = idx;
            self.accept_capture_chooser();
        }
    }
}
