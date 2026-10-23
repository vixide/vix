//! The `http` capability's absent half (T592): no HTTP client, so the shared
//! dispatch and event loop's entry points are no-ops.

use super::App;

impl App {
    /// No `tools.http_send` action in this build (also refused up front by
    /// `run_action`'s capability check).
    pub(super) fn run_http_action(&mut self, _action: &str) -> bool {
        debug_assert!(self.layout.edit_sql.width < u16::MAX);
        false
    }

    /// No response to drain.
    pub fn poll_http(&mut self) {
        debug_assert!(self.layout.edit_sql.width < u16::MAX);
    }

    /// Never running.
    #[must_use]
    pub fn http_running(&self) -> bool {
        false
    }
}
