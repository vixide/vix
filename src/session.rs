//! Per-workspace editor session, persisted with [`confy`].
//!
//! The session records the open files, the focused tab, and each tab's cursor
//! position for every workspace root Vix has been used in, so relaunching in the
//! same directory (with no file given on the command line) reopens what was
//! there. It lives next to [`Settings`](crate::settings::Settings) in the config
//! directory as `session.toml`, but is a separate file so it can be cleared
//! without touching preferences.
//!
//! ```
//! use vix::session::{Session, WorkspaceSession};
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

#![warn(clippy::pedantic)]

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
}

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

    /// Persist the sessions to the config directory.
    ///
    /// # Errors
    /// Returns a [`confy::ConfyError`] if the file cannot be written/serialized.
    pub fn save(&self) -> Result<(), confy::ConfyError> {
        confy::store(APP_NAME, Some(SESSION_NAME), self)
    }

    /// The saved session for `root`, if any.
    #[must_use]
    pub fn workspace(&self, root: &str) -> Option<&WorkspaceSession> {
        self.workspaces.iter().find(|w| w.root == root)
    }

    /// Insert or replace the session for its root, moving it to the front and
    /// capping the total number of retained workspaces.
    pub fn set_workspace(&mut self, mut ws: WorkspaceSession) {
        // Carry the prior visit count forward, incremented — set_workspace is
        // called once per open/save, so this counts opens for frecency ranking.
        // `last_visit` is stamped by the caller via `record_visit` before saving.
        let prior = self.workspaces.iter().find(|w| w.root == ws.root).map_or(0, |w| w.visits);
        ws.visits = prior.saturating_add(1).max(ws.visits);
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
        WorkspaceSession { root: root.into(), ..Default::default() }
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
        assert_eq!(s.workspace("/a").unwrap().files, vec!["/a/x.rs".to_string()]);
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
        assert_eq!(s.workspace("/a").unwrap().visits, 2, "re-opening counts a visit");
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
}
