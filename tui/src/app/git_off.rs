//! The `git` capability's absent half (T593): without the feature Vix never
//! spawns `git`, so there is no status, gutter, blame, branch or log state to
//! maintain, but shared startup, save, key-routing, mouse and drawing code still
//! calls these entry points. Each is a no-op, so call sites carry no `cfg`.
//!
//! `git` is surface-only: the `vix-git` crate stays compiled because the panel
//! data types (`FileStatus`, `LogPanel`, `Change`, …) live there and `App`'s
//! fields use them, but nothing here ever calls into its process-spawning code.

use std::path::Path;

use vix_input::{KeyEvent, MouseEvent};

use super::App;

impl App {
    /// No git state to refresh.
    pub fn refresh_git(&mut self) {
        debug_assert!(self.git_branch.is_none());
    }

    /// No git gutter to refresh.
    pub fn refresh_git_gutter(&mut self) {
        debug_assert!(self.git_branch.is_none());
    }

    /// No inline blame to refresh.
    pub fn refresh_inline_blame(&mut self) {
        debug_assert!(self.git_branch.is_none());
    }

    /// Nothing is ever dirty.
    #[must_use]
    pub fn git_dirty(&self) -> bool {
        false
    }

    /// No file has a git change.
    #[must_use]
    pub fn git_change_for(&self, _path: &Path) -> Option<crate::git::Change> {
        let _ = &self.git_status;
        None
    }

    /// Hunk navigation needs git's diff; without it there are never any hunks.
    pub(super) fn diff_goto(&mut self, _forward: bool) {
        self.status = t!("status.no_changes").to_string();
    }

    /// A git confirm can't be open; drop one defensively.
    pub(super) fn git_confirm_key(&mut self, _kind: super::ConfirmKind, _key: KeyEvent) {
        debug_assert!(self.git_branch.is_none());
        self.confirm = None;
    }

    /// The Git changes panel can't be open.
    pub(super) fn git_panel_key(&mut self, _key: KeyEvent) {
        debug_assert!(self.git_panel.is_none());
    }

    /// The Git changes panel can't be open.
    pub(super) fn git_panel_mouse(&mut self, _mouse: MouseEvent) {
        debug_assert!(self.git_panel.is_none());
    }

    /// The branch chooser can't be open.
    pub(super) fn branch_key(&mut self, _key: KeyEvent) {
        debug_assert!(self.branch_chooser.is_none());
    }

    /// The branch chooser can't be open.
    pub(super) fn branch_mouse(&mut self, _mouse: MouseEvent) {
        debug_assert!(self.branch_chooser.is_none());
    }

    /// The Git log can't be open.
    pub(super) fn git_log_key(&mut self, _key: KeyEvent) {
        debug_assert!(self.git_log.is_none());
    }

    /// The Git log can't be open.
    pub(super) fn git_log_mouse(&mut self, _mouse: MouseEvent) {
        debug_assert!(self.git_log.is_none());
    }

    /// The conflict list can't be open.
    pub(super) fn conflict_list_key(&mut self, _key: KeyEvent) {
        debug_assert!(self.conflict_list.is_none());
    }

    /// The conflict list can't be open.
    pub(super) fn conflict_list_mouse(&mut self, _mouse: MouseEvent) {
        debug_assert!(self.conflict_list.is_none());
    }
}
