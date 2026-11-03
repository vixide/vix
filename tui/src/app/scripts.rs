//! Scripting: loading/reloading `.rhai` scripts (global always, project
//! only once trusted -- T132), the script trust prompt, the script chooser
//! and its run/invoke/host-state plumbing, and the palette's `!` script
//! search mode.
//!
//! Moved out of `app.rs` verbatim (T141, slice 3); scattered across the
//! file like the git/jj slice, not one contiguous block -- interleaved
//! here with the keybinding-editor code (T204), which stays in `app.rs`
//! for now (not one of this epic's named slices).

use crate::app::chooser::ChooserSlot;
use vix_input::{KeyCode, KeyEvent, MouseEvent};

use super::{
    App, PendingScriptPrompt, Prompt, PromptKind, ScriptChooser, script_current_line_text,
};
use crate::palette::{self, Action as PAction, Entry};

impl App {
    /// How many commands every loaded script registered, in total.
    pub(super) fn script_command_count(&self) -> usize {
        self.scripts.iter().map(|s| s.commands.len()).sum()
    }

    /// Every loaded script's `bind_key` requests, as key-binding overrides.
    pub(super) fn script_key_requests(&self) -> Vec<vix_keybindings::Override> {
        let mut requests = Vec::new();
        for script in &self.scripts {
            for binding in &script.bindings {
                requests.push(vix_keybindings::Override {
                    key_token: binding.key_token.clone(),
                    action_id: format!("script:{}:{}", script.stem, binding.command_id),
                    source: vix_keybindings::Source::Script(script.stem.clone()),
                });
            }
        }
        requests
    }

    /// The stem of the loaded script that bound `key_token`, if any.
    pub(super) fn script_binding_stem(&self, key_token: &str) -> Option<String> {
        self.scripts
            .iter()
            .find(|s| s.bindings.iter().any(|b| b.key_token == key_token))
            .map(|s| s.stem.clone())
    }

    /// (Re)load every `.rhai` script from the global scripts directory
    /// (`Settings::scripts_dir()`) — always trusted, the user put them there
    /// directly — and, only if this workspace's project scripts are trusted
    /// (T132), this workspace's `.vix/scripts/`, plus every enabled plugin
    /// (T637, `<scripts dir>/<name>/*.rhai`). Replaces `self.scripts`
    /// wholesale — a script that was deleted or renamed since the last load
    /// leaves no stale command behind. Called once at startup (`main.rs`,
    /// after `App::new`) and again by `script.reload`. Any script that
    /// fails to read or load is skipped; its error goes to the message
    /// drawer, every other script still loads.
    pub fn load_scripts(&mut self) {
        let paths = self.plugin_paths();
        let global = paths.as_ref().map(|p| p.scripts_dir.clone());
        let project = self.root.join(".vix").join("scripts");
        // A remote workspace's scripts are never loaded: discovery reads this
        // machine's files, and remote project code waits for trust (T714).
        let project = (self.project_scripts_trusted() && self.remote_host().is_none())
            .then_some(project.as_path());
        let mut found = vix_script::discover(global.as_deref(), project);
        // T735: `tr` translates into the UI language, at load and at run.
        self.script_runtime.set_locale(&rust_i18n::locale());
        // Enabled plugins (T637): stems `<plugin>/<file>`, so never shadowed.
        if let Some(paths) = &paths {
            match vix_script::plugins::discover_enabled(paths) {
                Ok(plugin_scripts) => found.extend(plugin_scripts),
                Err(e) => self.messages.error(
                    t!(
                        "msg.plugin_failed",
                        error = crate::plugin_cli::error_text(&e)
                    )
                    .to_string(),
                ),
            }
        }
        let (scripts, errors) = vix_script::load_discovered(&self.script_runtime, found);
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

    /// Use `scripts_dir` (global scripts and installed plugins) and
    /// `lock_path` (`plugins.lock`) instead of the user's config directory —
    /// for tests and embedders, the same shape as [`App::with_session_path`].
    #[must_use]
    pub fn with_plugin_paths(
        mut self,
        scripts_dir: impl Into<std::path::PathBuf>,
        lock_path: impl Into<std::path::PathBuf>,
    ) -> Self {
        self.plugin_paths = Some(vix_script::plugins::PluginPaths {
            scripts_dir: scripts_dir.into(),
            lock_path: lock_path.into(),
        });
        self
    }

    /// Where global scripts, plugins and `plugins.lock` live: the override
    /// from [`App::with_plugin_paths`], else the user's config directory.
    fn plugin_paths(&self) -> Option<vix_script::plugins::PluginPaths> {
        self.plugin_paths
            .clone()
            .or_else(crate::plugin_cli::default_paths)
    }

    /// `script.plugins` (Vix → Plugins…, T637): list every installed plugin
    /// with its state, or say there are none.
    pub(super) fn open_plugin_chooser(&mut self) {
        let Some(paths) = self.plugin_paths() else {
            self.status = t!("status.no_plugins").to_string();
            return;
        };
        let lock = match vix_script::plugins::Lockfile::load(&paths.lock_path) {
            Ok(lock) => lock,
            Err(e) => {
                self.messages.error(
                    t!(
                        "msg.plugin_failed",
                        error = crate::plugin_cli::error_text(&e)
                    )
                    .to_string(),
                );
                return;
            }
        };
        if lock.plugins.is_empty() {
            self.status = t!("status.no_plugins").to_string();
            return;
        }
        let rows = lock
            .plugins
            .iter()
            .map(|e| super::PluginRow {
                name: e.name.clone(),
                commit: e.commit.clone(),
                state: vix_script::plugins::state(&paths, e),
            })
            .collect();
        let selected = self.chooser.plugin().map_or(0, |c| c.selected);
        self.chooser
            .open_plugin(super::PluginChooser { rows, selected });
        if let Some(c) = self.chooser.plugin_mut() {
            c.selected = c.selected.min(c.rows.len() - 1);
        }
    }

    pub(super) fn plugin_chooser_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(c) = self.chooser.plugin_mut() {
                    let n = c.rows.len();
                    c.selected = (c.selected + n - 1) % n;
                }
            }
            KeyCode::Down => {
                if let Some(c) = self.chooser.plugin_mut() {
                    c.selected = (c.selected + 1) % c.rows.len();
                }
            }
            KeyCode::Enter => self.toggle_selected_plugin(),
            KeyCode::Esc => self.chooser = None,
            _ => {}
        }
    }

    pub(super) fn plugin_chooser_mouse(&mut self, mouse: MouseEvent) {
        if let Some(idx) = self.chooser_row(mouse)
            && let Some(c) = self.chooser.plugin_mut()
            && idx < c.rows.len()
        {
            c.selected = idx;
            self.toggle_selected_plugin();
        }
    }

    /// Enable the highlighted plugin (trusting its files as they are now) or
    /// disable it (dropping its trust record), then reload scripts and
    /// refresh the chooser. A missing plugin can't be enabled.
    fn toggle_selected_plugin(&mut self) {
        use vix_script::plugins::PluginState;
        let Some(paths) = self.plugin_paths() else {
            return;
        };
        let Some((name, state)) = self
            .chooser
            .plugin()
            .and_then(|c| c.rows.get(c.selected))
            .map(|r| (r.name.clone(), r.state))
        else {
            return;
        };
        let result = match state {
            PluginState::Enabled => vix_script::plugins::disable(&paths, &name)
                .map(|()| t!("status.plugin_disabled", name = name.clone())),
            PluginState::Disabled | PluginState::Changed => {
                vix_script::plugins::enable(&paths, &name)
                    .map(|()| t!("status.plugin_enabled", name = name.clone()))
            }
            PluginState::Missing => {
                Err(vix_script::plugins::PluginError::NotInstalled(name.clone()))
            }
        };
        match result {
            Ok(status) => {
                self.load_scripts();
                self.resolve_key_overrides();
                self.status = status.to_string();
            }
            Err(e) => self.messages.error(
                t!(
                    "msg.plugin_failed",
                    error = crate::plugin_cli::error_text(&e)
                )
                .to_string(),
            ),
        }
        self.open_plugin_chooser();
    }

    /// The UI language changed (T735): load the scripts again so every label
    /// and string a script builds with `tr` follows it. Nothing to do when
    /// no script is loaded — the next load reads the new locale anyway.
    pub(super) fn relocalize_scripts(&mut self) {
        if !self.scripts.is_empty() {
            self.load_scripts();
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

    /// How many `.rhai` files wait under this workspace's `.vix/scripts/`
    /// (the trust prompt's count, T132).
    pub(super) fn project_script_count(&self) -> usize {
        let project = self.root.join(".vix").join("scripts");
        vix_script::discover(None, Some(&project)).len()
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
        self.chooser.open_script(ScriptChooser {
            commands,
            selected: 0,
        });
    }

    pub(super) fn script_chooser_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(c) = self.chooser.script_mut() {
                    let n = c.commands.len();
                    c.selected = (c.selected + n - 1) % n;
                }
            }
            KeyCode::Down => {
                if let Some(c) = self.chooser.script_mut() {
                    c.selected = (c.selected + 1) % c.commands.len();
                }
            }
            KeyCode::Enter => self.run_selected_script_command(),
            KeyCode::Esc => self.chooser = None,
            _ => {}
        }
    }

    pub(super) fn script_chooser_mouse(&mut self, mouse: MouseEvent) {
        if let Some(idx) = self.chooser_row(mouse)
            && let Some(c) = self.chooser.script_mut()
            && idx < c.commands.len()
        {
            c.selected = idx;
            self.run_selected_script_command();
        }
    }

    /// Run the highlighted command from `App::script_chooser` (Tools →
    /// Scripts → Run…).
    fn run_selected_script_command(&mut self) {
        let Some(c) = self.chooser.take_script() else {
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
