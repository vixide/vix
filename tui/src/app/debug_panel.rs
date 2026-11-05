//! The debugger's deeper UI (T617, `crates/vix-dap/spec`): applying the
//! client's events to the Debug panel's tree, the panel's keys, Run → Edit
//! Breakpoint… (condition, hit condition, log message), the exception-filter
//! checklist, setting a variable, and the inline values on the stopped line.
//!
//! The tree itself is the pure `vix_dap::view::DebugView`; this module only
//! routes keys and events between it, the client and the editor.

use std::path::{Path, PathBuf};

use vix_input::{KeyCode, KeyEvent, KeyModifiers};

use super::ide_parity::{ChoiceList, ChoicePurpose};
use super::{App, Focus, Prompt, PromptKind, Visible};
use crate::dap::view::Activation;
use crate::dap::{DapEvent, Frame, SourceBreakpoint};
use crate::editor_core::virtual_text::EolText;

/// The debugger prompts in progress.
#[derive(Debug, Default)]
pub(crate) struct DebugEdit {
    /// The breakpoint being edited by the Edit Breakpoint prompts: its file
    /// and the draft built up step by step (committed after the last one).
    breakpoint: Option<(PathBuf, SourceBreakpoint)>,
    /// The variable a set-variable prompt is for: `(container reference, name)`.
    variable: Option<(i64, String)>,
}

impl App {
    /// Apply one debug-adapter event. Called by [`App::poll_dap`].
    pub(super) fn apply_dap_event(&mut self, event: DapEvent) {
        match event {
            DapEvent::Running => {
                self.status = t!("status.debug_running").to_string();
            }
            DapEvent::Stopped { reason, thread_id } => {
                self.dap_view.stopped(thread_id);
                self.status = t!("status.debug_stopped_at", reason = reason).to_string();
            }
            DapEvent::Output(text) => self.bottom_dock.push(text),
            DapEvent::Threads(threads) => self.dap_view.set_threads(threads),
            DapEvent::Stack(frames) => self.dap_stack_arrived(frames),
            DapEvent::Scopes { frame_id, scopes } => {
                for reference in self.dap_view.set_scopes(frame_id, &scopes) {
                    self.dap.variables(reference);
                }
                // The frame is known now, so watches evaluate in it.
                self.evaluate_watches();
            }
            DapEvent::Variables {
                reference,
                variables,
            } => {
                for child in self.dap_view.set_variables(reference, &variables) {
                    self.dap.variables(child);
                }
                self.refresh_debug_markers();
            }
            DapEvent::VariableSet {
                reference,
                name,
                value,
            } => {
                self.dap_view.variable_set(reference, &name, &value);
                self.status =
                    t!("status.debug_variable_set", name = name, value = value).to_string();
                self.evaluate_watches();
                self.refresh_debug_markers();
            }
            DapEvent::DataBreakpointInfo {
                name,
                data_id,
                description,
                access_types,
            } => self.data_breakpoint_info_arrived(&name, data_id, &description, &access_types),
            DapEvent::Evaluated { expr, result } => {
                // Update a matching watch, else echo to the debug console.
                if let Some(w) = self.dap_view.watches.iter_mut().find(|(e, _)| *e == expr) {
                    w.1 = result;
                } else {
                    self.bottom_dock.push(format!("{expr} = {result}"));
                }
            }
            DapEvent::Terminated => {
                self.dap_stopped = None;
                self.dap_view.clear();
                if self.focus == Focus::Debug {
                    self.focus = Focus::Editor;
                }
                self.refresh_debug_markers();
                self.status = t!("status.debug_terminated").to_string();
            }
            DapEvent::RequestFailed(message) => {
                // T536 (Run H): mirrors the LSP handling
                // (`LspEvent::RequestFailed`) — a rejected breakpoint or an
                // invalid step used to just silently "do nothing".
                self.status = t!("status.dap_request_failed", message = message).to_string();
            }
            // Capabilities are read from the client when needed.
            _ => {}
        }
    }

    /// A stack arrived for the selected thread: jump to its top frame.
    fn dap_stack_arrived(&mut self, frames: Vec<Frame>) {
        if let Some(top) = frames.first()
            && let Some(p) = top.path.clone()
        {
            self.dap_stopped = Some((PathBuf::from(&p), top.line));
            self.jump_to_debug_location(&p, top.line);
        }
        self.dap_view.set_stack(frames);
        self.refresh_debug_markers();
    }

    /// Re-evaluate every watch in the selected frame.
    fn evaluate_watches(&mut self) {
        if !self.dap.is_stopped() {
            return;
        }
        let exprs: Vec<String> = self
            .dap_view
            .watches
            .iter()
            .map(|(e, _)| e.clone())
            .collect();
        for e in exprs {
            self.dap.evaluate(&e);
        }
    }

    /// Continue or step via `command`, then clear the stop's inline values.
    pub(super) fn resume_debugger(&mut self, command: fn(&mut crate::dap::Dap)) {
        command(&mut self.dap);
        self.refresh_debug_markers();
    }

    /// The inline values for `path`'s editor (T617): the selected frame's
    /// line, when the program is stopped there.
    pub(super) fn debug_inline_values(&self, path: &Path) -> Vec<EolText> {
        if !self.dap.is_stopped() {
            return Vec::new();
        }
        let Some(frame) = self.dap_view.current_frame() else {
            return Vec::new();
        };
        if frame.path.as_deref().map(Path::new) != Some(path) || frame.line == 0 {
            return Vec::new();
        }
        let Some(text) = self.editor.active_tab().map(|t| t.editor.get_content()) else {
            return Vec::new();
        };
        let Some(line) = text.lines().nth(frame.line - 1) else {
            return Vec::new();
        };
        self.dap_view
            .inline_values(line)
            .map(|v| EolText::new(frame.line - 1, v))
            .into_iter()
            .collect()
    }

    // ----- the panel --------------------------------------------------------

    /// Run → Focus Debug Panel: show the panel and give it the keyboard.
    pub(super) fn focus_debug_panel(&mut self) {
        self.visible.set(Visible::DEBUG_PANEL, true);
        self.focus = Focus::Debug;
        self.dap_view.move_by(0);
    }

    /// A key while the Debug panel has focus. Chords and function keys fall
    /// through to the keymap (returns `false`), as for the IM panel.
    pub(super) fn debug_panel_key(&mut self, key: KeyEvent) -> bool {
        if !self.visible.contains(Visible::DEBUG_PANEL) {
            self.focus = Focus::Editor;
            return false;
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
            || matches!(key.code, KeyCode::F(_))
        {
            return false;
        }
        let page = isize::try_from(self.layout.debug_panel.height.max(1)).unwrap_or(1);
        match key.code {
            KeyCode::Up => self.dap_view.move_by(-1),
            KeyCode::Down => self.dap_view.move_by(1),
            KeyCode::PageUp => self.dap_view.move_by(-page),
            KeyCode::PageDown => self.dap_view.move_by(page),
            KeyCode::Home => self.dap_view.home(),
            KeyCode::End => self.dap_view.end(),
            KeyCode::Enter => {
                let a = self.dap_view.activate();
                self.debug_activate(a);
            }
            KeyCode::Right => {
                let a = self.dap_view.expand();
                self.debug_activate(a);
            }
            KeyCode::Left => self.dap_view.collapse(),
            KeyCode::Char('e') => self.begin_set_variable(),
            KeyCode::Char('b') => self.begin_data_breakpoint(),
            KeyCode::Esc => self.focus = Focus::Editor,
            _ => {}
        }
        true
    }

    /// Carry out what a panel key asked for.
    fn debug_activate(&mut self, activation: Activation) {
        match activation {
            Activation::SelectThread(id) => self.dap.select_thread(id),
            Activation::SelectFrame(id) => {
                self.dap.select_frame(id);
                if let Some((path, line)) = self
                    .dap_view
                    .current_frame()
                    .and_then(|f| f.path.clone().map(|p| (p, f.line)))
                {
                    self.jump_to_debug_location(&path, line);
                    self.focus = Focus::Debug;
                }
                self.refresh_debug_markers();
            }
            Activation::Load(reference) => self.dap.variables(reference),
            _ => {}
        }
    }

    /// `e` in the panel: prompt for the highlighted variable's new value.
    fn begin_set_variable(&mut self) {
        let Some((reference, name, value)) = self.dap_view.selected_variable() else {
            return;
        };
        if !self
            .dap
            .capabilities()
            .is_some_and(|c| c.supports_set_variable)
        {
            self.status = t!("status.debug_set_variable_unsupported").to_string();
            return;
        }
        self.prompt = Some(
            Prompt::new(
                PromptKind::DebugSetVariable,
                t!("prompt.debug_set_variable", name = name.as_str()).to_string(),
            )
            .with_input(value),
        );
        self.dap_edit.variable = Some((reference, name));
    }

    /// The set-variable prompt was answered.
    pub(super) fn accept_set_variable(&mut self, value: &str) {
        let Some((reference, name)) = self.dap_edit.variable.take() else {
            return;
        };
        if !value.is_empty() && !self.dap.set_variable(reference, &name, value) {
            self.status = t!("status.debug_set_variable_unsupported").to_string();
        }
    }

    // ----- data breakpoints ---------------------------------------------------

    /// `b` in the panel / Run → Data Breakpoint on Variable: ask the adapter
    /// whether the highlighted variable can be watched (`dataBreakpointInfo`);
    /// the answer toggles the breakpoint
    /// ([`App::data_breakpoint_info_arrived`]).
    pub(super) fn begin_data_breakpoint(&mut self) {
        let Some((reference, name, _)) = self.dap_view.selected_variable() else {
            self.status = t!("status.debug_data_no_variable").to_string();
            return;
        };
        if !self.dap.data_breakpoint_info(reference, &name) {
            self.status = if self.dap.is_active() {
                t!("status.debug_data_breakpoint_unsupported").to_string()
            } else {
                t!("status.debug_data_no_variable").to_string()
            };
        }
    }

    /// The adapter answered `dataBreakpointInfo` for `name`: add a data
    /// breakpoint on the data (writes, when the adapter offers that), or
    /// remove the one already there, or say why it cannot be watched.
    fn data_breakpoint_info_arrived(
        &mut self,
        name: &str,
        data_id: Option<String>,
        description: &str,
        access_types: &[String],
    ) {
        let Some(data_id) = data_id else {
            self.status = t!(
                "status.debug_data_breakpoint_unavailable",
                name = name,
                reason = description
            )
            .to_string();
            return;
        };
        let mut list = self.dap.data_breakpoints().to_vec();
        if let Some(i) = list.iter().position(|b| b.data_id == data_id) {
            list.remove(i);
            self.status = t!("status.debug_data_breakpoint_removed", name = name).to_string();
        } else {
            let mut bp = crate::dap::DataBreakpoint::new(data_id, name);
            bp.access_type = access_types
                .iter()
                .find(|a| a.as_str() == "write")
                .or_else(|| access_types.first())
                .cloned();
            list.push(bp);
            self.status = t!("status.debug_data_breakpoint_added", name = name).to_string();
        }
        if !self.dap.set_data_breakpoints(list) {
            self.status = t!("status.debug_data_breakpoint_unsupported").to_string();
        }
    }

    // ----- instruction breakpoints --------------------------------------------

    /// Run → Instruction Breakpoint…: ask for a memory reference to toggle.
    pub(super) fn begin_instruction_breakpoint(&mut self) {
        // With a session the adapter must support them; with none, they are
        // only kept for the next session (which sends them if it can).
        if self
            .dap
            .capabilities()
            .is_some_and(|c| !c.supports_instruction_breakpoints)
        {
            self.status = t!("status.debug_instruction_breakpoint_unsupported").to_string();
            return;
        }
        self.prompt = Some(Prompt::new(
            PromptKind::DebugInstructionBreakpoint,
            t!("prompt.debug_instruction_breakpoint").to_string(),
        ));
    }

    /// The memory-reference prompt was answered: add the breakpoint, or
    /// remove the one already there (an empty answer does nothing).
    pub(super) fn accept_instruction_breakpoint(&mut self, reference: &str) {
        let reference = reference.trim();
        if reference.is_empty() {
            return;
        }
        let mut list = self.dap.instruction_breakpoints().to_vec();
        let status = if let Some(i) = list.iter().position(|b| b.reference == reference) {
            list.remove(i);
            t!(
                "status.debug_instruction_breakpoint_removed",
                reference = reference
            )
        } else {
            list.push(crate::dap::InstructionBreakpoint::new(reference));
            t!(
                "status.debug_instruction_breakpoint_added",
                reference = reference
            )
        };
        self.status = if self.dap.set_instruction_breakpoints(list) {
            status.to_string()
        } else {
            t!("status.debug_instruction_breakpoint_unsupported").to_string()
        };
    }

    // ----- Edit Breakpoint ----------------------------------------------------

    /// Run → Edit Breakpoint…: start the three prompts for the cursor line's
    /// breakpoint (created if there is none once they are answered).
    pub(super) fn begin_edit_breakpoint(&mut self) {
        let Some((path, line)) = self
            .editor
            .active_tab()
            .and_then(|t| t.path.clone().map(|p| (p, t.editor.cursor_line() + 1)))
        else {
            self.status = t!("status.debug_no_file").to_string();
            return;
        };
        let draft = self
            .breakpoints
            .get(&path)
            .and_then(|m| m.get(&line).cloned())
            .unwrap_or_else(|| SourceBreakpoint::new(line));
        let seed = draft.condition.clone().unwrap_or_default();
        self.dap_edit.breakpoint = Some((path, draft));
        self.prompt = Some(
            Prompt::new(
                PromptKind::DebugBreakpointCondition,
                t!("prompt.debug_breakpoint_condition", line = line).to_string(),
            )
            .with_input(seed),
        );
    }

    /// One Edit Breakpoint prompt was answered: record it and ask the next,
    /// or commit after the last. An empty answer clears the field.
    pub(super) fn accept_breakpoint_prompt(&mut self, kind: PromptKind, input: &str) {
        let Some((path, mut draft)) = self.dap_edit.breakpoint.take() else {
            return;
        };
        let value = (!input.is_empty()).then(|| input.to_string());
        let line = draft.line;
        let next = match kind {
            PromptKind::DebugBreakpointCondition => {
                draft.condition = value;
                Some((
                    PromptKind::DebugBreakpointHit,
                    t!("prompt.debug_breakpoint_hit", line = line),
                    draft.hit_condition.clone(),
                ))
            }
            PromptKind::DebugBreakpointHit => {
                draft.hit_condition = value;
                Some((
                    PromptKind::DebugBreakpointLog,
                    t!("prompt.debug_breakpoint_log", line = line),
                    draft.log_message.clone(),
                ))
            }
            _ => {
                draft.log_message = value;
                None
            }
        };
        if let Some((kind, title, seed)) = next {
            self.prompt =
                Some(Prompt::new(kind, title.to_string()).with_input(seed.unwrap_or_default()));
            self.dap_edit.breakpoint = Some((path, draft));
            return;
        }
        self.breakpoints
            .entry(path.clone())
            .or_default()
            .insert(line, draft);
        self.push_breakpoints(&path);
        self.refresh_debug_markers();
        self.status = t!("status.debug_breakpoint_edited", line = line).to_string();
    }

    // ----- function breakpoints -----------------------------------------------

    /// Run → Function Breakpoint…: ask for a function name to toggle.
    pub(super) fn begin_function_breakpoint(&mut self) {
        self.prompt = Some(Prompt::new(
            PromptKind::DebugFunctionBreakpoint,
            t!("prompt.debug_function_breakpoint").to_string(),
        ));
    }

    /// The function-name prompt was answered: add the breakpoint, or remove
    /// it if that function already has one. An empty answer does nothing.
    pub(super) fn accept_function_breakpoint(&mut self, name: &str) {
        if name.is_empty() {
            return;
        }
        let mut list = self.dap.function_breakpoints().to_vec();
        let message = if let Some(i) = list.iter().position(|b| b.name == name) {
            list.remove(i);
            t!("status.debug_function_breakpoint_removed", name = name)
        } else {
            list.push(crate::dap::FunctionBreakpoint::new(name));
            t!("status.debug_function_breakpoint_added", name = name)
        };
        self.dap.set_function_breakpoints(list);
        self.status = message.to_string();
        if self
            .dap
            .capabilities()
            .is_some_and(|c| !c.supports_function_breakpoints)
        {
            self.status = t!("status.debug_function_breakpoint_unsupported").to_string();
        }
    }

    // ----- exception filters --------------------------------------------------

    /// Run → Exception Breakpoints…: the checklist of the adapter's filters.
    pub(super) fn open_exception_filters(&mut self) {
        self.show_exception_filters(0);
    }

    /// Open (or reopen) the checklist with row `selected` highlighted.
    fn show_exception_filters(&mut self, selected: usize) {
        let filters = self
            .dap
            .capabilities()
            .map(|c| c.exception_filters.clone())
            .unwrap_or_default();
        if filters.is_empty() {
            self.status = t!("status.debug_no_exception_filters").to_string();
            return;
        }
        let on = self.dap.exception_filters();
        let rows = filters
            .iter()
            .map(|f| {
                let mark = if on.contains(&f.filter) { "[x]" } else { "[ ]" };
                format!("{mark} {}", f.label)
            })
            .collect();
        self.choice_list = Some(ChoiceList {
            panel: vix_choice_list::Panel::new(
                t!("ui.debug_exception_filters_title").to_string(),
                t!("ui.debug_exception_filters_hint").to_string(),
                rows,
            )
            .with_selected(selected),
            purpose: ChoicePurpose::ExceptionFilters {
                ids: filters.into_iter().map(|f| f.filter).collect(),
            },
        });
    }

    /// Enter on the checklist: toggle filter `row`, send the new set, and
    /// keep the list open.
    pub(super) fn toggle_exception_filter(&mut self, ids: &[String], row: usize) {
        let Some(id) = ids.get(row) else {
            return;
        };
        let mut on = self.dap.exception_filters();
        if let Some(i) = on.iter().position(|f| f == id) {
            on.remove(i);
        } else {
            on.push(id.clone());
        }
        // Keep the adapter's order, not the order of clicking.
        on.sort_by_key(|f| ids.iter().position(|i| i == f));
        self.dap.set_exception_filters(&on);
        self.show_exception_filters(row);
    }
}
