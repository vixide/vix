//! The `jupyter` capability's absent half: no kernel code is compiled, so
//! the shared dispatch, event loop, confirmation and status bar entry points
//! are no-ops. The `jupyter.*` actions are refused up front by `run_action`'s
//! capability check.

use vix_input::KeyEvent;

use super::App;

/// No kernels without the capability.
#[derive(Default)]
pub(crate) struct State(());

impl App {
    /// No `jupyter.*` action in this build (also refused up front by
    /// `run_action`'s capability check).
    pub(super) fn run_jupyter_action(&mut self, _action: &str) -> bool {
        let State(()) = &self.jupyter;
        false
    }

    /// Nothing to drain.
    pub fn poll_jupyter(&mut self) {
        let State(()) = &self.jupyter;
    }

    /// Never busy.
    #[must_use]
    pub fn jupyter_busy(&self) -> bool {
        let State(()) = &self.jupyter;
        false
    }

    /// No kernel to show.
    #[must_use]
    pub fn jupyter_segment(&self) -> String {
        let State(()) = &self.jupyter;
        String::new()
    }

    /// Org `C-c C-c` never evaluates a block in this build.
    pub(super) fn jupyter_org_ctrl_c(&mut self) -> bool {
        let State(()) = &self.jupyter;
        false
    }

    /// No Jupyter confirmation can be open.
    pub(super) fn jupyter_confirm_key(&mut self, _key: KeyEvent) -> bool {
        let State(()) = &self.jupyter;
        false
    }
}
