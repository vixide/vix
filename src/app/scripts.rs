//! Scripting: loading/reloading `.rhai` scripts (global always, project
//! only once trusted -- T132), the script trust prompt, the script chooser
//! and its run/invoke/host-state plumbing, and the palette's `!` script
//! search mode.
//!
//! Moved out of `app.rs` verbatim (T141, slice 3); scattered across the
//! file like the git/jj slice, not one contiguous block -- interleaved
//! here with the keybinding-editor code (T204), which stays in `app.rs`
//! for now (not one of this epic's named slices).

#![warn(clippy::pedantic)]

use crossterm::event::{KeyCode, KeyEvent, MouseEvent};

use super::{
    App, PendingScriptPrompt, Prompt, PromptKind, ScriptChooser, ScriptTrustPrompt,
    script_current_line_text,
};
use crate::palette::{self, Action as PAction, Entry};
use crate::settings::Settings;

impl App {
    /// (Re)load every `.rhai` script from the global scripts directory
    /// (`Settings::scripts_dir()`) — always trusted, the user put them there
    /// directly — and, only if this workspace's project scripts are trusted
    /// (T132), this workspace's `.vix/scripts/`. Replaces `self.scripts`
    /// wholesale — a script that was deleted or renamed since the last load
    /// leaves no stale command behind. Called once at startup (`main.rs`,
    /// after `App::new`) and again by `script.reload`. Any script that
    /// fails to read or load is skipped; its error goes to the message
    /// drawer, every other script still loads.
    pub fn load_scripts(&mut self) {
        let global = Settings::scripts_dir();
        let project = self.root.join(".vix").join("scripts");
        let project = self.project_scripts_trusted().then_some(project.as_path());
        let (scripts, errors) =
            vix_script::load_all(&self.script_runtime, global.as_deref(), project);
        self.scripts = scripts;
        for err in errors {
            self.messages.error(
                t!(
                    "msg.script_load_error",
                    stem = err.stem.clone(),
                    message = err.message.clone()
                )
                .to_string(),
            );
        }
    }

    /// `script.reload` (Tools → Scripts → Reload): re-run discovery,
    /// re-resolve key overrides (a reloaded script's `bind_key` requests
    /// may have changed — T104j), and report how many commands ended up
    /// available. Also re-checks this workspace's script trust (T132), so a
    /// prior decline isn't a permanent lockout — a manual reload is itself
    /// the user asking to look again.
    pub(super) fn reload_scripts(&mut self) {
        self.load_scripts();
        self.maybe_reprompt_script_trust();
        self.resolve_key_overrides();
        let count: usize = self.scripts.iter().map(|s| s.commands.len()).sum();
        self.messages
            .info(t!("msg.scripts_reloaded", count = count).to_string());
    }

    /// Whether this workspace's project scripts are trusted to load: the
    /// persisted decision from a prior [`App::accept_script_trust`]/
    /// [`App::decline_script_trust`], or `false` if never yet asked.
    fn project_scripts_trusted(&self) -> bool {
        self.load_session()
            .workspace(&self.session_key())
            .is_some_and(|w| w.scripts_trusted == Some(true))
    }

    /// Queue the trust prompt if this workspace's `.vix/scripts/` has at
    /// least one script and it has never been decided either way — called
    /// once at startup (`main.rs`, right after [`App::load_scripts`]). A
    /// prior explicit decline is deliberately **not** re-asked here (that
    /// would defeat the point of persisting "no" at all, asking again every
    /// single launch) — a private `script.reload`-only path re-checks even
    /// after a decline instead, so a user who changes their mind has a way
    /// to be asked again without every ordinary startup re-asking too. A
    /// no-op when there's nothing to ask about (no project scripts) or the
    /// answer is already on file (either way).
    pub fn maybe_prompt_script_trust(&mut self) {
        let decided = self
            .load_session()
            .workspace(&self.session_key())
            .is_some_and(|w| w.scripts_trusted.is_some());
        if !decided {
            self.queue_script_trust_prompt_if_any();
        }
    }

    /// Re-check trust even after a prior explicit decline — called by
    /// `script.reload`, so a user who changes their mind about an already-
    /// declined workspace has a way to be asked again (just reload)
    /// without every ordinary startup re-asking too.
    fn maybe_reprompt_script_trust(&mut self) {
        if !self.project_scripts_trusted() {
            self.queue_script_trust_prompt_if_any();
        }
    }

    /// Queue the trust prompt if `.vix/scripts/` has at least one script,
    /// unconditionally — the shared tail of [`App::maybe_prompt_script_trust`]/
    /// [`App::maybe_reprompt_script_trust`], which differ only in *when*
    /// they're willing to call this.
    fn queue_script_trust_prompt_if_any(&mut self) {
        let project = self.root.join(".vix").join("scripts");
        let count = vix_script::discover(None, Some(&project)).len();
        if count > 0 {
            self.script_trust = Some(ScriptTrustPrompt { count });
        }
    }

    /// Trust this workspace's project scripts: persist the decision, then
    /// actually load them (they were skipped by every `load_scripts()` call
    /// so far, since [`App::project_scripts_trusted`] was false until now).
    fn accept_script_trust(&mut self) {
        if self.script_trust.take().is_none() {
            return;
        }
        self.set_scripts_trusted(Some(true));
        self.load_scripts();
        self.resolve_key_overrides();
        self.status = t!("status.scripts_trusted").to_string();
    }

    /// Decline this workspace's project scripts: persist the decision (so
    /// startup doesn't ask again every launch) and leave them unloaded.
    fn decline_script_trust(&mut self) {
        if self.script_trust.take().is_none() {
            return;
        }
        self.set_scripts_trusted(Some(false));
        self.status = t!("status.scripts_not_trusted").to_string();
    }

    /// Persist this workspace's script-trust decision to the session store,
    /// without disturbing anything else already saved for it (open files,
    /// project command history, …) or counting as a workspace "open".
    fn set_scripts_trusted(&mut self, trusted: Option<bool>) {
        let key = self.session_key();
        let mut session = self.load_session();
        session.set_scripts_trusted(&key, trusted);
        let _ = self.store_session(&session);
    }

    /// Keys for the script-trust prompt: `y`/`Enter` trusts and loads, `n`/
    /// `Esc` declines.
    pub(super) fn script_trust_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y' | 'Y') | KeyCode::Enter => self.accept_script_trust(),
            KeyCode::Char('n' | 'N') | KeyCode::Esc => self.decline_script_trust(),
            _ => {}
        }
    }

    /// `script.run` (Tools → Scripts → Run…): open a chooser listing every
    /// currently-loaded script command, flattened across all scripts.
    pub(super) fn open_script_chooser(&mut self) {
        let commands: Vec<(String, vix_script::Command)> = self
            .scripts
            .iter()
            .flat_map(|s| s.commands.iter().map(move |c| (s.stem.clone(), c.clone())))
            .collect();
        if commands.is_empty() {
            self.status = t!("status.no_scripts").to_string();
            return;
        }
        self.script_chooser = Some(ScriptChooser {
            commands,
            selected: 0,
        });
    }

    pub(super) fn script_chooser_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(c) = self.script_chooser.as_mut() {
                    let n = c.commands.len();
                    c.selected = (c.selected + n - 1) % n;
                }
            }
            KeyCode::Down => {
                if let Some(c) = self.script_chooser.as_mut() {
                    c.selected = (c.selected + 1) % c.commands.len();
                }
            }
            KeyCode::Enter => self.run_selected_script_command(),
            KeyCode::Esc => self.script_chooser = None,
            _ => {}
        }
    }

    pub(super) fn script_chooser_mouse(&mut self, mouse: MouseEvent) {
        if let Some(idx) = self.chooser_row(mouse)
            && let Some(c) = self.script_chooser.as_mut()
            && idx < c.commands.len()
        {
            c.selected = idx;
            self.run_selected_script_command();
        }
    }

    /// Run the highlighted command from `App::script_chooser` (Tools →
    /// Scripts → Run…).
    fn run_selected_script_command(&mut self) {
        let Some(c) = self.script_chooser.take() else {
            return;
        };
        if let Some((stem, cmd)) = c.commands.get(c.selected) {
            let stem = stem.clone();
            let handler = cmd.handler.clone();
            if let Some(idx) = self.scripts.iter().position(|s| s.stem == stem) {
                self.invoke_script_handler(idx, &handler, Vec::new());
            }
        }
    }

    /// Run a palette-issued `script:<script-stem>:<command-id>` action
    /// (§ API v1 "Registering commands" — this is exactly the id
    /// `App::script_palette_entries` constructs). Reports, rather than
    /// panics, if the script or command no longer exists — the id can be
    /// stale if it came from persisted `command_recents` and the script was
    /// since removed or renamed.
    pub(super) fn run_script_command(&mut self, action: &str) {
        let rest = &action["script:".len()..];
        let found = rest.split_once(':').and_then(|(stem, id)| {
            self.scripts
                .iter()
                .position(|s| s.stem == stem)
                .map(|idx| (idx, id.to_string()))
        });
        let Some((idx, id)) = found else {
            self.messages.error(
                t!(
                    "msg.script_command_unavailable",
                    action = action.to_string()
                )
                .to_string(),
            );
            return;
        };
        let Some(handler) = self.scripts[idx]
            .commands
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.handler.clone())
        else {
            self.messages.error(
                t!(
                    "msg.script_command_unavailable",
                    action = action.to_string()
                )
                .to_string(),
            );
            return;
        };
        self.invoke_script_handler(idx, &handler, Vec::new());
    }

    /// Answer a script's `prompt(message, on_submit)` request
    /// (`PromptKind::Script`): re-invoke `on_submit` with the entered text
    /// as its one argument — a fresh call, not a resumed one (§ "Prompting
    /// for input").
    pub(super) fn accept_script_prompt(&mut self, answer: &str) {
        if let Some(pending) = self.pending_script_prompt.take() {
            self.invoke_script_handler(
                pending.script_index,
                &pending.on_submit,
                vec![vix_script::Dynamic::from(answer.to_string())],
            );
        }
    }

    /// Snapshot the active buffer into a `HostState`, call `handler` in
    /// `self.scripts[script_idx]` through `self.script_runtime`, and apply
    /// whatever the call changed. A runtime error (§ Error handling) still
    /// applies any effects made before it failed, then reports the error —
    /// it never reaches here as a Rust panic.
    fn invoke_script_handler(
        &mut self,
        script_idx: usize,
        handler: &str,
        args: Vec<vix_script::Dynamic>,
    ) {
        if script_idx >= self.scripts.len() {
            return;
        }
        let state = self.build_script_host_state();
        let outcome = self
            .script_runtime
            .invoke(&self.scripts[script_idx], handler, args, state);
        match outcome {
            vix_script::InvokeOutcome::Ran(state) => {
                self.apply_script_host_state(script_idx, state);
            }
            vix_script::InvokeOutcome::Error { message, state } => {
                self.apply_script_host_state(script_idx, state);
                let stem = self.scripts[script_idx].stem.clone();
                self.messages
                    .error(t!("msg.script_error", stem = stem, message = message).to_string());
            }
        }
    }

    /// Build the `HostState` snapshot a script handler sees: the active
    /// buffer's text/selection/current-line/cursor, all in **character
    /// offsets** (`vix-find-panel`'s convention). Everything defaults empty
    /// with no active tab (or an image tab, which has no text buffer) —
    /// scripts still run, they just see nothing to read.
    fn build_script_host_state(&mut self) -> vix_script::HostState {
        let Some(tab) = self.editor.active_tab_mut() else {
            return vix_script::HostState::default();
        };
        if tab.is_image() {
            return vix_script::HostState::default();
        }
        let ed = &mut tab.editor;
        let buffer_text = ed.get_content();
        let selection_text = ed.get_selection_text().unwrap_or_default();
        let cursor_offset = ed.get_cursor();
        let current_line = script_current_line_text(ed);
        vix_script::HostState {
            buffer_text,
            selection_text,
            current_line,
            cursor_offset,
            ..Default::default()
        }
    }

    /// Apply a handler's effects back to the active buffer: `set_buffer_text`
    /// replaces the whole buffer, `set_selection_text` replaces the
    /// selection (or inserts at the cursor with none) via the same
    /// `paste_text` path a real paste uses — one undo step either way.
    /// Blocked, like any other edit, when the buffer is read-only; a bare
    /// `set_cursor_offset` (no text change) still moves the cursor even
    /// then. `message`/`error` and a `prompt` request always apply,
    /// regardless of read-only or of whether there was an active tab at all.
    fn apply_script_host_state(&mut self, script_idx: usize, state: vix_script::HostState) {
        let wants_edit = state.buffer_text_written || state.selection_text_written;
        if wants_edit && self.active_read_only() {
            self.messages
                .error(t!("status.read_only_blocked").to_string());
        } else if let Some(tab) = self.editor.active_tab_mut() {
            if state.buffer_text_written {
                tab.editor.set_content(&state.buffer_text);
            }
            if state.selection_text_written {
                tab.editor.paste_text(&state.selection_text);
            }
            if state.cursor_offset_written {
                tab.editor.set_cursor(state.cursor_offset);
            } else if state.buffer_text_written {
                // set_content doesn't move or clamp the cursor itself.
                let c = tab.editor.get_cursor();
                tab.editor.set_cursor(c);
            }
        }
        if state.cursor_offset_written
            && !wants_edit
            && let Some(tab) = self.editor.active_tab_mut()
        {
            tab.editor.set_cursor(state.cursor_offset);
        }
        for msg in state.messages {
            match msg {
                vix_script::HostMessage::Info(text) => self.messages.info(text),
                vix_script::HostMessage::Error(text) => self.messages.error(text),
            }
        }
        if let Some(p) = state.prompt {
            self.prompt = Some(Prompt::new(PromptKind::Script, p.message));
            self.pending_script_prompt = Some(PendingScriptPrompt {
                script_index: script_idx,
                on_submit: p.on_submit,
            });
        }
    }

    /// Score every script-registered command against `query`, continuing the
    /// numbering after `palette::COMMANDS` so catalog and script commands
    /// never collide as a stable-sort tiebreak. Mirrors the static-command
    /// scoring in [`App::recompute_palette`] exactly (recency-first when
    /// `query` is empty, fuzzy score plus a recency boost otherwise) — see
    /// `crates/vix-script/spec/index.md`'s "Registering commands", which
    /// namespaces each entry `script:<script-stem>:<id>` and shows the
    /// script-authored label verbatim (not routed through `t!`).
    pub(super) fn script_palette_entries(&self, query: &str) -> Vec<(i32, usize, Entry)> {
        let recent_rank = |action: &str| self.command_recents.iter().position(|a| a == action);
        let mut scored = Vec::new();
        let mut idx = palette::COMMANDS.len();
        for script in &self.scripts {
            for cmd in &script.commands {
                let action_id = format!("script:{}:{}", script.stem, cmd.id);
                let entry = Entry {
                    label: format!("> {}", cmd.label),
                    action: PAction::RunCommand(action_id.clone()),
                };
                if query.is_empty() {
                    let key = recent_rank(&action_id)
                        .map_or(1000 + i32::try_from(idx).unwrap_or(0), |r| {
                            i32::try_from(r).unwrap_or(0)
                        });
                    scored.push((-key, idx, entry));
                } else if let Some(score) = palette::fuzzy_score(&cmd.label, query) {
                    let boost = recent_rank(&action_id)
                        .map_or(0, |r| (12 - i32::try_from(r).unwrap_or(12)).max(0));
                    scored.push((score + boost, idx, entry));
                }
                idx += 1;
            }
        }
        scored
    }
}
