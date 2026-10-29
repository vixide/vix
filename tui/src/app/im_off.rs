//! The `im` capability's absent half: without the feature Vix opens no
//! socket, so there are no IRC connections, but the IM menu still offers the
//! panel, Notes, sharing into Notes, status and history. These stand-ins keep
//! the shared IM code free of `cfg`; the network actions themselves are
//! refused up front by `run_action`'s capability check.

use vix_im_irc::Command;

use super::App;

/// A connection, which this build can never hold.
pub(crate) enum Link {}

impl App {
    /// Never reached (`im.connect` is refused first); says so if it is.
    pub(super) fn im_connect_account(&mut self, _index: usize) {
        self.status = t!("msg.capability_missing", name = "im").to_string();
    }

    /// Never reached (`im.disconnect` is refused first); says so if it is.
    pub(super) fn im_disconnect(&mut self) {
        self.status = t!("msg.capability_missing", name = "im").to_string();
    }

    /// No connections.
    pub(super) fn im_link_names(&self) -> Vec<String> {
        debug_assert!(self.im.links.is_empty());
        Vec::new()
    }

    /// Nothing to send on.
    pub(super) fn im_send(&mut self, _account: &str, _command: Command) -> bool {
        debug_assert!(self.im.links.is_empty());
        false
    }

    /// No events to drain.
    pub(super) fn im_poll_links(&mut self) {
        debug_assert!(self.im.links.is_empty());
    }

    /// Never connected.
    #[must_use]
    pub fn im_connected(&self) -> bool {
        debug_assert!(self.im.links.is_empty());
        false
    }
}
