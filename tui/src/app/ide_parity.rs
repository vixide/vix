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
    /// **File → Open Remote…** (T713): row 0 types a new name, the rest are
    /// the recent remote names.
    RecentRemote {
        /// The recent names, for rows 1 and on.
        names: Vec<String>,
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
        let page = (self.layout.choice_list.height as usize).max(1);
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
        let idx = open.panel.scroll + (mouse.row - rect.y) as usize;
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
            ChoicePurpose::RecentRemote { names } => {
                if matches!(action, ChoiceAction::Primary) {
                    self.choose_recent_remote(&names, row);
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
            (first, last + 1)
        } else {
            let line = code.char_to_line(tab.editor.get_cursor());
            (line, line + 1)
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
            Some((s, e)) => local_history::revert_range(&current, snapshot, s, e),
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
            lines = report.trimmed_lines + report.trailing_blank_lines
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
        self.show_peek(path, line0 as usize + 1);
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
        let start = cursor - exp.replace_chars;
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
    let target = line.saturating_sub(1);
    let first = target.saturating_sub(PEEK_BEFORE);
    let rows: Vec<String> = text
        .lines()
        .enumerate()
        .skip(first)
        .take(PEEK_BEFORE + PEEK_AFTER + 1)
        .map(|(i, l)| {
            let mark = if i == target { '>' } else { ' ' };
            format!("{mark} {:>5}  {l}", i + 1)
        })
        .collect();
    (rows, target - first)
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
        format!("{}.{} KB", n / KB, n % KB * 10 / KB)
    } else {
        format!("{}.{} MB", n / (KB * KB), n % (KB * KB) * 10 / (KB * KB))
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
}
