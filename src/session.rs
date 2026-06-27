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
}

/// A restorable editor split (two panes). Mirrors `editor::Split` but with a
/// portable string direction so the session file is stable.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SplitSession {
    /// `"vertical"` (side by side) or `"horizontal"` (stacked).
    pub dir: String,
    /// Tab index shown in the non-focused pane.
    pub other: usize,
    /// Which side is focused: 0 = left/top, 1 = right/bottom.
    pub focused_side: usize,
    /// Percentage width/height of the left/top pane.
    pub ratio: u16,
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
    pub fn set_workspace(&mut self, ws: WorkspaceSession) {
        self.workspaces.retain(|w| w.root != ws.root);
        self.workspaces.insert(0, ws);
        self.workspaces.truncate(MAX_WORKSPACES);
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
