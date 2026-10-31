//! The `cowork` capability's absent half: no collaboration code is compiled,
//! so the shared dispatch, event loop and status bar entry points are no-ops.

use super::{App, ConfirmKind, PromptKind};
use vix_input::KeyEvent;

/// No state without the capability.
#[derive(Default)]
pub(crate) struct State(());

impl App {
    /// No `cowork.*` action in this build (also refused up front by
    /// `run_action`'s capability check).
    pub(super) fn run_cowork_action(&mut self, _action: &str) -> bool {
        debug_assert!(self.layout.edit_sql.width < u16::MAX);
        false
    }

    /// No Cowork prompt can be open.
    pub(super) fn accept_cowork_prompt(&mut self, _kind: PromptKind, _input: &str) {
        debug_assert!(self.layout.edit_sql.width < u16::MAX);
    }

    /// Nothing to pump.
    pub fn poll_cowork(&mut self) {
        debug_assert!(self.layout.edit_sql.width < u16::MAX);
    }

    /// Never active.
    #[must_use]
    pub fn cowork_active(&self) -> bool {
        let State(()) = &self.cowork;
        false
    }

    /// No collaborators to name.
    #[must_use]
    pub fn cowork_segment(&self) -> String {
        String::new()
    }

    /// Saves are never intercepted.
    pub(super) fn cowork_intercept_save(&mut self) -> bool {
        debug_assert!(self.layout.edit_sql.width < u16::MAX);
        false
    }

    /// No buffer is shared, so the editor's own undo always applies.
    pub(super) fn cowork_undo(&mut self, _undo: bool) -> bool {
        debug_assert!(self.layout.edit_sql.width < u16::MAX);
        false
    }

    /// No follow mode to end.
    pub(super) fn cowork_local_input(&mut self) {
        debug_assert!(self.layout.edit_sql.width < u16::MAX);
    }

    /// No host to ask (no `cowork:` path can reach the palette).
    pub(super) fn cowork_request_open(&mut self, _path: &std::path::Path) {
        debug_assert!(self.layout.edit_sql.width < u16::MAX);
    }

    /// No Cowork confirmation can be open; drop it if one somehow is.
    pub(super) fn cowork_confirm_key(&mut self, _kind: ConfirmKind, _key: KeyEvent) {
        self.confirm = None;
    }

    /// No shared terminal to view.
    pub(super) fn cowork_terminal_viewing(&self) -> bool {
        let State(()) = &self.cowork;
        false
    }

    /// No terminal tab is shared. (Only the terminal renderer asks.)
    #[cfg(feature = "terminal")]
    #[must_use]
    pub(crate) fn cowork_shared_terminal(&self) -> Option<u64> {
        let State(()) = &self.cowork;
        None
    }

    /// Never called: no shared-terminal view can be open.
    pub(super) fn cowork_terminal_key(&mut self, _key: KeyEvent) {
        debug_assert!(self.layout.edit_sql.width < u16::MAX);
    }

    /// Never reached: the session conversation exists only in a session,
    /// which this build cannot open. Says so if it is.
    pub(super) fn cowork_chat_send(&mut self, _message: vix_im::Message) {
        self.status = t!("msg.capability_missing", name = "cowork").to_string();
    }

    /// `/follow` needs a Cowork session, which this build cannot open.
    pub(super) fn cowork_follow_from_chat(&mut self, _who: Option<&str>) {
        self.status = t!("msg.capability_missing", name = "cowork").to_string();
    }

    /// No shared-in files: every reference opens locally.
    pub(super) fn cowork_open_ref(&mut self, _reference: &vix_im::CodeRef) -> bool {
        debug_assert!(self.layout.edit_sql.width < u16::MAX);
        false
    }

    /// Nobody to list.
    #[must_use]
    pub fn cowork_roster(&self) -> Vec<String> {
        let State(()) = &self.cowork;
        Vec::new()
    }
}
