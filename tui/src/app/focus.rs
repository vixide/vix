//! Terminal focus events (T628): the front end reports when the terminal
//! window gains or loses focus (xterm focus reporting, `CSI ? 1004 h`).
//!
//! Gaining focus checks open files for outside changes at once, instead of
//! waiting for the next one-second poll, so a file edited in another program
//! is already reloaded when the user looks back. Losing focus saves every
//! dirty buffer when `auto_save = "focus_lost"`.

use super::App;
use crate::settings::AutoSave;

impl App {
    /// The terminal window gained focus: run the external-change check
    /// (reload clean buffers, warn about dirty ones) now, bypassing its
    /// one-second throttle.
    pub fn on_focus_gained(&mut self) {
        self.last_disk_poll = None;
        self.poll_file_changes();
    }

    /// The terminal window lost focus: with `auto_save = "focus_lost"`, write
    /// every dirty buffer that can be saved in place (the same set Save All
    /// writes, without format-on-save), then return to the tab that was
    /// active; a failed write stops there and leaves its tab active. Any
    /// other `auto_save` mode does nothing here.
    pub fn on_focus_lost(&mut self) {
        if self.settings.editor_behavior.auto_save != AutoSave::FocusLost {
            return;
        }
        let (targets, _skipped) = self.savable_dirty_tabs();
        if targets.is_empty() {
            return;
        }
        let original = self.editor.active;
        for i in targets {
            self.editor.active = i;
            // A failure (or a save conflict's question) stays on that tab,
            // next to its cause, with its own status message — as Save All.
            if !self.write_active_to_disk() {
                return;
            }
        }
        self.editor.active = original;
        self.status = t!("status.auto_saved").to_string();
    }
}
