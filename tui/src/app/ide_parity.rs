//! IDE-parity features (T720-T727, `spec/ide-parity`): Local History, Search
//! Everywhere, the Refactor menu, Organize Imports and Code Cleanup, named run
//! configurations, Peek Definition and postfix completion (`CodeLens`, T726,
//! lives in `code_lens.rs` and uses the overlay here).
//!
//! Most of these are thin wiring over pure crates (`vix-undo-store`'s
//! `local_history`, `vix-tasks`' `run_config`, `vix-snippets`' `postfix`,
//! `vix-textops`' `cleanup`) and the existing palette, LSP code-action and
//! command-running plumbing. The shared pick-one overlay is
//! [`vix_choice_list::Panel`], wrapped in [`ChoiceList`] with the purpose that
//! says what a chosen row means.

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

use crate::geo::Geo;
use std::path::{Path, PathBuf};

use vix_input::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use vix_undo_store::local_history::{self, HistoryError, LocalHistory};

use super::{App, Focus, Prompt, PromptKind, rect_contains};
use crate::palette::{Action as PAction, Entry, Mode as PMode};

/// What a chosen row of the [`ChoiceList`] overlay means.
#[derive(Debug, Clone)]
pub(crate) enum ChoicePurpose {
    /// Local History snapshots of `file`, newest first; `lines` is the
    /// 0-based `start..end` line range "revert selection" applies to.
    LocalHistory {
        /// The file whose history is shown.
        file: PathBuf,
        /// Snapshot id per row.
        ids: Vec<u64>,
        /// The lines a range revert replaces.
        lines: (usize, usize),
    },
    /// Run configurations, one per row.
    RunConfig {
        /// Configuration name per row.
        names: Vec<String>,
    },
    /// A definition peek: Enter opens `path` at `line` (1-based).
    Peek {
        /// The definition's file.
        path: PathBuf,
        /// 1-based line of the definition.
        line: usize,
    },
    /// Code lenses (T726), one per row: Enter runs it.
    CodeLens {
        /// The lens per row.
        lenses: Vec<super::code_lens::Lens>,
    },
    /// Color presentations (T621), one per row: Enter applies its edits.
    ColorPresentation {
        /// The file the color is in.
        path: PathBuf,
        /// The edits of each row (its `textEdit`, then `additionalTextEdits`).
        edits: Vec<Vec<(crate::lsp_core::Range, String)>>,
    },
    /// **File → Open Remote…** (T713): row 0 types a new name, then one row
    /// per picker (T718c), then the recent remote names.
    RecentRemote {
        /// The pickers offered (Kubernetes, WSL), for rows 1 and on.
        pickers: Vec<vix_authority::container::ExecKind>,
        /// The recent names, for the rows after the pickers.
        names: Vec<String>,
    },
    /// A picker's results (T718c): one container or distribution per row;
    /// Enter fills in the Open Remote prompt with its name.
    #[cfg(feature = "tasks")]
    RemotePick {
        /// The target per row.
        targets: Vec<vix_authority::container::ContainerTarget>,
    },
    /// **Run → Exception Breakpoints…** (T617): one adapter exception filter
    /// per row; Enter toggles it and the list stays open.
    ExceptionFilters {
        /// The filter id per row.
        ids: Vec<String>,
    },
    /// **Run → Jupyter → Start Kernel…** (T734c): one kernelspec per row.
    #[cfg(feature = "jupyter")]
    Jupyter(super::jupyter::Pick),
}

/// The pick-one overlay and what its rows mean.
#[derive(Debug, Clone)]
pub(crate) struct ChoiceList {
    /// Rows, selection and scroll.
    pub panel: vix_choice_list::Panel,
    /// What Enter does.
    pub purpose: ChoicePurpose,
}

/// Why the last code-action request was made, so the reply is routed to the
/// chooser, the Refactor chooser or applied directly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum CodeActionIntent {
    /// Lightbulb menu: every kind, shown in the chooser.
    #[default]
    Menu,
    /// Refactor menu: only `refactor.*` kinds, shown in the chooser.
    Refactor,
    /// Organize imports: apply the first edit.
    Organize,
    /// Code cleanup's language-server step: same as organize, then a summary.
    Cleanup,
}

impl CodeActionIntent {
    /// The `CodeActionKind` filter sent to the server (`context.only`).
    pub(crate) fn only(self) -> &'static [&'static str] {
        match self {
            Self::Menu => &[],
            Self::Refactor => &["refactor"],
            Self::Organize | Self::Cleanup => &["source.organizeImports"],
        }
    }
}

/// Context lines shown above and below a peeked definition.
const PEEK_BEFORE: usize = 3;
/// Context lines shown below a peeked definition.
const PEEK_AFTER: usize = 12;

/// A starter `.vix/run.toml`.
const RUN_CONFIG_TEMPLATE: &str = "[[config]]\nname = \"build\"\ncommand = \"make\"\n\n\
[[config]]\nname = \"run\"\ncommand = \"make run\"\nbefore = [\"build\"]\n";

impl App {
    /// Dispatch the IDE-parity actions. Returns `true` if handled.
    pub(super) fn run_ide_parity_action(&mut self, action: &str) -> bool {
        match action {
            "file.local_history" => self.open_local_history(),
            "file.local_history_label" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::LocalHistoryLabel,
                    t!("prompt.local_history_label").to_string(),
                ));
            }
            "nav.search_everywhere" => self.open_search_everywhere(),
            "nav.peek_definition" => self.peek_definition(),
            "lsp.refactor" => self.request_code_actions_for(CodeActionIntent::Refactor),
            "lsp.organize_imports" => self.request_code_actions_for(CodeActionIntent::Organize),
            "edit.cleanup_code" => self.cleanup_code(),
            "project.run_config" => self.open_run_configs(),
            "project.run_config_edit" => self.edit_run_configs(),
            _ => return false,
        }
        true
    }

    // ----- shared overlay ---------------------------------------------------

    /// Handle a key for the pick-one overlay.
    pub(super) fn choice_list_key(&mut self, key: KeyEvent) {
        let page = (usize::from(self.layout.choice_list.height)).max(1);
        let Some(open) = self.choice_list.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Up => open.panel.up(),
            KeyCode::Down => open.panel.down(),
            KeyCode::PageUp => open.panel.page_up(page),
            KeyCode::PageDown => open.panel.page_down(page),
            KeyCode::Esc => self.choice_list = None,
            KeyCode::Enter => self.activate_choice(ChoiceAction::Primary),
            KeyCode::Char('o') => self.activate_choice(ChoiceAction::Open),
            KeyCode::Char('r') => self.activate_choice(ChoiceAction::RestoreFile),
            KeyCode::Char('s') => self.activate_choice(ChoiceAction::RestoreRange),
            _ => {}
        }
    }

    /// Click a row of the pick-one overlay: select it and run its primary action.
    pub(super) fn choice_list_mouse(&mut self, mouse: MouseEvent) {
        let rect = self.layout.choice_list;
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
            || !rect_contains(rect, mouse.column, mouse.row)
        {
            return;
        }
        let Some(open) = self.choice_list.as_mut() else {
            return;
        };
        let idx = open.panel.scroll.plus(usize::from(mouse.row.minus(rect.y)));
        if open.panel.select_index(idx) {
            self.activate_choice(ChoiceAction::Primary);
        }
    }

    /// Close the overlay and carry out `action` on the highlighted row.
    fn activate_choice(&mut self, action: ChoiceAction) {
        let Some(open) = self.choice_list.take() else {
            return;
        };
        let row = open.panel.selected;
        match open.purpose {
            ChoicePurpose::LocalHistory { file, ids, lines } => {
                if let Some(&id) = ids.get(row) {
                    self.local_history_act(&file, id, lines, action);
                }
            }
            ChoicePurpose::RunConfig { names } => {
                if matches!(action, ChoiceAction::Primary)
                    && let Some(name) = names.get(row)
                {
                    self.run_config_named(name);
                }
            }
            ChoicePurpose::CodeLens { lenses } => {
                if matches!(action, ChoiceAction::Primary)
                    && let Some(lens) = lenses.get(row)
                {
                    self.run_lens(lens);
                }
            }
            ChoicePurpose::ColorPresentation { path, edits } => {
                if matches!(action, ChoiceAction::Primary) {
                    self.apply_color_presentation(&path, &edits, row);
                }
            }
            ChoicePurpose::RecentRemote { pickers, names } => {
                if matches!(action, ChoiceAction::Primary) {
                    self.choose_recent_remote(&pickers, &names, row);
                }
            }
            #[cfg(feature = "tasks")]
            ChoicePurpose::RemotePick { targets } => {
                if matches!(action, ChoiceAction::Primary)
                    && let Some(target) = targets.get(row)
                {
                    self.remote_pick_chosen(target);
                }
            }
            ChoicePurpose::ExceptionFilters { ids } => {
                if matches!(action, ChoiceAction::Primary) {
                    self.toggle_exception_filter(&ids, row);
                }
            }
            #[cfg(feature = "jupyter")]
            ChoicePurpose::Jupyter(pick) => {
                if matches!(action, ChoiceAction::Primary) {
                    self.jupyter_picked(pick, row);
                }
            }
            ChoicePurpose::Peek { path, line } => {
                if matches!(action, ChoiceAction::Primary) {
                    self.with_jump(|s| {
                        s.open_path(&path, false);
                        let area = s.editor_view();
                        s.editor.goto(line, Some(1), area);
                        s.focus = Focus::Editor;
                    });
                }
            }
        }
    }

    // ----- T720 Local History ---------------------------------------------

    /// The Local History store: the test override, else the user's config dir.
    fn local_history_store(&self) -> Option<LocalHistory> {
        self.local_history_root
            .clone()
            .map(LocalHistory::at)
            .or_else(LocalHistory::default_location)
            .map(|h| h.with_keep(self.settings.editor_behavior.local_history_keep))
    }

    /// Take a snapshot of `text` for the file just saved at `path`.
    pub(super) fn take_local_history_snapshot(&self, path: &Path, text: &str) {
        if self.settings.editor_behavior.local_history_keep == 0 {
            return;
        }
        if let Some(store) = self.local_history_store() {
            // Best-effort, like persistent undo: a failed snapshot must never
            // fail the save itself.
            let _ = store.snapshot(path, text, None);
        }
    }

    /// File -> Local History: list the active file's snapshots.
    fn open_local_history(&mut self) {
        let Some(path) = self.active_path() else {
            self.status = t!("status.local_history_no_file").to_string();
            return;
        };
        let Some(store) = self.local_history_store() else {
            self.status = t!("status.local_history_none").to_string();
            return;
        };
        let snaps = store.list(&path);
        if snaps.is_empty() {
            self.status = t!("status.local_history_none").to_string();
            return;
        }
        let rows = snaps
            .iter()
            .map(|s| {
                let when = format_time_ms(s.id);
                let label = s.label.as_deref().unwrap_or("");
                format!("{when}  {:>9}  {label}", format_bytes(s.bytes))
            })
            .collect();
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let lines = self.selected_line_range();
        self.choice_list = Some(ChoiceList {
            panel: vix_choice_list::Panel::new(
                t!("ui.local_history", file = name).to_string(),
                t!("ui.local_history_hint").to_string(),
                rows,
            ),
            purpose: ChoicePurpose::LocalHistory {
                file: path,
                ids: snaps.iter().map(|s| s.id).collect(),
                lines,
            },
        });
    }

    /// The 0-based `start..end` lines of the selection, else the cursor line.
    fn selected_line_range(&mut self) -> (usize, usize) {
        let Some(tab) = self.editor.active_tab_mut() else {
            return (0, 1);
        };
        let sel = tab.editor.get_selection().filter(|s| !s.is_empty());
        let code = tab.editor.code_ref();
        if let Some(s) = sel {
            let first = code.char_to_line(s.start.min(s.end));
            let last = code.char_to_line(s.start.max(s.end));
            (first, last.plus(1))
        } else {
            let line = code.char_to_line(tab.editor.get_cursor());
            (line, line.plus(1))
        }
    }

    /// Carry out a Local History action on snapshot `id` of `file`.
    fn local_history_act(
        &mut self,
        file: &Path,
        id: u64,
        lines: (usize, usize),
        action: ChoiceAction,
    ) {
        let Some(store) = self.local_history_store() else {
            return;
        };
        let snapshot = match store.read(file, id) {
            Ok(s) => s,
            Err(e) => {
                self.report_history_error(&e);
                return;
            }
        };
        let name = file
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let when = format_time_ms(id);
        match action {
            ChoiceAction::Primary => {
                let current = self.active_text_of(file).unwrap_or_default();
                let diff = local_history::unified_diff(&snapshot, &current);
                let body = if diff.is_empty() {
                    t!("status.local_history_same").to_string()
                } else {
                    diff
                };
                self.open_readonly_text_tab(
                    &t!("ui.local_history_diff_title", file = name, when = when),
                    &body,
                );
            }
            ChoiceAction::Open => {
                self.open_readonly_text_tab(
                    &t!("ui.local_history_snapshot_title", file = name, when = when),
                    &snapshot,
                );
            }
            ChoiceAction::RestoreFile => self.restore_snapshot(file, &snapshot, None),
            ChoiceAction::RestoreRange => self.restore_snapshot(file, &snapshot, Some(lines)),
        }
    }

    /// The text of `file` if it is the active buffer.
    fn active_text_of(&self, file: &Path) -> Option<String> {
        let tab = self.editor.active_tab()?;
        (tab.path.as_deref() == Some(file)).then(|| tab.text())
    }

    /// Put `snapshot` (all of it, or just `lines`) into the active buffer,
    /// leaving the buffer dirty so the revert is itself undoable and saved
    /// only when the user chooses.
    fn restore_snapshot(&mut self, file: &Path, snapshot: &str, lines: Option<(usize, usize)>) {
        if self.active_read_only() {
            self.status = t!("status.read_only_blocked").to_string();
            return;
        }
        let Some(current) = self.active_text_of(file) else {
            self.status = t!("status.local_history_stale").to_string();
            return;
        };
        let new = match lines {
            None => snapshot.to_string(),
            Some((s, e)) => match local_history::revert_range(&current, snapshot, s, e) {
                Ok(text) => text,
                Err(e) => {
                    self.messages
                        .error(t!("msg.command_failed", error = e.to_string()).to_string());
                    return;
                }
            },
        };
        if new == current {
            self.status = t!("status.local_history_same").to_string();
            return;
        }
        if let Some(tab) = self.editor.active_tab_mut() {
            let caret = tab.editor.get_cursor().min(new.chars().count());
            tab.editor.set_content(&new);
            tab.editor.set_selection(None);
            tab.editor.set_cursor(caret);
            tab.dirty = true;
            tab.preview = false;
        }
        self.status = t!("status.local_history_restored").to_string();
    }

    /// The label prompt's accept: snapshot the active buffer under a label.
    pub(super) fn local_history_label(&mut self, label: &str) {
        let Some(tab) = self.editor.active_tab() else {
            return;
        };
        let (Some(path), text) = (tab.path.clone(), tab.text()) else {
            self.status = t!("status.local_history_no_file").to_string();
            return;
        };
        let Some(store) = self.local_history_store() else {
            return;
        };
        match store.snapshot(&path, &text, Some(label)) {
            Ok(_) => self.status = t!("status.local_history_labelled", label = label).to_string(),
            Err(e) => self.report_history_error(&e),
        }
    }

    /// Show a Local History failure.
    fn report_history_error(&mut self, e: &HistoryError) {
        self.messages
            .error(t!("msg.local_history_failed", error = e.to_string()).to_string());
    }

    // ----- T721 Search Everywhere -------------------------------------------

    /// Open the palette as one box over files, commands, symbols and lines.
    /// An unprefixed query searches them all; `>`, `@`, `@@`, `#` and `:`
    /// still pick a single kind.
    fn open_search_everywhere(&mut self) {
        self.open_palette_seeded("");
        self.palette_everywhere = true;
        self.recompute_palette();
    }

    /// The mixed result list for an unprefixed Search Everywhere query: the
    /// best files, commands, symbols of the open file and workspace symbols,
    /// in that order, each capped so one kind cannot crowd out the rest.
    pub(super) fn everywhere_entries(&self, query: &str) -> Vec<Entry> {
        // Per-kind caps for the mixed Search Everywhere list: files first, then commands, then
        // symbols.
        const FILES: usize = 12;
        const COMMANDS: usize = 8;
        const SYMBOLS: usize = 6;
        let mut out: Vec<Entry> = self
            .palette_entries(PMode::Files, query)
            .into_iter()
            .take(FILES)
            .collect();
        if query.trim().is_empty() {
            return out;
        }
        out.extend(
            self.palette_entries(PMode::Commands, query)
                .into_iter()
                .take(COMMANDS),
        );
        out.extend(
            self.palette_entries(PMode::Symbols, query)
                .into_iter()
                .take(SYMBOLS),
        );
        out.extend(
            self.palette_entries(PMode::WorkspaceSymbols, query)
                .into_iter()
                .take(SYMBOLS),
        );
        if let Ok(n) = query.trim().parse::<usize>() {
            out.push(Entry {
                label: format!(": {n}"),
                action: PAction::GotoLine(n),
            });
        }
        out
    }

    // ----- T722 / T723 Refactor, Organize imports, Code cleanup -----------

    /// Ask the language server for code actions of the kinds `intent` wants.
    fn request_code_actions_for(&mut self, intent: CodeActionIntent) {
        if !self.active_path().is_some_and(|p| self.lsp.handles(&p)) {
            self.status = t!("status.lsp_inactive").to_string();
            return;
        }
        self.code_action_intent = intent;
        self.request_code_action();
    }

    /// Route a `textDocument/codeAction` reply by the intent that asked.
    pub(super) fn on_code_actions(&mut self, actions: Vec<crate::lsp::CodeAction>) {
        let intent = std::mem::take(&mut self.code_action_intent);
        match intent {
            CodeActionIntent::Menu | CodeActionIntent::Refactor => {
                if actions.is_empty() && intent == CodeActionIntent::Refactor {
                    self.status = t!("status.refactor_none").to_string();
                    return;
                }
                self.code_actions = Some(super::CodeActionMenu {
                    actions,
                    selected: 0,
                    refactor: intent == CodeActionIntent::Refactor,
                });
            }
            CodeActionIntent::Organize | CodeActionIntent::Cleanup => {
                let applied = self.apply_first_edit(actions);
                self.status = match applied {
                    Some(true) => t!("status.organize_done").to_string(),
                    Some(false) => t!("status.organize_command_only").to_string(),
                    None => t!("status.organize_none").to_string(),
                };
            }
        }
    }

    /// Apply the first action that carries an edit. `None` when there are no
    /// actions, `Some(false)` when none had an inline edit (command-only),
    /// `Some(true)` when an edit was applied.
    fn apply_first_edit(&mut self, actions: Vec<crate::lsp::CodeAction>) -> Option<bool> {
        if actions.is_empty() {
            return None;
        }
        match actions.into_iter().find(|(_, edit)| !edit.is_empty()) {
            Some((_, edit)) => {
                self.apply_workspace_edit(&edit);
                Some(true)
            }
            None => Some(false),
        }
    }

    /// Edit -> Code Cleanup: tidy whitespace now, then (with a language server)
    /// organize imports.
    fn cleanup_code(&mut self) {
        if self.active_read_only() {
            self.status = t!("status.read_only_blocked").to_string();
            return;
        }
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        if tab.is_image() {
            return;
        }
        let (new, report) = crate::textops::cleanup::cleanup(&tab.editor.get_content());
        if !report.is_clean() {
            let caret = tab.editor.get_cursor().min(new.chars().count());
            tab.editor.set_content(&new);
            tab.editor.set_cursor(caret);
            tab.dirty = true;
            tab.preview = false;
        }
        let tidied = t!(
            "status.cleanup_done",
            lines = report.trimmed_lines.plus(report.trailing_blank_lines)
        )
        .to_string();
        if self.active_path().is_some_and(|p| self.lsp.handles(&p)) {
            self.request_code_actions_for(CodeActionIntent::Cleanup);
        } else {
            self.status = tidied;
        }
    }

    /// Preview the highlighted code action as a unified diff in a read-only tab.
    pub(super) fn preview_selected_code_action(&mut self) {
        use std::fmt::Write as _;
        let Some(menu) = self.code_actions.as_ref() else {
            return;
        };
        let Some((title, edits)) = menu.actions.get(menu.selected).cloned() else {
            return;
        };
        if edits.is_empty() {
            self.status = t!("status.code_action_no_edit", title = title).to_string();
            return;
        }
        let mut body = String::new();
        for (path, file_edits) in &edits {
            let enc = self.lsp.encoding_for(path);
            let old = self
                .active_text_of(path)
                .or_else(|| {
                    self.editor
                        .tabs
                        .iter()
                        .find(|t| t.path.as_deref() == Some(path.as_path()))
                        .map(crate::editor::Tab::text)
                })
                .or_else(|| crate::authority::current().read_to_string(path).ok())
                .unwrap_or_default();
            let new = super::apply_edits_to_text(&old, enc, file_edits);
            let _ = writeln!(body, "--- {0}\n+++ {0}", path.display());
            body.push_str(&local_history::unified_diff(&old, &new));
        }
        self.open_readonly_text_tab(&t!("ui.refactor_preview_title", title = title), &body);
        self.code_actions = None;
    }

    // ----- T724 run configurations ------------------------------------------

    /// Project -> Run Configuration...: pick a saved configuration and run it.
    fn open_run_configs(&mut self) {
        let configs = match crate::tasks::run_config::load(&self.root) {
            Ok(c) => c,
            Err(e) => {
                self.report_run_config_error(&e);
                return;
            }
        };
        if configs.is_empty() {
            self.status = t!("status.run_config_none").to_string();
            return;
        }
        let rows = configs
            .iter()
            .map(|c| match (&c.command, c.compound.is_empty()) {
                (Some(cmd), _) => format!("{}  -  {cmd}", c.name),
                (None, _) => format!("{}  -  {}", c.name, c.compound.join(" + ")),
            })
            .collect();
        self.choice_list = Some(ChoiceList {
            panel: vix_choice_list::Panel::new(
                t!("ui.run_config").to_string(),
                t!("ui.run_config_hint").to_string(),
                rows,
            ),
            purpose: ChoicePurpose::RunConfig {
                names: configs.into_iter().map(|c| c.name).collect(),
            },
        });
    }

    /// Plan and run the configuration called `name` through the one async
    /// command pipeline, as a single shell line (before-launch steps chained
    /// with `&&`, a compound's members started together).
    pub(super) fn run_config_named(&mut self, name: &str) {
        let configs = match crate::tasks::run_config::load(&self.root) {
            Ok(c) => c,
            Err(e) => {
                self.report_run_config_error(&e);
                return;
            }
        };
        let tasks = crate::tasks::load(&self.root);
        let plan = match crate::tasks::run_config::plan(&configs, &tasks, name) {
            Ok(p) => p,
            Err(e) => {
                self.report_run_config_error(&e);
                return;
            }
        };
        // The shell where the configuration runs (T716): a remote
        // workspace's is POSIX even from a Windows client.
        let shell = if crate::authority::current().shell_is_posix() {
            crate::tasks::run_config::Shell::Posix
        } else {
            crate::tasks::run_config::Shell::Cmd
        };
        let command = plan.shell_command(shell);
        let root = self.root.clone();
        self.run_command_in(&root, &command);
        self.status = t!("status.run_config_started", name = name).to_string();
        self.project_last_command = Some(command);
    }

    /// Open `.vix/run.toml`, creating a starter file first when absent.
    fn edit_run_configs(&mut self) {
        let dir = self.root.join(".vix");
        let path = dir.join("run.toml");
        let auth = crate::authority::current();
        if !auth.exists(&path) {
            let made = auth
                .create_dir_all(&dir)
                .and_then(|()| auth.write(&path, RUN_CONFIG_TEMPLATE.as_bytes()));
            if let Err(e) = made {
                self.messages
                    .error(t!("msg.command_failed", error = e).to_string());
                return;
            }
        }
        self.open_path(&path, false);
        self.focus = Focus::Editor;
    }

    /// Show a run-configuration failure.
    fn report_run_config_error(&mut self, e: &crate::tasks::run_config::RunConfigError) {
        use crate::tasks::run_config::RunConfigError as E;
        let text = match e {
            E::Parse(m) => t!("msg.run_config_parse", error = m),
            E::Duplicate(n) => t!("msg.run_config_duplicate", name = n),
            E::Invalid(n) => t!("msg.run_config_invalid", name = n),
            E::Unknown { config, name } => {
                t!("msg.run_config_unknown", config = config, name = name)
            }
            E::Cycle(n) => t!("msg.run_config_cycle", name = n),
            E::NotFound(n) => t!("msg.run_config_not_found", name = n),
            _ => t!("msg.run_config_parse", error = e.to_string()),
        };
        self.messages.error(text.to_string());
    }

    // ----- T725 Peek Definition ---------------------------------------------

    /// Show the definition of the symbol under the cursor in a popup, without
    /// leaving the current file.
    fn peek_definition(&mut self) {
        if let Some(path) = self.active_path()
            && self.lsp.handles(&path)
        {
            let (line, character) = self.cursor_lsp_position(&path);
            self.peek_pending = true;
            self.lsp.request_definition(&path, line, character);
            return;
        }
        let Some(symbol) = self.symbol_under_cursor() else {
            self.messages.warn(t!("msg.no_symbol"));
            return;
        };
        self.build_file_index();
        let hits = self.find_definitions(&symbol);
        match hits.first() {
            Some(h) => {
                let (path, line) = (h.path.clone(), h.line);
                self.show_peek(&path, line);
            }
            None => self
                .messages
                .warn(t!("msg.no_definition", symbol = symbol).to_string()),
        }
    }

    /// A language-server definition reply while a peek is pending: show it.
    /// Returns `true` when it was consumed as a peek.
    pub(super) fn finish_peek(&mut self, path: &Path, line0: u32) -> bool {
        if !std::mem::take(&mut self.peek_pending) {
            return false;
        }
        self.show_peek(path, usize::try_from(line0).unwrap_or(usize::MAX).plus(1));
        true
    }

    /// Fill the overlay with the lines around 1-based `line` of `path`.
    fn show_peek(&mut self, path: &Path, line: usize) {
        let Ok(text) = crate::authority::current().read_to_string_guarded(path) else {
            self.status = t!("status.peek_unreadable").to_string();
            return;
        };
        let (rows, selected) = peek_rows(&text, line);
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        self.choice_list = Some(ChoiceList {
            panel: vix_choice_list::Panel::new(
                t!("ui.peek", file = name, line = line).to_string(),
                t!("ui.peek_hint").to_string(),
                rows,
            )
            .with_selected(selected),
            purpose: ChoicePurpose::Peek {
                path: path.to_path_buf(),
                line,
            },
        });
    }

    // ----- T727 Postfix completion ------------------------------------------

    /// If the text before the cursor ends in `expr.key` for a postfix template
    /// of the file's language, replace it with the expanded snippet.
    pub(super) fn expand_postfix(&mut self) -> bool {
        let Some(tab) = self.editor.active_tab() else {
            return false;
        };
        let Some(family) = tab
            .path
            .as_ref()
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .and_then(crate::snippets::postfix::Family::from_extension)
        else {
            return false;
        };
        if tab.is_image() {
            return false;
        }
        let cursor = tab.editor.get_cursor();
        let code = tab.editor.code_ref();
        let line_start = code.line_to_char(code.char_to_line(cursor));
        let before = code.slice(line_start, cursor);
        let Some(exp) = crate::snippets::postfix::expand(&before, family) else {
            return false;
        };
        let start = cursor.minus(exp.replace_chars);
        let area = self.editor_view();
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_selection_range(start, cursor);
            t.editor.focus(&area);
        }
        self.insert_snippet_body(&exp.body);
        true
    }
}

/// What a keypress asks of the highlighted row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChoiceAction {
    /// Enter / click: the purpose's main action.
    Primary,
    /// `o`: open the snapshot's text.
    Open,
    /// `r`: restore the whole file from the snapshot.
    RestoreFile,
    /// `s`: restore only the selected lines from the snapshot.
    RestoreRange,
}

/// Rows for a peek at 1-based `line` of `text` (numbered, the definition line
/// marked) and the index of the definition row.
fn peek_rows(text: &str, line: usize) -> (Vec<String>, usize) {
    let target = line.minus(1);
    let first = target.minus(PEEK_BEFORE);
    let rows: Vec<String> = text
        .lines()
        .enumerate()
        .skip(first)
        .take(PEEK_BEFORE + PEEK_AFTER + 1)
        .map(|(i, l)| {
            let mark = if i == target { '>' } else { ' ' };
            format!("{mark} {:>5}  {l}", i.plus(1))
        })
        .collect();
    (rows, target.minus(first))
}

/// `YYYY-MM-DD HH:MM:SS` in the local time zone for `ms` since the Unix epoch.
fn format_time_ms(ms: u64) -> String {
    i64::try_from(ms)
        .ok()
        .and_then(|m| jiff::Timestamp::from_millisecond(m).ok())
        .map_or_else(String::new, |ts| {
            ts.to_zoned(jiff::tz::TimeZone::system())
                .strftime("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
}

/// A short size such as `812 B` or `3.4 KB`.
fn format_bytes(n: u64) -> String {
    // Binary (1024-based) units, matching how file sizes are shown elsewhere in the UI.
    const KB: u64 = 1024;
    if n < KB {
        format!("{n} B")
    } else if n < KB * KB {
        format!("{}.{} KB", n.quot(KB), n.modulo(KB).times(10).quot(KB))
    } else {
        format!(
            "{}.{} MB",
            n.quot(KB.times(KB)),
            n.modulo(KB.times(KB)).times(10).quot(KB.times(KB))
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peek_rows_number_lines_and_mark_the_definition() {
        let text = (1..=30)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let (rows, sel) = peek_rows(&text, 10);
        assert!(rows[sel].starts_with('>'));
        assert!(rows[sel].contains("10  10"));
        assert_eq!(sel, PEEK_BEFORE);
        let (rows, sel) = peek_rows(&text, 1);
        assert_eq!(sel, 0);
        assert!(rows[0].starts_with('>'));
    }

    #[test]
    fn byte_sizes_read_naturally() {
        assert_eq!(format_bytes(812), "812 B");
        assert_eq!(format_bytes(3 * 1024 + 512), "3.5 KB");
        assert_eq!(format_bytes(2 * 1024 * 1024), "2.0 MB");
    }

    #[test]
    fn the_intent_filters_match_the_lsp_kinds() {
        assert_eq!(CodeActionIntent::Menu.only(), [] as [&str; 0]);
        assert_eq!(CodeActionIntent::Refactor.only(), ["refactor"]);
        assert_eq!(
            CodeActionIntent::Organize.only(),
            ["source.organizeImports"]
        );
    }

    // ----- App-level behavior ----------------------------------------------

    use crate::settings::Settings;
    use ratatui::layout::Rect;
    use vix_input::KeyModifiers;

    fn app_in(tag: &str) -> (App, PathBuf) {
        let dir = std::env::temp_dir().join(format!("vix-r2c-idep-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = App::new(dir.clone(), Settings::default());
        app.layout.editor = Rect::new(0, 0, 80, 24);
        app.local_history_root = Some(dir.join(".history-store"));
        (app, dir)
    }

    fn open(app: &mut App, dir: &Path, name: &str, content: &str) -> PathBuf {
        let file = dir.join(name);
        std::fs::write(&file, content).unwrap();
        app.open_initial(&file);
        app.editor.active_tab().unwrap().path.clone().unwrap()
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn last_error(app: &App) -> String {
        app.messages.items.last().map_or_default(|m| m.text.clone())
    }

    fn list(purpose: ChoicePurpose, rows: &[&str]) -> ChoiceList {
        ChoiceList {
            panel: vix_choice_list::Panel::new(
                "title".to_string(),
                "hint".to_string(),
                rows.iter().map(ToString::to_string).collect(),
            ),
            purpose,
        }
    }

    #[test]
    fn the_overlay_keys_move_the_selection_and_esc_closes_it() {
        let (mut app, dir) = app_in("keys");
        // With nothing open, keys and clicks are ignored.
        app.choice_list_key(key(KeyCode::Down));
        app.choice_list_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 1,
            row: 1,
            modifiers: KeyModifiers::NONE,
        });
        app.activate_choice(ChoiceAction::Primary);
        assert!(app.choice_list.is_none());

        let rows = ["a", "b", "c", "d"];
        app.choice_list = Some(list(ChoicePurpose::RunConfig { names: Vec::new() }, &rows));
        app.layout.choice_list = Rect::new(2, 2, 30, 4);
        app.choice_list_key(key(KeyCode::Down));
        assert_eq!(app.choice_list.as_ref().unwrap().panel.selected, 1);
        app.choice_list_key(key(KeyCode::PageDown));
        let after_page = app.choice_list.as_ref().unwrap().panel.selected;
        assert!(after_page > 1);
        app.choice_list_key(key(KeyCode::PageUp));
        app.choice_list_key(key(KeyCode::Up));
        app.choice_list_key(key(KeyCode::Char('z')));
        assert!(app.choice_list.is_some());
        // A non-click mouse event and a click outside the rows do nothing.
        app.choice_list_mouse(MouseEvent {
            kind: MouseEventKind::Moved,
            column: 3,
            row: 3,
            modifiers: KeyModifiers::NONE,
        });
        app.choice_list_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 90,
            row: 30,
            modifiers: KeyModifiers::NONE,
        });
        assert!(app.choice_list.is_some());
        app.choice_list_key(key(KeyCode::Esc));
        assert!(app.choice_list.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn enter_on_a_peek_row_opens_the_file_at_its_line() {
        let (mut app, dir) = app_in("peekopen");
        open(&mut app, &dir, "a.txt", "x\n");
        let target = dir.join("def.txt");
        std::fs::write(&target, "l1\nl2\nl3\nl4\n").unwrap();
        app.choice_list = Some(list(
            ChoicePurpose::Peek {
                path: target,
                line: 3,
            },
            &["row"],
        ));
        app.choice_list_key(key(KeyCode::Enter));
        assert!(app.choice_list.is_none());
        assert_eq!(app.editor.cursor_1based().0, 3);
        assert_eq!(app.focus, Focus::Editor);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_click_on_a_color_presentation_row_applies_its_edit() {
        let (mut app, dir) = app_in("color");
        let file = open(&mut app, &dir, "a.css", "color: red;\n");
        let rev = app.editor.active_tab().unwrap().editor.revision();
        app.lsp_synced.insert(file.clone(), rev);
        let range = crate::lsp_core::Range {
            start: crate::lsp_core::Position {
                line: 0,
                character: 7,
            },
            end: crate::lsp_core::Position {
                line: 0,
                character: 10,
            },
        };
        app.choice_list = Some(list(
            ChoicePurpose::ColorPresentation {
                path: file,
                edits: vec![
                    vec![(range, "#f00".to_string())],
                    vec![(range, "blue".to_string())],
                ],
            },
            &["#f00", "blue"],
        ));
        app.layout.choice_list = Rect::new(0, 5, 30, 2);
        app.choice_list_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 2,
            row: 6,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.editor.active_tab().unwrap().text(), "color: blue;\n");
        assert_eq!(app.status, t!("status.color_applied").to_string());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_recent_remote_row_zero_opens_the_name_prompt() {
        let (mut app, dir) = app_in("recentremote");
        app.choice_list = Some(list(
            ChoicePurpose::RecentRemote {
                pickers: Vec::new(),
                names: vec!["ssh://u@h".to_string()],
            },
            &["new", "ssh://u@h"],
        ));
        app.choice_list_key(key(KeyCode::Enter));
        assert!(app.prompt.is_some());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn non_primary_keys_do_nothing_for_run_configs_and_filters() {
        let (mut app, dir) = app_in("nonprimary");
        app.choice_list = Some(list(
            ChoicePurpose::RunConfig {
                names: vec!["build".to_string()],
            },
            &["build"],
        ));
        let before = app.status.clone();
        app.choice_list_key(key(KeyCode::Char('o')));
        assert!(app.choice_list.is_none());
        assert_eq!(app.status, before, "no run was started");
        app.choice_list = Some(list(
            ChoicePurpose::ExceptionFilters {
                ids: vec!["raised".to_string()],
            },
            &["raised"],
        ));
        app.choice_list_key(key(KeyCode::Char('r')));
        assert!(app.choice_list.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn local_history_label_and_listing_need_a_file_and_a_store() {
        let (mut app, dir) = app_in("lhlabel");
        // A scratch buffer with no path cannot be labelled.
        app.editor.active_tab_mut().unwrap().path = None;
        app.local_history_label("v1");
        assert_eq!(app.status, t!("status.local_history_no_file").to_string());
        let file = open(&mut app, &dir, "a.txt", "one\n");
        app.local_history_label("v1");
        assert_eq!(
            app.status,
            t!("status.local_history_labelled", label = "v1").to_string()
        );
        app.run_action("file.local_history");
        let c = app.choice_list.as_ref().unwrap();
        assert!(matches!(c.purpose, ChoicePurpose::LocalHistory { .. }));
        assert!(c.panel.rows.iter().any(|r| r.contains("v1")));
        let _ = file;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn restoring_from_a_missing_snapshot_reports_an_error() {
        let (mut app, dir) = app_in("lhmissing");
        let file = open(&mut app, &dir, "a.txt", "one\n");
        app.local_history_act(&file, 12345, (0, 1), ChoiceAction::Primary);
        assert_ne!(last_error(&app), "");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_snapshot_for_another_file_is_stale_and_identical_text_is_the_same() {
        let (mut app, dir) = app_in("lhstale");
        let file = open(&mut app, &dir, "a.txt", "one\ntwo\n");
        app.local_history_label("snap");
        let store = app.local_history_store().unwrap();
        let id = store.list(&file).first().unwrap().id;
        // Primary on identical text opens a "same" diff tab.
        app.local_history_act(&file, id, (0, 1), ChoiceAction::Primary);
        let body = app.editor.active_tab().unwrap().text();
        assert_eq!(body, t!("status.local_history_same").to_string());
        // Restoring when another file is active is stale.
        let other = open(&mut app, &dir, "b.txt", "other\n");
        app.local_history_act(&file, id, (0, 1), ChoiceAction::RestoreFile);
        assert_eq!(app.status, t!("status.local_history_stale").to_string());
        let _ = other;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn restoring_the_same_text_or_an_out_of_range_revert_reports_nothing_to_do() {
        let (mut app, dir) = app_in("lhrange");
        let file = open(&mut app, &dir, "a.txt", "one\ntwo\n");
        app.restore_snapshot(&file, "one\ntwo\n", None);
        assert_eq!(app.status, t!("status.local_history_same").to_string());
        app.restore_snapshot(&file, "one\ntwo\n", Some((50, 60)));
        // A range past the end leaves the text unchanged, so it is "the same".
        assert_eq!(app.status, t!("status.local_history_same").to_string());
        assert_eq!(app.editor.active_tab().unwrap().text(), "one\ntwo\n");
        // A real range restore replaces just those lines and dirties the tab.
        app.restore_snapshot(&file, "ONE\ntwo\n", Some((0, 1)));
        assert_eq!(app.editor.active_tab().unwrap().text(), "ONE\ntwo\n");
        assert!(app.editor.active_tab().unwrap().dirty);
        assert_eq!(app.status, t!("status.local_history_restored").to_string());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn search_everywhere_adds_a_goto_line_row_for_a_number() {
        let (mut app, dir) = app_in("everywhere");
        open(&mut app, &dir, "notes.txt", "x\n");
        app.build_file_index();
        assert!(app.everywhere_entries("  ").len() <= 12);
        let entries = app.everywhere_entries("42");
        let goto = entries.last().unwrap();
        assert_eq!(goto.label, ": 42");
        assert!(matches!(goto.action, PAction::GotoLine(42)));
        let entries = app.everywhere_entries("notes");
        assert!(entries.iter().any(|e| e.label.contains("notes.txt")));
        std::fs::remove_dir_all(&dir).ok();
    }

    fn action(title: &str, edits: Vec<crate::lsp::FileEdits>) -> crate::lsp::CodeAction {
        (title.to_string(), edits)
    }

    #[test]
    fn code_action_replies_route_by_intent() {
        let (mut app, dir) = app_in("actions");
        let file = open(&mut app, &dir, "a.rs", "use b;\nuse a;\n");
        // Refactor with nothing offered says so; the menu keeps an empty list.
        app.code_action_intent = CodeActionIntent::Refactor;
        app.on_code_actions(Vec::new());
        assert_eq!(app.status, t!("status.refactor_none").to_string());
        assert!(app.code_actions.is_none());
        app.code_action_intent = CodeActionIntent::Menu;
        app.on_code_actions(vec![action("Fix it", Vec::new())]);
        let menu = app.code_actions.take().unwrap();
        assert!(!menu.refactor);
        app.code_action_intent = CodeActionIntent::Refactor;
        app.on_code_actions(vec![action("Extract", Vec::new())]);
        assert!(app.code_actions.take().unwrap().refactor);

        // Organize imports: none, command-only, then an inline edit.
        app.code_action_intent = CodeActionIntent::Organize;
        app.on_code_actions(Vec::new());
        assert_eq!(app.status, t!("status.organize_none").to_string());
        app.code_action_intent = CodeActionIntent::Organize;
        app.on_code_actions(vec![action("cmd", Vec::new())]);
        assert_eq!(app.status, t!("status.organize_command_only").to_string());
        let range = crate::lsp_core::Range {
            start: crate::lsp_core::Position {
                line: 0,
                character: 0,
            },
            end: crate::lsp_core::Position {
                line: 0,
                character: 6,
            },
        };
        app.code_action_intent = CodeActionIntent::Cleanup;
        app.on_code_actions(vec![action(
            "sort",
            vec![(file, vec![(range, "use c;".to_string())])],
        )]);
        assert_eq!(app.status, t!("status.organize_done").to_string());
        assert!(
            app.editor
                .active_tab()
                .unwrap()
                .text()
                .starts_with("use c;")
        );
        assert_eq!(app.code_action_intent, CodeActionIntent::Menu);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn previewing_a_code_action_shows_a_diff_in_a_read_only_tab() {
        let (mut app, dir) = app_in("preview");
        let file = open(&mut app, &dir, "a.rs", "let x = 1;\n");
        let closed = dir.join("closed.rs");
        std::fs::write(&closed, "let y = 2;\n").unwrap();
        // Nothing selected: ignored.
        app.preview_selected_code_action();
        let range = crate::lsp_core::Range {
            start: crate::lsp_core::Position {
                line: 0,
                character: 4,
            },
            end: crate::lsp_core::Position {
                line: 0,
                character: 5,
            },
        };
        app.code_actions = Some(crate::app::CodeActionMenu {
            actions: vec![action("No edit", Vec::new())],
            selected: 0,
            refactor: false,
        });
        app.preview_selected_code_action();
        assert_eq!(
            app.status,
            t!("status.code_action_no_edit", title = "No edit").to_string()
        );
        app.code_actions = Some(crate::app::CodeActionMenu {
            actions: vec![action(
                "Rename",
                vec![
                    (file, vec![(range, "renamed".to_string())]),
                    (closed.clone(), vec![(range, "other".to_string())]),
                ],
            )],
            selected: 0,
            refactor: true,
        });
        app.preview_selected_code_action();
        assert!(app.code_actions.is_none());
        let body = app.editor.active_tab().unwrap().text();
        assert!(body.contains("+let renamed = 1;"));
        assert!(body.contains("+let other = 2;"));
        assert!(body.contains(&format!("--- {}", closed.display())));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cleanup_code_tidies_whitespace_and_respects_read_only() {
        let (mut app, dir) = app_in("cleanup");
        open(&mut app, &dir, "a.txt", "keep   \ntrail  \n\n\n");
        app.run_action("edit.cleanup_code");
        assert_eq!(app.editor.active_tab().unwrap().text(), "keep\ntrail\n");
        assert!(app.editor.active_tab().unwrap().dirty);
        assert!(app.status.contains('4') || !app.status.is_empty());
        // Already clean: nothing changes, the summary is still reported.
        app.run_action("edit.cleanup_code");
        assert_eq!(app.editor.active_tab().unwrap().text(), "keep\ntrail\n");
        app.editor.active_tab_mut().unwrap().read_only = true;
        app.run_action("edit.cleanup_code");
        assert_eq!(app.status, t!("status.read_only_blocked").to_string());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn refactor_and_organize_need_a_language_server() {
        let (mut app, dir) = app_in("needlsp");
        open(&mut app, &dir, "a.txt", "x\n");
        app.run_action("lsp.refactor");
        assert_eq!(app.status, t!("status.lsp_inactive").to_string());
        app.status.clear();
        app.run_action("lsp.organize_imports");
        assert_eq!(app.status, t!("status.lsp_inactive").to_string());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn run_configurations_list_start_a_template_and_report_errors() {
        let (mut app, dir) = app_in("runcfg");
        open(&mut app, &dir, "a.txt", "x\n");
        app.run_action("project.run_config");
        assert_eq!(app.status, t!("status.run_config_none").to_string());
        // The edit action writes the starter file and opens it.
        app.run_action("project.run_config_edit");
        let path = dir.join(".vix").join("run.toml");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), RUN_CONFIG_TEMPLATE);
        assert!(
            app.editor
                .active_tab()
                .unwrap()
                .path
                .as_ref()
                .unwrap()
                .ends_with("run.toml")
        );
        // A second edit keeps the file as it is.
        std::fs::write(&path, format!("{RUN_CONFIG_TEMPLATE}\n# mine\n")).unwrap();
        app.run_action("project.run_config_edit");
        assert!(std::fs::read_to_string(&path).unwrap().contains("# mine"));
        // Listing shows each configuration with its command or compound.
        app.run_action("project.run_config");
        let c = app.choice_list.take().unwrap();
        assert!(c.panel.rows.iter().any(|r| r.contains("build  -  make")));
        assert!(c.panel.rows.iter().any(|r| r.contains("run  -  make run")));
        // Compounds are listed by their members.
        std::fs::write(
            &path,
            "[[config]]\nname = \"a\"\ncommand = \"true\"\n\n[[config]]\nname = \"both\"\ncompound = [\"a\"]\n",
        )
        .unwrap();
        app.run_action("project.run_config");
        let c = app.choice_list.take().unwrap();
        assert!(c.panel.rows.iter().any(|r| r.contains("both  -  a")));
        // Bad TOML is reported, not listed; an unknown name is not found.
        std::fs::write(&path, "[[config\n").unwrap();
        app.run_action("project.run_config");
        assert!(app.choice_list.is_none());
        assert_ne!(last_error(&app), "");
        app.messages.items.clear();
        app.run_config_named("anything");
        assert_ne!(last_error(&app), "");
        std::fs::write(&path, "[[config]]\nname = \"a\"\ncommand = \"true\"\n").unwrap();
        app.messages.items.clear();
        app.run_config_named("missing");
        assert_ne!(last_error(&app), "");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn every_run_config_error_has_its_own_message() {
        use crate::tasks::run_config::RunConfigError as E;
        let (mut app, dir) = app_in("cfgerr");
        let errors = [
            E::Parse("bad".to_string()),
            E::Duplicate("dup".to_string()),
            E::Invalid("inv".to_string()),
            E::Unknown {
                config: "c".to_string(),
                name: "n".to_string(),
            },
            E::Cycle("cyc".to_string()),
            E::NotFound("nf".to_string()),
        ];
        let mut seen = std::collections::HashSet::new();
        for e in &errors {
            app.report_run_config_error(e);
            assert!(
                seen.insert(last_error(&app)),
                "each variant words its own message"
            );
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn peek_definition_falls_back_to_a_workspace_search() {
        let (mut app, dir) = app_in("peek");
        std::fs::write(dir.join("lib.rs"), "fn alpha() {}\nfn target_fn() {}\n").unwrap();
        open(&mut app, &dir, "main.rs", "target_fn();\n");
        app.run_action("nav.peek_definition");
        let c = app.choice_list.as_ref().expect("a peek opened");
        assert!(matches!(c.purpose, ChoicePurpose::Peek { line: 2, .. }));
        assert!(
            c.panel
                .rows
                .iter()
                .any(|r| r.starts_with('>') && r.contains("target_fn"))
        );
        app.choice_list = None;
        // A symbol with no definition warns; no symbol at all warns too.
        app.editor
            .active_tab_mut()
            .unwrap()
            .editor
            .set_content("nosuchsym();\n");
        app.editor.active_tab_mut().unwrap().editor.set_cursor(2);
        app.run_action("nav.peek_definition");
        assert!(app.choice_list.is_none());
        assert!(last_error(&app).contains("nosuchsym"));
        app.editor
            .active_tab_mut()
            .unwrap()
            .editor
            .set_content("   \n");
        app.run_action("nav.peek_definition");
        assert!(app.choice_list.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_pending_peek_consumes_the_server_reply_once() {
        let (mut app, dir) = app_in("finishpeek");
        let target = dir.join("def.rs");
        std::fs::write(&target, "a\nb\nc\n").unwrap();
        assert!(!app.finish_peek(&target, 1), "no peek was pending");
        app.peek_pending = true;
        assert!(app.finish_peek(&target, 1));
        assert!(matches!(
            app.choice_list.as_ref().unwrap().purpose,
            ChoicePurpose::Peek { line: 2, .. }
        ));
        assert!(!app.finish_peek(&target, 1), "the flag is cleared");
        app.choice_list = None;
        app.show_peek(&dir.join("gone.rs"), 1);
        assert_eq!(app.status, t!("status.peek_unreadable").to_string());
        assert!(app.choice_list.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn postfix_completion_needs_a_known_language_and_a_template() {
        let (mut app, dir) = app_in("postfix");
        open(&mut app, &dir, "a.zzz", "x.if");
        app.editor.active_tab_mut().unwrap().editor.set_cursor(4);
        assert!(!app.expand_postfix(), "unknown extension has no templates");
        open(&mut app, &dir, "a.rs", "nothing here");
        app.editor.active_tab_mut().unwrap().editor.set_cursor(12);
        assert!(!app.expand_postfix(), "no postfix key before the cursor");
        open(&mut app, &dir, "b.rs", "flag.if");
        app.editor.active_tab_mut().unwrap().editor.set_cursor(7);
        assert!(app.expand_postfix());
        assert!(app.editor.active_tab().unwrap().text().contains("if flag"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn times_render_as_local_date_and_clock() {
        let s = format_time_ms(0);
        assert_eq!(s.len(), "1970-01-01 00:00:00".len());
        assert!(s.contains(':') && s.contains('-'));
        assert_eq!(format_time_ms(u64::MAX), "");
    }
}
