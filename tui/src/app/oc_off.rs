//! The `oc` capability's absent half: without the feature Vix never starts a
//! container engine, but shared dispatch, key-routing and mouse-routing still
//! call these entry points. Each is a no-op, so call sites carry no `cfg`.

use vix_input::{KeyEvent, MouseEvent};

use super::App;

impl App {
    /// No `oc.*` actions in this build (also refused up front by
    /// `run_action`'s capability check).
    pub(super) fn run_oc_action(&mut self, _action: &str) -> bool {
        debug_assert!(self.oc.is_none());
        false
    }

    /// The OC panel can't be open.
    pub(super) fn oc_key(&mut self, _key: KeyEvent) {
        debug_assert!(self.oc.is_none());
    }

    /// The OC panel can't be open.
    pub(super) fn oc_mouse(&mut self, _mouse: MouseEvent) {
        debug_assert!(self.oc.is_none());
    }
}
