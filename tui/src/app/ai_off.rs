//! The `ai` capability's absent half (T592): without the feature there is no
//! assistant, chat panel or diff review, but shared dispatch, key-routing and the
//! event loop still call these entry points. Each is a no-op, so call sites carry
//! no `cfg`.

use vix_input::KeyEvent;

use super::App;

impl App {
    /// No `ai.*` actions in this build (also refused up front by `run_action`'s
    /// capability check).
    pub(super) fn run_ai_action(&mut self, _action: &str) -> bool {
        debug_assert!(self.ai_panel.is_none());
        false
    }

    /// The chat panel can't be open, so it never takes a key.
    pub(super) fn ai_panel_key(&mut self, _key: KeyEvent) {
        debug_assert!(self.ai_panel.is_none());
    }

    /// The diff review can't be open, so it never takes a key.
    pub(super) fn ai_diff_key(&mut self, _key: KeyEvent) {
        debug_assert!(self.ai_panel.is_none());
    }

    /// No AI task to drain.
    pub fn poll_ai_replace(&mut self) {
        debug_assert!(self.ai_replace.is_none());
    }

    /// Never running.
    #[must_use]
    pub fn ai_replace_running(&self) -> bool {
        false
    }

    /// No diff review is ever open.
    #[must_use]
    pub fn ai_diff_review(&self) -> Option<&crate::ai_diff::Review> {
        None
    }
}
