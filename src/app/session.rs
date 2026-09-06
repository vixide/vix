//! Session and settings: persisting/loading `App`'s settings and
//! per-workspace session (`with_settings_path`/`with_session_path` for
//! test/embedder isolation), restoring a session on startup, capturing
//! and saving it, project-field bookkeeping, and the workspace chooser.
//!
//! Moved out of `app.rs` verbatim (T141, slice 5); mostly contiguous
//! (unlike git/scripts), plus a few outliers (`ensure_project_session_loaded`/
//! `fill_project_fields`, `open_workspace_chooser` and its
//! key/mouse handlers, `open_settings_file`). `App::new`/`build_core`
//! (the constructor itself) deliberately stayed in `app.rs`.

#![warn(clippy::pedantic)]

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, MouseEvent};

use super::{App, Focus, ProjectHistory, WorkspaceChooser, node_to_pane, pane_to_node};
use crate::explorer::Explorer;
use crate::settings::Settings;

impl App {
    /// Persist settings to [`App::settings_path`] when one is set, else to the
    /// user's config directory. Every in-app settings write goes through here so
    /// a run pointed at another config file never touches the user's.
    pub(super) fn store_settings(&self) -> Result<(), confy::ConfyError> {
        match &self.settings_path {
            Some(path) => self.settings.save_to(path),
            None => self.settings.save(),
        }
    }

    /// Persist this app's settings to `path` instead of the user's config
    /// directory. Builder form of [`App::settings_path`], for tests and
    /// embedders that need an isolated config file.
    #[must_use]
    pub fn with_settings_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.settings_path = Some(path.into());
        self
    }

    /// Load the session from [`App::session_path`] when one is set, else
    /// from the user's config directory. Every in-app session read goes
    /// through here so a run pointed at another session file never touches
    /// the user's (same shape as [`App::store_settings`]).
    pub(super) fn load_session(&self) -> crate::session::Session {
        match &self.session_path {
            Some(path) => crate::session::Session::load_from(path),
            None => crate::session::Session::load(),
        }
    }

    /// Persist `session` to [`App::session_path`] when one is set, else to
    /// the user's config directory.
    pub(super) fn store_session(
        &self,
        session: &crate::session::Session,
    ) -> Result<(), confy::ConfyError> {
        match &self.session_path {
            Some(path) => session.save_to(path),
            None => session.save(),
        }
    }

    /// Persist this app's session to `path` instead of the user's config
    /// directory. Builder form of [`App::session_path`], for tests and
    /// embedders that need an isolated session file (T132: this is what
    /// keeps a script-trust decision made in a test from ever touching the
    /// real developer's `session.toml`).
    #[must_use]
    pub fn with_session_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.session_path = Some(path.into());
        self
    }

    /// A stable string key for the current workspace root (canonicalized when
    /// possible, so symlinked paths map to one session entry).
    pub(super) fn session_key(&self) -> String {
        self.root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone())
            .to_string_lossy()
            .into_owned()
    }

    /// Reopen the previous session for this workspace: its open files, focused
    /// tab, and per-file cursor positions. No-op when session restore is
    /// disabled or there is no saved session for this root. Called by `main`
    /// only when no file was given on the command line.
    pub fn restore_session(&mut self) {
        if !self.settings.restore_session {
            return;
        }
        let key = self.session_key();
        let session = self.load_session();
        let Some(ws) = session.workspace(&key).cloned() else {
            return;
        };
        let opened = self.apply_session(&ws);
        if opened > 0 {
            self.status = t!("status.session_restored", count = opened).to_string();
        }
    }

    /// Reopen the files/cursors/active tab described by `ws`, returning how many
    /// files were actually opened. Pure (no config IO) so it can be tested
    /// directly; the fresh app's blank untitled buffer is dropped once at least
    /// one real file is reopened.
    pub fn apply_session(&mut self, ws: &crate::session::WorkspaceSession) -> usize {
        let had_blank = self.editor.tabs.len() == 1
            && self.editor.tabs[0].path.is_none()
            && !self.editor.tabs[0].dirty
            && self.editor.tabs[0].text().trim().is_empty();

        let mut opened = 0usize;
        for (i, file) in ws.files.iter().enumerate() {
            let path = PathBuf::from(file);
            if !path.is_file() {
                continue;
            }
            self.open_path(&path, false);
            if let Some(t) = self.editor.active_tab_mut() {
                let max = t.editor.code_ref().len_chars();
                let pos = ws.cursors.get(i).copied().unwrap_or(0).min(max);
                t.editor.set_cursor(pos);
                t.editor
                    .set_offset_y(ws.scrolls.get(i).copied().unwrap_or(0));
            }
            opened += 1;
        }

        if opened > 0 {
            if had_blank {
                self.editor.tabs.remove(0);
            }
            self.editor.active = ws.active.min(self.editor.tabs.len().saturating_sub(1));
            // Restore the split only when every file reopened cleanly, so the
            // recorded pane index still lines up with the tab order.
            if had_blank && opened == ws.files.len() {
                self.restore_split(ws);
            }
        }
        opened
    }

    /// Rebuild the editor split recorded in `ws`, if any. Leaves index into the
    /// reopened files, which (after a clean restore) match the tab order.
    fn restore_split(&mut self, ws: &crate::session::WorkspaceSession) {
        let Some(s) = ws.split.as_ref() else { return };
        let count = self.editor.tabs.len();
        let tree = node_to_pane(&s.tree, count);
        if tree.leaf_count() < 2 {
            return;
        }
        self.editor.focused_leaf = s.focused.min(tree.leaf_count() - 1);
        if let Some(tab) = tree.leaf_tab(self.editor.focused_leaf) {
            self.editor.active = tab.min(count.saturating_sub(1));
        }
        self.editor.split_root = Some(tree);
    }

    /// Capture the current open files, focused tab, and cursor positions as a
    /// [`WorkspaceSession`](crate::session::WorkspaceSession). Untitled and image
    /// tabs are skipped. Pure (no config IO) so it can be tested directly.
    #[must_use]
    pub fn workspace_session(&self) -> crate::session::WorkspaceSession {
        let mut files = Vec::new();
        let mut cursors = Vec::new();
        let mut scrolls = Vec::new();
        let mut active = 0;
        // Map each editor tab index to its position in `files` (None for skipped
        // untitled/image tabs), so the split's pane index can be translated.
        let mut tab_to_file: Vec<Option<usize>> = Vec::with_capacity(self.editor.tabs.len());
        for (i, tab) in self.editor.tabs.iter().enumerate() {
            let Some(path) = tab.path.as_ref().filter(|_| !tab.is_image()) else {
                tab_to_file.push(None);
                continue;
            };
            if i == self.editor.active {
                active = files.len();
            }
            tab_to_file.push(Some(files.len()));
            files.push(path.to_string_lossy().into_owned());
            cursors.push(tab.editor.get_cursor());
            scrolls.push(tab.editor.get_offset_y());
        }
        let split = self.editor.split_root.as_ref().and_then(|root| {
            let tree = pane_to_node(root, &tab_to_file)?;
            Some(crate::session::SplitSession {
                tree,
                focused: self.editor.focused_leaf,
            })
        });
        crate::session::WorkspaceSession {
            root: self.session_key(),
            files,
            active,
            cursors,
            scrolls,
            split,
            visits: 0, // filled/incremented by Session::set_workspace
            last_visit: jiff::Zoned::now().timestamp().as_second(),
            // The project.* cache/history fields are filled in by
            // `save_session`, which has the full picture (this run's
            // in-memory copy vs. the prior on-disk save); left at their
            // defaults here.
            ..Default::default()
        }
    }

    /// Capture the current session and persist it to the per-workspace store.
    /// Also carries the project command cache/history forward: unchanged
    /// when this run never loaded it (see
    /// [`App::ensure_project_session_loaded`]), else this run's in-memory
    /// copy.
    pub(super) fn save_session(&self) {
        let mut ws = self.workspace_session();
        let key = self.session_key();
        let mut session = self.load_session();
        if let Some(prior) = session.workspace(&key) {
            Self::carry_forward_project_fields(&mut ws, prior);
        }
        if self.project_session_loaded {
            self.fill_project_fields(&mut ws);
        }
        session.set_workspace(ws);
        let _ = self.store_session(&session);
    }

    /// Load this workspace root's persisted project command cache, history,
    /// and last-run command from the session store, at most once per run.
    /// A no-op once already loaded (whether or not a saved session existed).
    pub(super) fn ensure_project_session_loaded(&mut self) {
        if self.project_session_loaded {
            return;
        }
        self.project_session_loaded = true;
        let key = self.session_key();
        let Some(ws) = self.load_session().workspace(&key).cloned() else {
            return;
        };
        self.project_command_cache = crate::tasks::lifecycle::LifecycleCommands {
            configure: ws.project_cmd_configure,
            compile: ws.project_cmd_compile,
            test: ws.project_cmd_test,
            install: ws.project_cmd_install,
            package: ws.project_cmd_package,
            run: ws.project_cmd_run,
        };
        self.project_history = ProjectHistory {
            configure: ws.project_history_configure,
            compile: ws.project_history_compile,
            test: ws.project_history_test,
            install: ws.project_history_install,
            package: ws.project_history_package,
            run: ws.project_history_run,
        };
        self.project_last_command = ws.project_last_command;
    }

    /// Write this run's in-memory project cache/history/last-command into
    /// `ws`. Only called when `project_session_loaded` — see
    /// [`App::carry_forward_project_fields`].
    fn fill_project_fields(&self, ws: &mut crate::session::WorkspaceSession) {
        ws.project_cmd_configure
            .clone_from(&self.project_command_cache.configure);
        ws.project_cmd_compile
            .clone_from(&self.project_command_cache.compile);
        ws.project_cmd_test
            .clone_from(&self.project_command_cache.test);
        ws.project_cmd_install
            .clone_from(&self.project_command_cache.install);
        ws.project_cmd_package
            .clone_from(&self.project_command_cache.package);
        ws.project_cmd_run
            .clone_from(&self.project_command_cache.run);
        ws.project_history_configure
            .clone_from(&self.project_history.configure);
        ws.project_history_compile
            .clone_from(&self.project_history.compile);
        ws.project_history_test
            .clone_from(&self.project_history.test);
        ws.project_history_install
            .clone_from(&self.project_history.install);
        ws.project_history_package
            .clone_from(&self.project_history.package);
        ws.project_history_run.clone_from(&self.project_history.run);
        ws.project_last_command
            .clone_from(&self.project_last_command);
    }

    /// Open the recent-projects chooser from the saved session, excluding the
    /// current workspace. Reports when there is nowhere else to switch to.
    pub(super) fn open_workspace_chooser(&mut self) {
        let current = self.session_key();
        let now = jiff::Zoned::now().timestamp().as_second();
        let roots: Vec<String> = self
            .load_session()
            .frecency_ordered(now)
            .into_iter()
            .filter(|r| *r != current)
            .collect();
        if roots.is_empty() {
            self.status = t!("status.no_recent_projects").to_string();
            return;
        }
        self.workspace_chooser = Some(WorkspaceChooser { roots, selected: 0 });
    }

    pub(super) fn workspace_chooser_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(c) = self.workspace_chooser.as_mut() {
                    let n = c.roots.len();
                    c.selected = (c.selected + n - 1) % n;
                }
            }
            KeyCode::Down => {
                if let Some(c) = self.workspace_chooser.as_mut() {
                    c.selected = (c.selected + 1) % c.roots.len();
                }
            }
            KeyCode::Enter => self.switch_to_selected_workspace(),
            KeyCode::Esc => self.workspace_chooser = None,
            _ => {}
        }
    }

    pub(super) fn workspace_chooser_mouse(&mut self, mouse: MouseEvent) {
        if let Some(idx) = self.chooser_row(mouse)
            && let Some(c) = self.workspace_chooser.as_mut()
            && idx < c.roots.len()
        {
            c.selected = idx;
            self.switch_to_selected_workspace();
        }
    }

    /// Re-root the app at `new_root`: persist the current session, restart the
    /// LSP, rebuild the explorer, reset the tabs, refresh git, and restore the new
    /// workspace's saved session.
    pub(super) fn switch_workspace(&mut self, new_root: &Path) {
        self.save_session();
        self.lsp.shutdown();
        self.lsp_synced.clear();
        self.root = new_root.to_path_buf();
        self.workspace_folders = vec![new_root.to_path_buf()];
        self.explorer = Explorer::new(new_root.to_path_buf());
        self.lsp = crate::lsp::Lsp::new(
            self.settings.lsp_enabled,
            self.settings.lsp_servers.clone(),
            new_root,
        );
        self.editor.close_all();
        self.refresh_git();
        let key = self.session_key();
        if let Some(ws) = self.load_session().workspace(&key).cloned() {
            self.apply_session(&ws);
        }
        self.focus = Focus::Editor;
        self.status = t!("status.project_switched", path = new_root.display()).to_string();
    }

    /// Open the user's settings file in the editor. Saves the current settings
    /// first so the file exists (and reflects in-app changes) before opening.
    pub(super) fn open_settings_file(&mut self) {
        let Some(path) = Settings::config_path() else {
            self.status = t!("status.settings_no_path").to_string();
            return;
        };
        let _ = self.store_settings();
        self.with_jump(|s| {
            s.open_path(&path, false);
            s.focus = Focus::Editor;
        });
    }
}
