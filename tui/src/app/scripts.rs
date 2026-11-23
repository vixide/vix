//! Scripting: loading/reloading `.rhai` scripts (global always, project
//! only once trusted -- T132), the script trust prompt, the script chooser
//! and its run/invoke/host-state plumbing, and the palette's `!` script
//! search mode.
//!
//! Moved out of `app.rs` verbatim (T141, slice 3); scattered across the
//! file like the git/jj slice, not one contiguous block -- interleaved
//! here with the keybinding-editor code (T204), which stays in `app.rs`
//! for now (not one of this epic's named slices).

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

use crate::app::chooser::ChooserSlot;
use crate::geo::Geo;
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
                    stem = err.stem,
                    message = err.message
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
            c.selected = c.selected.min(c.rows.len().minus(1));
        }
    }

    /// Handle a key in the plugin chooser: Up/Down move (wrapping), Enter toggles the plugin's
    /// enabled state, Esc closes.
    pub(super) fn plugin_chooser_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(c) = self.chooser.plugin_mut() {
                    let n = c.rows.len();
                    c.selected = vix_list_state::wrap_up(c.selected, n);
                }
            }
            KeyCode::Down => {
                if let Some(c) = self.chooser.plugin_mut() {
                    c.selected = vix_list_state::wrap_down(c.selected, c.rows.len());
                }
            }
            KeyCode::Enter => self.toggle_selected_plugin(),
            KeyCode::Esc => self.chooser = None,
            _ => {}
        }
    }

    /// Handle a mouse event in the plugin chooser: clicking a row toggles that plugin.
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

    /// Handle a key in the script-command chooser: Up/Down move (wrapping), Enter runs the
    /// command, Esc closes.
    pub(super) fn script_chooser_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(c) = self.chooser.script_mut() {
                    let n = c.commands.len();
                    c.selected = vix_list_state::wrap_up(c.selected, n);
                }
            }
            KeyCode::Down => {
                if let Some(c) = self.chooser.script_mut() {
                    c.selected = vix_list_state::wrap_down(c.selected, c.commands.len());
                }
            }
            KeyCode::Enter => self.run_selected_script_command(),
            KeyCode::Esc => self.chooser = None,
            _ => {}
        }
    }

    /// Handle a mouse event in the script-command chooser: clicking a row runs that command.
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
        let rest = action.get("script:".len()..).unwrap_or("");
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
        let Some(handler) = self.scripts.get(idx).and_then(|script| {
            script
                .commands
                .iter()
                .find(|c| c.id == id)
                .map(|c| c.handler.clone())
        }) else {
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
        let Some(script) = self.scripts.get(script_idx) else {
            return;
        };
        let outcome = self.script_runtime.invoke(script, handler, args, state);
        match outcome {
            vix_script::InvokeOutcome::Ran(state) => {
                self.apply_script_host_state(script_idx, state);
            }
            vix_script::InvokeOutcome::Error { message, state } => {
                self.apply_script_host_state(script_idx, state);
                let stem = self
                    .scripts
                    .get(script_idx)
                    .map_or_default(|s| s.stem.clone());
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
                        .map_or(i32::try_from(idx).unwrap_or(0).plus(1000), |r| {
                            i32::try_from(r).unwrap_or(0)
                        });
                    scored.push((0_i32.minus(key), idx, entry));
                } else if let Some(score) = palette::fuzzy_score(&cmd.label, query) {
                    let boost = recent_rank(&action_id)
                        .map_or(0, |r| 12_i32.minus(i32::try_from(r).unwrap_or(12)).max(0));
                    scored.push((score.plus(boost), idx, entry));
                }
                idx = idx.plus(1);
            }
        }
        scored
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::cov4::*;
    use ratatui::layout::Rect;
    use vix_script::plugins::{LockEntry, Lockfile, PluginState};

    /// An app whose global scripts directory and `plugins.lock` live in a
    /// fresh temp dir; returns it with the scripts dir and lock path.
    fn scripted(tag: &str) -> (App, std::path::PathBuf, std::path::PathBuf) {
        let dir = temp(tag);
        let scripts = dir.join("config/scripts");
        let lock = dir.join("config/plugins.lock");
        std::fs::create_dir_all(&scripts).unwrap();
        let app = app_at(&dir).with_plugin_paths(&scripts, &lock);
        (app, scripts, lock)
    }

    fn write_script(scripts: &std::path::Path, stem: &str, body: &str) {
        std::fs::write(scripts.join(format!("{stem}.rhai")), body).unwrap();
    }

    fn entry(name: &str) -> LockEntry {
        LockEntry {
            name: name.into(),
            url: format!("https://example.invalid/{name}.git"),
            rev: None,
            commit: "0123456789abcdef0123456789abcdef01234567".into(),
            trusted: None,
        }
    }

    /// Lay out plugin `name` with one script command and add it to the lock.
    fn plugin(scripts: &std::path::Path, lock: &std::path::Path, names: &[&str]) {
        let mut plugins = Vec::new();
        for name in names {
            std::fs::create_dir_all(scripts.join(name)).unwrap();
            std::fs::write(
                scripts.join(name).join("p.rhai"),
                format!(
                    "register_command(\"go\", \"Go {name}\", \"on_go\");\nfn on_go() {{ set_buffer_text(\"{name}\"); }}"
                ),
            )
            .unwrap();
            plugins.push(entry(name));
        }
        Lockfile { plugins }.save(lock).unwrap();
    }

    #[test]
    fn global_scripts_load_and_report_their_commands_and_bindings() {
        let (mut app, scripts, _lock) = scripted("scr-load");
        write_script(
            &scripts,
            "greet",
            r#"register_command("hi", "Say Hi", "on_hi");
               bind_key("C-c", "hi");
               fn on_hi() { set_buffer_text("hi"); }"#,
        );
        write_script(&scripts, "broken", "fn (");
        app.load_scripts();
        assert_eq!(app.script_command_count(), 1);
        assert_eq!(app.script_binding_stem("C-c"), Some("greet".to_string()));
        assert_eq!(app.script_binding_stem("C-d"), None);
        let reqs = app.script_key_requests();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].key_token, "C-c");
        assert_eq!(reqs[0].action_id, "script:greet:hi");
        let err = last_message(&app).expect("the broken script is reported");
        assert!(err.contains("broken"), "{err}");
    }

    #[test]
    fn reload_reports_the_command_count_and_drops_deleted_scripts() {
        let (mut app, scripts, _lock) = scripted("scr-reload");
        write_script(
            &scripts,
            "a",
            r#"register_command("x", "X", "h"); fn h() {}"#,
        );
        app.reload_scripts();
        assert_eq!(
            last_message(&app),
            Some(t!("msg.scripts_reloaded", count = 1).to_string())
        );
        std::fs::remove_file(scripts.join("a.rhai")).unwrap();
        app.reload_scripts();
        assert_eq!(app.script_command_count(), 0);
        assert_eq!(
            last_message(&app),
            Some(t!("msg.scripts_reloaded", count = 0).to_string())
        );
        // Relocalizing with no scripts loaded does nothing; with one, reloads.
        app.relocalize_scripts();
        write_script(
            &scripts,
            "b",
            r#"register_command("x", "X", "h"); fn h() {}"#,
        );
        app.relocalize_scripts();
        assert_eq!(app.script_command_count(), 0, "nothing was loaded to redo");
        app.load_scripts();
        write_script(
            &scripts,
            "c",
            r#"register_command("y", "Y", "h"); fn h() {}"#,
        );
        app.relocalize_scripts();
        assert_eq!(app.script_command_count(), 2);
    }

    #[test]
    fn project_script_count_counts_the_workspace_scripts() {
        let (app, _scripts, _lock) = scripted("scr-count");
        assert_eq!(app.project_script_count(), 0);
        let dir = app.root.join(".vix/scripts");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("one.rhai"), "").unwrap();
        std::fs::write(dir.join("two.rhai"), "").unwrap();
        std::fs::write(dir.join("notes.txt"), "").unwrap();
        assert_eq!(app.project_script_count(), 2);
    }

    #[test]
    fn script_chooser_lists_navigates_wraps_runs_and_closes() {
        let (mut app, scripts, _lock) = scripted("scr-chooser");
        app.run_action("script.run");
        assert_eq!(app.status, t!("status.no_scripts").to_string());
        write_script(
            &scripts,
            "s",
            r#"register_command("a", "Alpha", "ha");
               register_command("b", "Beta", "hb");
               fn ha() { set_buffer_text("alpha"); }
               fn hb() { set_buffer_text("beta"); }"#,
        );
        app.load_scripts();
        app.run_action("script.run");
        assert_eq!(app.chooser.script().unwrap().commands.len(), 2);
        app.on_key(code(KeyCode::Up)); // wraps to the last
        assert_eq!(app.chooser.script().unwrap().selected, 1);
        app.on_key(code(KeyCode::Down)); // wraps to the first
        assert_eq!(app.chooser.script().unwrap().selected, 0);
        app.on_key(code(KeyCode::Tab)); // ignored
        assert!(app.chooser.script().is_some());
        app.on_key(code(KeyCode::Down));
        app.on_key(code(KeyCode::Enter));
        assert!(app.chooser.script().is_none(), "running closes the chooser");
        assert_eq!(content(&mut app), "beta");

        app.run_action("script.run");
        app.on_key(code(KeyCode::Esc));
        assert!(app.chooser.script().is_none());
        // A click on a row runs that command; outside the list does nothing.
        app.run_action("script.run");
        app.layout.chooser = Rect::new(10, 5, 40, 6);
        app.on_mouse(click(1, 1));
        assert!(app.chooser.script().is_some());
        app.on_mouse(click(12, 5));
        assert_eq!(content(&mut app), "alpha");
        // Out-of-range row inside the box selects nothing.
        app.run_action("script.run");
        app.on_mouse(click(12, 10));
        assert!(app.chooser.script().is_some());
        // Running with the chooser already gone is a no-op.
        app.chooser = None;
        app.run_selected_script_command();
    }

    #[test]
    fn palette_script_commands_run_and_stale_ids_are_reported() {
        let (mut app, scripts, _lock) = scripted("scr-palette");
        write_script(
            &scripts,
            "s",
            r#"register_command("up", "Make Upper", "h");
               fn h() { set_buffer_text(buffer_text().to_upper()); }"#,
        );
        app.load_scripts();
        buffer_with(&mut app, "abc", 0);
        app.run_script_command("script:s:up");
        assert_eq!(content(&mut app), "ABC");
        for stale in ["script:gone:up", "script:s:nope", "script:s"] {
            app.run_script_command(stale);
            assert_eq!(
                last_message(&app),
                Some(t!("msg.script_command_unavailable", action = stale).to_string())
            );
        }
    }

    #[test]
    fn palette_entries_filter_by_query_and_rank_recent_commands_first() {
        let (mut app, scripts, _lock) = scripted("scr-entries");
        write_script(
            &scripts,
            "s",
            r#"register_command("a", "Alpha Thing", "h");
               register_command("b", "Beta Thing", "h");
               fn h() {}"#,
        );
        app.load_scripts();
        let all = app.script_palette_entries("");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].2.label, "> Alpha Thing");
        assert!(matches!(
            &all[0].2.action,
            PAction::RunCommand(id) if id == "script:s:a"
        ));
        assert!(all[0].0 > all[1].0, "earlier registration ranks higher");
        app.command_recents = vec!["script:s:b".to_string()];
        let recent = app.script_palette_entries("");
        assert!(recent[1].0 > recent[0].0, "the recent one now ranks higher");
        let filtered = app.script_palette_entries("beta");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].2.label, "> Beta Thing");
        let boosted = app.script_palette_entries("thing");
        assert_eq!(boosted.len(), 2);
        assert!(app.script_palette_entries("zzzz").is_empty());
    }

    #[test]
    fn handlers_edit_the_selection_move_the_cursor_and_post_messages() {
        let (mut app, scripts, _lock) = scripted("scr-host");
        write_script(
            &scripts,
            "s",
            r#"register_command("sel", "Sel", "on_sel");
               register_command("cur", "Cur", "on_cur");
               register_command("msg", "Msg", "on_msg");
               register_command("bad", "Bad", "on_bad");
               fn on_sel() { set_selection_text(selection_text().to_upper() + current_line()); }
               fn on_cur() { set_cursor_offset(2); }
               fn on_msg() { message("note"); error("oops"); }
               fn on_bad() { set_buffer_text("partial"); throw "kaboom"; }"#,
        );
        app.load_scripts();
        buffer_with(&mut app, "hello world", 0);
        app.editor
            .active_tab_mut()
            .unwrap()
            .editor
            .set_selection_range(0, 5);
        app.run_script_command("script:s:sel");
        assert_eq!(content(&mut app), "HELLOhello world world");

        buffer_with(&mut app, "abcdef", 5);
        app.run_script_command("script:s:cur");
        assert_eq!(app.editor.active_tab_mut().unwrap().editor.get_cursor(), 2);
        assert_eq!(content(&mut app), "abcdef");

        let before = app.messages.items.len();
        app.run_script_command("script:s:msg");
        let texts: Vec<_> = app.messages.items[before..]
            .iter()
            .map(|m| m.text.clone())
            .collect();
        assert_eq!(texts, vec!["note".to_string(), "oops".to_string()]);

        app.run_script_command("script:s:bad");
        assert_eq!(
            content(&mut app),
            "partial",
            "effects before the error apply"
        );
        assert!(last_message(&app).unwrap().contains("kaboom"));
    }

    #[test]
    fn a_read_only_buffer_blocks_script_edits_but_not_cursor_moves() {
        let (mut app, scripts, _lock) = scripted("scr-ro");
        write_script(
            &scripts,
            "s",
            r#"register_command("w", "W", "on_w");
               register_command("c", "C", "on_c");
               fn on_w() { set_buffer_text("changed"); }
               fn on_c() { set_cursor_offset(1); }"#,
        );
        app.load_scripts();
        buffer_with(&mut app, "stay", 3);
        app.editor.active_tab_mut().unwrap().read_only = true;
        app.run_script_command("script:s:w");
        assert_eq!(content(&mut app), "stay");
        assert_eq!(
            last_message(&app),
            Some(t!("status.read_only_blocked").to_string())
        );
        app.run_script_command("script:s:c");
        assert_eq!(app.editor.active_tab_mut().unwrap().editor.get_cursor(), 1);
    }

    #[test]
    fn a_script_prompt_collects_text_and_calls_the_answer_handler() {
        let (mut app, scripts, _lock) = scripted("scr-prompt");
        write_script(
            &scripts,
            "s",
            r#"register_command("ask", "Ask", "on_ask");
               fn on_ask() { prompt("Name?", "on_answer"); }
               fn on_answer(a) { set_buffer_text("hi " + a); }"#,
        );
        app.load_scripts();
        app.run_script_command("script:s:ask");
        let p = app.prompt.as_ref().expect("prompt opened");
        assert!(matches!(p.kind, PromptKind::Script));
        assert_eq!(p.title, "Name?");
        assert!(app.pending_script_prompt.is_some());
        app.accept_script_prompt("Ada");
        assert_eq!(content(&mut app), "hi Ada");
        assert!(app.pending_script_prompt.is_none());
        // With nothing pending, an answer is ignored.
        app.accept_script_prompt("late");
        assert_eq!(content(&mut app), "hi Ada");
    }

    #[test]
    fn invoking_a_handler_for_an_unknown_script_does_nothing() {
        let (mut app, _scripts, _lock) = scripted("scr-badidx");
        buffer_with(&mut app, "same", 0);
        app.invoke_script_handler(7, "h", Vec::new());
        assert_eq!(content(&mut app), "same");
    }

    #[test]
    fn plugin_chooser_with_no_plugins_says_so_and_a_corrupt_lock_reports() {
        let (mut app, scripts, lock) = scripted("scr-plug-none");
        app.open_plugin_chooser();
        assert_eq!(app.status, t!("status.no_plugins").to_string());
        assert!(app.chooser.plugin().is_none());
        std::fs::write(&lock, "not [valid").unwrap();
        app.open_plugin_chooser();
        assert!(
            last_message(&app)
                .unwrap()
                .starts_with(t!("msg.plugin_failed", error = "").to_string().trim_end())
        );
        // Loading scripts with a corrupt lock reports it too and still loads
        // the global scripts.
        write_script(
            &scripts,
            "g",
            r#"register_command("x", "X", "h"); fn h() {}"#,
        );
        let before = app.messages.items.len();
        app.load_scripts();
        assert!(app.messages.items.len() > before);
        assert_eq!(app.script_command_count(), 1);
    }

    #[test]
    fn plugin_chooser_navigation_wraps_and_toggles_by_key_and_click() {
        let (mut app, scripts, lock) = scripted("scr-plug-nav");
        plugin(&scripts, &lock, &["alpha", "beta"]);
        app.open_plugin_chooser();
        assert_eq!(app.chooser.plugin().unwrap().rows.len(), 2);
        app.on_key(code(KeyCode::Up));
        assert_eq!(app.chooser.plugin().unwrap().selected, 1);
        app.on_key(code(KeyCode::Down));
        assert_eq!(app.chooser.plugin().unwrap().selected, 0);
        app.on_key(code(KeyCode::Tab)); // ignored
        assert!(app.chooser.plugin().is_some());

        app.on_key(code(KeyCode::Enter)); // enable alpha
        assert_eq!(
            app.status,
            t!("status.plugin_enabled", name = "alpha").to_string()
        );
        assert_eq!(
            app.chooser.plugin().unwrap().rows[0].state,
            PluginState::Enabled
        );
        assert_eq!(app.script_command_count(), 1, "the plugin's script loaded");

        app.layout.chooser = Rect::new(0, 0, 40, 5);
        app.on_mouse(click(2, 1)); // enable beta
        assert_eq!(app.chooser.plugin().unwrap().selected, 1);
        assert_eq!(
            app.chooser.plugin().unwrap().rows[1].state,
            PluginState::Enabled
        );
        assert_eq!(app.script_command_count(), 2);
        app.on_mouse(click(2, 0)); // disable alpha again
        assert_eq!(
            app.status,
            t!("status.plugin_disabled", name = "alpha").to_string()
        );
        assert_eq!(app.script_command_count(), 1);
        app.on_mouse(click(2, 4)); // below the rows: nothing happens
        assert_eq!(app.chooser.plugin().unwrap().selected, 0);
        app.on_key(code(KeyCode::Esc));
        assert!(app.chooser.plugin().is_none());
    }

    #[test]
    fn a_missing_plugin_cannot_be_enabled_and_a_changed_one_is_trusted_again() {
        let (mut app, scripts, lock) = scripted("scr-plug-state");
        plugin(&scripts, &lock, &["alpha", "ghost"]);
        std::fs::remove_dir_all(scripts.join("ghost")).unwrap();
        app.open_plugin_chooser();
        assert_eq!(
            app.chooser.plugin().unwrap().rows[1].state,
            PluginState::Missing
        );
        app.on_key(code(KeyCode::Down));
        let before = app.messages.items.len();
        app.on_key(code(KeyCode::Enter));
        assert!(app.messages.items.len() > before);
        assert!(last_message(&app).unwrap().contains("ghost"));
        assert_eq!(app.chooser.plugin().unwrap().selected, 1, "selection kept");

        // Enable alpha, then change its files: it reads as Changed and Enter
        // trusts the new files.
        app.on_key(code(KeyCode::Up));
        app.on_key(code(KeyCode::Enter));
        std::fs::write(scripts.join("alpha/p.rhai"), "// edited\n").unwrap();
        app.open_plugin_chooser();
        assert_eq!(
            app.chooser.plugin().unwrap().rows[0].state,
            PluginState::Changed
        );
        app.on_key(code(KeyCode::Enter));
        assert_eq!(
            app.chooser.plugin().unwrap().rows[0].state,
            PluginState::Enabled
        );
        // Without any selectable row the toggle quietly does nothing.
        app.chooser = None;
        app.toggle_selected_plugin();
    }
}
