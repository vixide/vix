//! Per-workspace editor session, persisted with [`confy`].
//!
//! The session records the open files, the focused tab, and each tab's cursor
//! position for every workspace root Vix has been used in, so relaunching in the
//! same directory (with no file given on the command line) reopens what was
//! there. It lives next to `Settings` (`vix-settings`) in the config
//! directory as `session.toml`, but is a separate file so it can be cleared
//! without touching preferences.
//!
//! ```
//! use vix_session::{Session, WorkspaceSession};
//!
//! let mut s = Session::default();
//! assert!(s.workspace("/tmp/proj").is_none());
//! s.set_workspace(WorkspaceSession {
//!     root: "/tmp/proj".into(),
//!     files: vec!["/tmp/proj/a.rs".into()],
//!     active: 0,
//!     cursors: vec![12],
//!     ..Default::default()
//! });
//! assert_eq!(s.workspace("/tmp/proj").unwrap().files.len(), 1);
//! ```

use serde::{Deserialize, Serialize};

/// Application name used by [`confy`] to locate the config directory (matches
/// [`Settings`](crate::settings::Settings)).
const APP_NAME: &str = "vix";

/// Config file stem for the session (`session.toml`).
const SESSION_NAME: &str = "session";

/// How many workspaces' sessions to retain (most-recently-saved first). Older
/// ones are dropped so the file does not grow without bound.
const MAX_WORKSPACES: usize = 50;

/// The saved sessions for every workspace Vix has been used in.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    /// One entry per workspace root, most-recently-saved first.
    pub workspaces: Vec<WorkspaceSession>,
    /// Remote workspaces opened with **File → Open Remote…** or `vix
    /// ssh://…` (T713), as canonical `ssh://` names, most recent first and
    /// at most [`MAX_RECENT_REMOTES`]. No secret is ever part of one.
    pub recent_remotes: Vec<String>,
}

/// How many remote names [`Session::recent_remotes`] keeps.
pub const MAX_RECENT_REMOTES: usize = 10;

/// One workspace's restorable editor state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceSession {
    /// Absolute workspace root path this session belongs to.
    pub root: String,
    /// Open file paths (absolute), in tab order.
    pub files: Vec<String>,
    /// Index of the focused tab within `files`.
    pub active: usize,
    /// Cursor character offset per file, parallel to `files`.
    pub cursors: Vec<usize>,
    /// First visible line (vertical scroll) per file, parallel to `files`.
    /// `#[serde(default)]` lets older sessions (without it) still load.
    pub scrolls: Vec<usize>,
    /// Split-pane layout to restore, or `None` for a single pane.
    pub split: Option<SplitSession>,
    /// How many times this workspace has been opened (for frecency ranking).
    /// `#[serde(default)]` lets older sessions load with 0.
    #[serde(default)]
    pub visits: u32,
    /// Unix seconds of the last open (for frecency ranking); 0 if unknown.
    #[serde(default)]
    pub last_visit: i64,

    // ----- project task-runner state (`vix-tasks` wiring) -----------------
    //
    // The private, per-user half of the two-tier persistence model (the
    // shareable half is the host's `.vix/project.toml`, outside this crate).
    // Kept as plain fields rather than a `vix_tasks::lifecycle::
    // LifecycleCommands`/history struct so this crate stays a leaf with no
    // dependency on the `vix-tasks` feature crate; the host converts at
    // the call site. `#[serde(default)]` (redundant with the struct-level
    // attribute, kept for documentation) lets older `session.toml` files
    // without these fields still load.
    /// Cached (previously resolved or user-edited) lifecycle commands,
    /// mirroring `vix_tasks::lifecycle::LifecycleCommands`'s six slots.
    /// Takes precedence over the project-type default and the
    /// `.vix/project.toml` override, so an edited command sticks.
    #[serde(default)]
    pub project_cmd_configure: Option<String>,
    /// See [`WorkspaceSession::project_cmd_configure`].
    #[serde(default)]
    pub project_cmd_compile: Option<String>,
    /// See [`WorkspaceSession::project_cmd_configure`].
    #[serde(default)]
    pub project_cmd_test: Option<String>,
    /// See [`WorkspaceSession::project_cmd_configure`].
    #[serde(default)]
    pub project_cmd_install: Option<String>,
    /// See [`WorkspaceSession::project_cmd_configure`].
    #[serde(default)]
    pub project_cmd_package: Option<String>,
    /// See [`WorkspaceSession::project_cmd_configure`].
    #[serde(default)]
    pub project_cmd_run: Option<String>,
    /// Command run history for the `configure` lifecycle slot, most-recent
    /// last. Each lifecycle slot keeps its own separate history, so cycling
    /// through past commands at one slot's prompt never shows another
    /// slot's commands.
    #[serde(default)]
    pub project_history_configure: Vec<String>,
    /// See [`WorkspaceSession::project_history_configure`].
    #[serde(default)]
    pub project_history_compile: Vec<String>,
    /// See [`WorkspaceSession::project_history_configure`].
    #[serde(default)]
    pub project_history_test: Vec<String>,
    /// See [`WorkspaceSession::project_history_configure`].
    #[serde(default)]
    pub project_history_install: Vec<String>,
    /// See [`WorkspaceSession::project_history_configure`].
    #[serde(default)]
    pub project_history_package: Vec<String>,
    /// See [`WorkspaceSession::project_history_configure`].
    #[serde(default)]
    pub project_history_run: Vec<String>,
    /// The most recently run project command of any kind (a lifecycle
    /// command or a named task), for "repeat last task".
    #[serde(default)]
    pub project_last_command: Option<String>,

    // ----- vix-script workspace trust (T132) -------------------------------
    /// Whether this workspace's `.vix/scripts/` project scripts are trusted
    /// to load: `None` = not yet asked, `Some(true)` = trusted,
    /// `Some(false)` = declined. Global scripts (`Settings::scripts_dir()`)
    /// carry no such flag — they're always trusted, since the user put them
    /// there directly rather than a repo's own author. `#[serde(default)]`
    /// lets an older `session.toml` (predating this field) still load, as
    /// `None` — the safe "not yet asked" state, not silently trusted.
    #[serde(default)]
    pub scripts_trusted: Option<bool>,

    // ----- remote workspace trust (T714) ------------------------------------
    /// Whether this *remote* workspace (its `root` is an `ssh://` name, so
    /// the decision is per host and path) is trusted to run project-defined
    /// code: tasks, run configurations, language servers, the debugger,
    /// Prettier from its `node_modules`. `None` = not yet asked, the safe
    /// default an older `session.toml` loads as; `Some(false)` = declined
    /// (restricted mode). Unused for a local workspace.
    #[serde(default)]
    pub remote_trusted: Option<bool>,

    // ----- Jupyter evaluation trust (T734f) ---------------------------------
    /// Whether Org `src` blocks in this workspace run in a Jupyter kernel
    /// without asking first: `Some(true)` once the user answered "always in
    /// this workspace" to the run confirmation. `None` (the default, and
    /// what an older `session.toml` loads as) keeps asking. It also lets the
    /// workspace's own kernelspecs (`.venv/share/jupyter/kernels`) be used.
    #[serde(default)]
    pub jupyter_trusted: Option<bool>,

    // ----- ACP agent session (T635) -----------------------------------------
    /// The `ai_agents` command line of the last Agent Client Protocol agent
    /// started here. Paired with [`Self::acp_session`]; the command is
    /// re-checked against the current `ai_agents` before it is ever run.
    #[serde(default)]
    pub acp_agent: Option<String>,
    /// That agent's session id, so **AI → Resume Agent** can reload the
    /// conversation after Vix restarts. An id is not a secret and grants
    /// nothing by itself.
    #[serde(default)]
    pub acp_session: Option<String>,

    // ----- debugger breakpoints (T617) -------------------------------------
    /// The line breakpoints set in this workspace, so they survive a restart.
    /// Written on exit from the host's breakpoint table and restored at
    /// startup; an older `session.toml` loads with none.
    #[serde(default)]
    pub breakpoints: Vec<BreakpointRecord>,
    /// The function breakpoints set in this workspace (see
    /// [`Self::breakpoints`]).
    #[serde(default)]
    pub function_breakpoints: Vec<FunctionBreakpointRecord>,
}

/// One saved line breakpoint (T617): a file, a 1-based line and the optional
/// condition, hit condition and log message of the DAP `SourceBreakpoint`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BreakpointRecord {
    /// Absolute path of the file the breakpoint is in.
    pub path: String,
    /// 1-based line.
    pub line: usize,
    /// Condition expression, if any.
    pub condition: Option<String>,
    /// Hit-count condition, if any.
    pub hit_condition: Option<String>,
    /// Log message (makes it a logpoint), if any.
    pub log_message: Option<String>,
}

/// One saved function breakpoint (T617).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FunctionBreakpointRecord {
    /// The function's name, as the adapter resolves it.
    pub name: String,
    /// Condition expression, if any.
    pub condition: Option<String>,
}

/// A restorable split layout: the pane tree plus the focused leaf (in-order).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SplitSession {
    /// The pane tree (leaves index into `files`).
    pub tree: PaneNode,
    /// In-order index of the focused leaf.
    pub focused: usize,
}

/// A serializable mirror of the editor's pane tree. Leaves carry a **file index**
/// (position in [`WorkspaceSession::files`]) so the layout survives across runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PaneNode {
    /// A pane showing the file at this index in `files`.
    Leaf(usize),
    /// A split of two child panes.
    Split {
        /// `"vertical"` or `"horizontal"`.
        dir: String,
        /// Percent for the first child.
        ratio: u16,
        /// First child (left / top).
        first: Box<PaneNode>,
        /// Second child (right / bottom).
        second: Box<PaneNode>,
    },
}

impl Default for PaneNode {
    /// A fresh layout is a single pane showing the first buffer (index 0).
    fn default() -> Self {
        PaneNode::Leaf(0)
    }
}

impl Session {
    /// Load the saved sessions, falling back to an empty set on any error.
    #[must_use]
    pub fn load() -> Session {
        confy::load(APP_NAME, Some(SESSION_NAME)).unwrap_or_default()
    }

    /// Load sessions from an explicit file, falling back to an empty set on
    /// any error (missing file, parse failure, …). Used by tests and
    /// embedders that keep a session file outside the user's config
    /// directory (T132); [`Session::load`] is the normal entry point.
    #[must_use]
    pub fn load_from(path: &std::path::Path) -> Session {
        confy::load_path(path).unwrap_or_default()
    }

    /// Load the saved sessions **without ever writing**: unlike
    /// [`Session::load`] (whose `confy` call creates a default file when none
    /// exists), a missing or unreadable file just yields an empty set. Used by
    /// the read-only `vix --mcp` server (T731).
    #[must_use]
    pub fn load_existing() -> Session {
        confy::get_configuration_file_path(APP_NAME, Some(SESSION_NAME))
            .ok()
            .filter(|path| path.is_file())
            .map(|path| Self::load_existing_from(&path))
            .unwrap_or_default()
    }

    /// [`Session::load_existing`] for an explicit file: parsed directly, never
    /// created, empty on any error.
    #[must_use]
    pub fn load_existing_from(path: &std::path::Path) -> Session {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Persist the sessions to the config directory. `session.toml` can
    /// carry mildly sensitive content (open-file paths reveal project
    /// structure; a cached `project_cmd_*` could embed a secret in a custom
    /// build command), so on Unix it's narrowed to owner-only after each
    /// save (T133) — best-effort, same caveat as `Settings::save`: `confy`
    /// gives no hook to choose the mode as the file is made, only after.
    ///
    /// # Errors
    /// Returns a [`confy::ConfyError`] if the file cannot be written/serialized.
    pub fn save(&self) -> Result<(), confy::ConfyError> {
        confy::store(APP_NAME, Some(SESSION_NAME), self)?;
        if let Ok(path) = confy::get_configuration_file_path(APP_NAME, Some(SESSION_NAME)) {
            vix_fileops::restrict_to_owner(&path);
        }
        Ok(())
    }

    /// Persist sessions to an explicit file (the counterpart of
    /// [`Session::load_from`]); parent directories are created as needed.
    /// Narrowed to owner-only on Unix, same as [`Session::save`].
    ///
    /// # Errors
    /// Returns a [`confy::ConfyError`] if the file cannot be written or
    /// serialized.
    pub fn save_to(&self, path: &std::path::Path) -> Result<(), confy::ConfyError> {
        confy::store_path(path, self)?;
        vix_fileops::restrict_to_owner(path);
        Ok(())
    }

    /// The saved session for `root`, if any.
    #[must_use]
    pub fn workspace(&self, root: &str) -> Option<&WorkspaceSession> {
        self.workspaces.iter().find(|w| w.root == root)
    }

    /// Record `root`'s script-trust decision (T132) — `None` for "not yet
    /// asked", `Some(true)`/`Some(false)` for trusted/declined — creating a
    /// minimal entry if `root` has no saved session yet (the trust prompt
    /// can fire before the first real `set_workspace` save, e.g. a brand
    /// new workspace opened with a file argument). Deliberately **not**
    /// [`Self::set_workspace`]: that also bumps `visits` (this isn't an
    /// "open" event) and this narrow field-set shouldn't disturb anything
    /// else already saved for `root` (open files, project history, …).
    pub fn set_scripts_trusted(&mut self, root: &str, trusted: Option<bool>) {
        if let Some(ws) = self.workspaces.iter_mut().find(|w| w.root == root) {
            ws.scripts_trusted = trusted;
        } else {
            self.workspaces.insert(
                0,
                WorkspaceSession {
                    root: root.to_string(),
                    scripts_trusted: trusted,
                    ..Default::default()
                },
            );
            self.workspaces.truncate(MAX_WORKSPACES);
        }
    }

    /// Record a remote workspace's trust decision (T714): `None` for "not
    /// yet asked", `Some(true)`/`Some(false)` for trusted/declined. Like
    /// [`Self::set_scripts_trusted`], it creates a minimal entry for a root
    /// with no saved session yet and leaves everything else alone.
    pub fn set_remote_trusted(&mut self, root: &str, trusted: Option<bool>) {
        if let Some(ws) = self.workspaces.iter_mut().find(|w| w.root == root) {
            ws.remote_trusted = trusted;
        } else {
            self.workspaces.insert(
                0,
                WorkspaceSession {
                    root: root.to_string(),
                    remote_trusted: trusted,
                    ..Default::default()
                },
            );
            self.workspaces.truncate(MAX_WORKSPACES);
        }
    }

    /// Record the Jupyter evaluation trust decision for `root` (T734f).
    /// Like [`Self::set_scripts_trusted`], it creates a minimal entry for a
    /// root with no saved session yet and leaves everything else alone.
    pub fn set_jupyter_trusted(&mut self, root: &str, trusted: Option<bool>) {
        if let Some(ws) = self.workspaces.iter_mut().find(|w| w.root == root) {
            ws.jupyter_trusted = trusted;
        } else {
            self.workspaces.insert(
                0,
                WorkspaceSession {
                    root: root.to_string(),
                    jupyter_trusted: trusted,
                    ..Default::default()
                },
            );
            self.workspaces.truncate(MAX_WORKSPACES);
        }
    }

    /// Record the last ACP agent `command` and its `session` id for `root`
    /// (T635). Like [`Self::set_scripts_trusted`], it creates a minimal entry
    /// for a root with no saved session yet and leaves everything else alone.
    pub fn set_acp_session(&mut self, root: &str, command: &str, session: &str) {
        let (command, session) = (Some(command.to_string()), Some(session.to_string()));
        if let Some(ws) = self.workspaces.iter_mut().find(|w| w.root == root) {
            ws.acp_agent = command;
            ws.acp_session = session;
        } else {
            self.workspaces.insert(
                0,
                WorkspaceSession {
                    root: root.to_string(),
                    acp_agent: command,
                    acp_session: session,
                    ..Default::default()
                },
            );
            self.workspaces.truncate(MAX_WORKSPACES);
        }
    }

    /// Put `name` (a canonical `ssh://` name) at the front of
    /// [`Session::recent_remotes`], dropping an older copy and anything past
    /// [`MAX_RECENT_REMOTES`].
    pub fn push_recent_remote(&mut self, name: &str) {
        self.recent_remotes.retain(|r| r != name);
        self.recent_remotes.insert(0, name.to_string());
        self.recent_remotes.truncate(MAX_RECENT_REMOTES);
    }

    /// Insert or replace the session for its root, moving it to the front and
    /// capping the total number of retained workspaces.
    ///
    /// The trust decisions (`scripts_trusted`, `remote_trusted`,
    /// `jupyter_trusted`) are not part
    /// of a captured session, so a `ws` that leaves one unset keeps the
    /// prior entry's answer rather than forgetting it.
    pub fn set_workspace(&mut self, mut ws: WorkspaceSession) {
        // Carry the prior visit count forward, incremented — set_workspace is
        // called once per open/save, so this counts opens for frecency ranking.
        // `last_visit` is stamped by the caller via `record_visit` before saving.
        let prior = self
            .workspaces
            .iter()
            .find(|w| w.root == ws.root)
            .map_or(0, |w| w.visits);
        ws.visits = prior.saturating_add(1).max(ws.visits);
        if let Some(old) = self.workspaces.iter().find(|w| w.root == ws.root) {
            ws.scripts_trusted = ws.scripts_trusted.or(old.scripts_trusted);
            ws.remote_trusted = ws.remote_trusted.or(old.remote_trusted);
            ws.jupyter_trusted = ws.jupyter_trusted.or(old.jupyter_trusted);
            if ws.acp_session.is_none() {
                ws.acp_agent.clone_from(&old.acp_agent);
                ws.acp_session.clone_from(&old.acp_session);
            }
        }
        self.workspaces.retain(|w| w.root != ws.root);
        self.workspaces.insert(0, ws);
        self.workspaces.truncate(MAX_WORKSPACES);
    }

    /// Workspace roots ranked by *frecency* (frequency × recency) relative to
    /// `now` (unix seconds), most relevant first. Recent opens outweigh old ones:
    /// a visit within a day counts most, then a week, then older.
    #[must_use]
    pub fn frecency_ordered(&self, now: i64) -> Vec<String> {
        let score = |w: &WorkspaceSession| -> i64 {
            let age = now.saturating_sub(w.last_visit);
            let weight = if w.last_visit == 0 {
                1
            } else if age < 86_400 {
                8
            } else if age < 604_800 {
                4
            } else if age < 2_592_000 {
                2
            } else {
                1
            };
            i64::from(w.visits) * weight
        };
        let mut ranked: Vec<&WorkspaceSession> = self.workspaces.iter().collect();
        ranked.sort_by_key(|w| std::cmp::Reverse(score(w)));
        ranked.into_iter().map(|w| w.root.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(root: &str) -> WorkspaceSession {
        WorkspaceSession {
            root: root.into(),
            ..Default::default()
        }
    }

    #[test]
    fn the_acp_session_survives_a_later_workspace_save_and_a_reload() {
        let mut s = Session::default();
        s.set_acp_session("/a", "agent --acp", "S9");
        s.set_workspace(ws("/a"));
        let saved = s.workspace("/a").unwrap();
        assert_eq!(saved.acp_agent.as_deref(), Some("agent --acp"));
        assert_eq!(saved.acp_session.as_deref(), Some("S9"));
        let text = toml::to_string(&s).unwrap();
        let back: Session = toml::from_str(&text).unwrap();
        assert_eq!(
            back.workspace("/a").unwrap().acp_session.as_deref(),
            Some("S9")
        );
        s.set_acp_session("/a", "agent --acp", "S10");
        assert_eq!(
            s.workspace("/a").unwrap().acp_session.as_deref(),
            Some("S10")
        );
    }

    #[test]
    fn breakpoints_round_trip_and_an_older_file_loads_without_them() {
        let mut w = ws("/a");
        w.breakpoints.push(BreakpointRecord {
            path: "/a/x.py".into(),
            line: 7,
            condition: Some("n > 1".into()),
            ..Default::default()
        });
        w.function_breakpoints.push(FunctionBreakpointRecord {
            name: "main".into(),
            condition: None,
        });
        let mut s = Session::default();
        s.set_workspace(w.clone());
        let back: Session = toml::from_str(&toml::to_string(&s).unwrap()).unwrap();
        let saved = back.workspace("/a").unwrap();
        assert_eq!(saved.breakpoints, w.breakpoints);
        assert_eq!(saved.function_breakpoints, w.function_breakpoints);
        let old: Session = toml::from_str("[[workspaces]]\nroot = \"/b\"\n").unwrap();
        assert_eq!(old.workspace("/b").unwrap().breakpoints, []);
    }

    #[test]
    fn set_workspace_keeps_the_trust_decisions() {
        let mut s = Session::default();
        s.set_scripts_trusted("/a", Some(true));
        s.set_remote_trusted("/a", Some(false));
        s.set_workspace(ws("/a"));
        let saved = s.workspace("/a").unwrap();
        assert_eq!(saved.scripts_trusted, Some(true));
        assert_eq!(saved.remote_trusted, Some(false));
    }

    #[test]
    fn recent_remotes_are_most_recent_first_without_duplicates() {
        let mut s = Session::default();
        for i in 0..12 {
            s.push_recent_remote(&format!("ssh://h{i}/x"));
        }
        s.push_recent_remote("ssh://h5/x");
        assert_eq!(s.recent_remotes.len(), MAX_RECENT_REMOTES);
        assert_eq!(s.recent_remotes[0], "ssh://h5/x");
        assert_eq!(
            s.recent_remotes
                .iter()
                .filter(|r| *r == "ssh://h5/x")
                .count(),
            1
        );
    }

    #[test]
    fn load_existing_from_never_creates_a_file() {
        let dir = std::env::temp_dir().join(format!("vix-session-ro-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let missing = dir.join("session.toml");
        assert!(Session::load_existing_from(&missing).workspaces.is_empty());
        assert!(!missing.exists());
        let mut s = Session::default();
        s.set_workspace(WorkspaceSession {
            root: "/w".into(),
            files: vec!["/w/a.rs".into()],
            ..Default::default()
        });
        std::fs::write(&missing, toml::to_string(&s).unwrap()).unwrap();
        assert_eq!(
            Session::load_existing_from(&missing)
                .workspace("/w")
                .unwrap()
                .files,
            vec!["/w/a.rs".to_string()]
        );
    }

    #[test]
    fn set_workspace_replaces_and_moves_to_front() {
        let mut s = Session::default();
        s.set_workspace(ws("/a"));
        s.set_workspace(ws("/b"));
        // Re-saving /a moves it to the front without duplicating.
        s.set_workspace(WorkspaceSession {
            root: "/a".into(),
            files: vec!["/a/x.rs".into()],
            ..Default::default()
        });
        assert_eq!(s.workspaces.len(), 2);
        assert_eq!(s.workspaces[0].root, "/a");
        assert_eq!(
            s.workspace("/a").unwrap().files,
            vec!["/a/x.rs".to_string()]
        );
    }

    #[test]
    fn jupyter_trust_is_kept_across_workspace_saves() {
        let mut s = Session::default();
        s.set_jupyter_trusted("/j", Some(true));
        assert_eq!(s.workspace("/j").unwrap().jupyter_trusted, Some(true));
        s.set_workspace(WorkspaceSession {
            root: "/j".into(),
            ..Default::default()
        });
        assert_eq!(s.workspace("/j").unwrap().jupyter_trusted, Some(true));
    }

    #[test]
    fn set_scripts_trusted_creates_a_minimal_entry_for_a_new_root() {
        let mut s = Session::default();
        s.set_scripts_trusted("/new", Some(true));
        assert_eq!(s.workspace("/new").unwrap().scripts_trusted, Some(true));
        assert_eq!(
            s.workspace("/new").unwrap().visits,
            0,
            "recording a trust decision is not an \"open\" event"
        );
    }

    #[test]
    fn set_scripts_trusted_updates_an_existing_entry_without_disturbing_it() {
        let mut s = Session::default();
        s.set_workspace(WorkspaceSession {
            root: "/a".into(),
            files: vec!["/a/x.rs".into()],
            ..Default::default()
        });
        s.set_scripts_trusted("/a", Some(false));
        let saved = s.workspace("/a").unwrap();
        assert_eq!(saved.scripts_trusted, Some(false));
        assert_eq!(
            saved.files,
            vec!["/a/x.rs".to_string()],
            "other fields untouched"
        );
        assert_eq!(
            saved.visits, 1,
            "still just the one real open, not bumped again"
        );
    }

    #[test]
    fn frecency_ranks_frequent_and_recent_first() {
        let now = 1_000_000_000i64;
        let mut s = Session::default();
        // /rare: opened once, long ago. /freq: opened several times, recently.
        s.workspaces.push(WorkspaceSession {
            root: "/rare".into(),
            visits: 1,
            last_visit: now - 60 * 86_400, // ~2 months old
            ..Default::default()
        });
        s.workspaces.push(WorkspaceSession {
            root: "/freq".into(),
            visits: 5,
            last_visit: now - 3600, // an hour ago
            ..Default::default()
        });
        let ranked = s.frecency_ordered(now);
        assert_eq!(ranked, vec!["/freq".to_string(), "/rare".to_string()]);
    }

    #[test]
    fn set_workspace_increments_visits() {
        let mut s = Session::default();
        s.set_workspace(ws("/a"));
        s.set_workspace(ws("/a"));
        assert_eq!(
            s.workspace("/a").unwrap().visits,
            2,
            "re-opening counts a visit"
        );
    }

    #[test]
    fn set_workspace_caps_retained_count() {
        let mut s = Session::default();
        for i in 0..(MAX_WORKSPACES + 10) {
            s.set_workspace(ws(&format!("/w{i}")));
        }
        assert_eq!(s.workspaces.len(), MAX_WORKSPACES);
        // The most-recent insert is at the front.
        assert_eq!(s.workspaces[0].root, format!("/w{}", MAX_WORKSPACES + 9));
    }

    #[test]
    fn workspace_missing_is_none() {
        assert!(Session::default().workspace("/nope").is_none());
    }

    #[test]
    fn project_fields_round_trip_through_toml() {
        let mut s = Session::default();
        s.set_workspace(WorkspaceSession {
            root: "/proj".into(),
            project_cmd_compile: Some("cargo build --workspace".into()),
            project_history_compile: vec!["cargo build".into(), "cargo build --workspace".into()],
            project_last_command: Some("cargo build --workspace".into()),
            ..Default::default()
        });
        let toml = toml::to_string(&s).expect("serializes");
        let back: Session = toml::from_str(&toml).expect("deserializes");
        let ws = back.workspace("/proj").expect("round-tripped");
        assert_eq!(
            ws.project_cmd_compile.as_deref(),
            Some("cargo build --workspace")
        );
        assert_eq!(
            ws.project_history_compile,
            vec![
                "cargo build".to_string(),
                "cargo build --workspace".to_string()
            ]
        );
        assert_eq!(
            ws.project_last_command.as_deref(),
            Some("cargo build --workspace")
        );
    }

    #[test]
    fn old_session_toml_without_project_fields_still_loads() {
        // Simulates a `session.toml` written before this feature existed.
        let old_toml = r#"
            [[workspaces]]
            root = "/legacy"
            files = ["/legacy/main.rs"]
            active = 0
        "#;
        let s: Session = toml::from_str(old_toml).expect("old file still parses");
        let ws = s.workspace("/legacy").expect("workspace present");
        assert_eq!(ws.project_cmd_compile, None);
        assert_eq!(ws.project_history_compile, [] as [std::string::String; 0]);
        assert_eq!(ws.project_last_command, None);
    }

    #[test]
    #[cfg(unix)]
    fn save_to_narrows_the_file_to_owner_only() {
        // session.toml can carry mildly sensitive content -- open-file paths
        // reveal project structure, a cached project_cmd_* could embed a
        // secret (T133) -- an explicit-path save must come back owner-only
        // regardless of umask.
        use std::os::unix::fs::PermissionsExt as _;
        let path =
            std::env::temp_dir().join(format!("vix-session-mode-{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&path);
        Session::default().save_to(&path).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        std::fs::remove_file(&path).ok();
    }
}
