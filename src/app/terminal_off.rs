//! The `terminal` capability's absent half (T591): without the feature there is
//! no PTY shell, but shared dispatch, key-routing and the event loop still call
//! these entry points. Each is a no-op, so call sites carry no `cfg`.

use vix_input::KeyEvent;

use super::App;

impl App {
    /// No `tools.terminal` action in this build (also refused up front by
    /// `run_action`'s capability check).
    pub(super) fn run_terminal_action(&mut self, _action: &str) -> bool {
        debug_assert!(self.terminal.is_none());
        false
    }

    /// The terminal can't be open, so it never takes a key.
    pub(super) fn terminal_key(&mut self, _key: KeyEvent) {
        debug_assert!(self.terminal.is_none());
    }

    /// No terminal output to drain.
    pub fn poll_terminal(&mut self) {
        debug_assert!(self.terminal.is_none());
    }

    /// Never running.
    #[must_use]
    pub fn terminal_running(&self) -> bool {
        false
    }
}
