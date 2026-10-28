//! The `http` capability's absent half for the FHIR client (T684): no network
//! layer, so sending and paging are no-ops (the actions are also refused up
//! front by `run_action`'s capability check).

use super::App;

impl App {
    /// No FHIR requests in this build.
    pub(super) fn fhir_send(&mut self, _input: &str) {
        debug_assert!(self.layout.fhir_bundle.width < u16::MAX);
    }

    /// No paging in this build.
    pub(super) fn fhir_follow_next(&mut self) {
        debug_assert!(self.layout.fhir_bundle.width < u16::MAX);
    }
}
