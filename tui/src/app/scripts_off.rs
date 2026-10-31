//! The `script` capability's absent half (T590): without the feature there is no
//! Rhai runtime and no loaded scripts, but shared startup, dispatch, key-routing
//! and palette code still calls these entry points. Each is a no-op, so the call
//! sites carry no `cfg` of their own.

use vix_input::{KeyEvent, MouseEvent};

use super::App;
use crate::palette::Entry;

impl App {
    /// No scripts to load.
    pub fn load_scripts(&mut self) {
        debug_assert!(self.script_chooser.is_none());
    }

    /// No project scripts are ever counted: the trust prompt (`trust.rs`)
    /// still asks for a project settings file (T627).
    pub(super) fn project_script_count(&self) -> usize {
        debug_assert!(self.script_chooser.is_none());
        0
    }

    /// The script chooser can't be open, so it never takes a key.
    pub(super) fn script_chooser_key(&mut self, _key: KeyEvent) {
        debug_assert!(self.script_chooser.is_none());
    }

    /// The plugin chooser can't be open, so it never takes a key.
    pub(super) fn plugin_chooser_key(&mut self, _key: KeyEvent) {
        debug_assert!(self.plugin_chooser.is_none());
    }

    /// The plugin chooser can't be open, so it never takes a click.
    pub(super) fn plugin_chooser_mouse(&mut self, _mouse: MouseEvent) {
        debug_assert!(self.plugin_chooser.is_none());
    }

    /// The script chooser can't be open, so it never takes a click.
    pub(super) fn script_chooser_mouse(&mut self, _mouse: MouseEvent) {
        debug_assert!(self.script_chooser.is_none());
    }

    /// No script prompt can be pending.
    pub(super) fn accept_script_prompt(&mut self, _answer: &str) {
        debug_assert!(self.script_chooser.is_none());
    }

    /// No script commands in the palette.
    pub(super) fn script_palette_entries(&self, _query: &str) -> Vec<(i32, usize, Entry)> {
        debug_assert!(self.script_chooser.is_none());
        Vec::new()
    }

    /// No script commands.
    pub(super) fn script_command_count(&self) -> usize {
        debug_assert!(self.script_chooser.is_none());
        0
    }

    /// No script key bindings.
    pub(super) fn script_key_requests(&self) -> Vec<vix_keybindings::Override> {
        debug_assert!(self.script_chooser.is_none());
        Vec::new()
    }

    /// No script owns any key binding.
    pub(super) fn script_binding_stem(&self, _key_token: &str) -> Option<String> {
        debug_assert!(self.script_chooser.is_none());
        None
    }
}
