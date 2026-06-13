//! Available keyboard navigation styles ("keymaps") and the chooser selection
//! state.
//!
//! A *keymap* is a whole-keyboard philosophy for driving the editor, menus, and
//! file explorer. Exactly one is active at a time:
//!
//! - **Apple** — modifier keys trigger system-style actions (e.g. `control-o`).
//! - **macOS VSCode** — VS Code's signature shortcuts (e.g. `control-p` Quick
//!   Open, `control-shift-p` Command Palette).
//! - **Emacs** — layered "chord" sequences run functions (e.g. `control-x-f`).
//! - **Vim** — modal editing, where keys mean different things per mode.
//!
//! This crate is pure data: it lists the keymaps and tracks which one is
//! highlighted. The host persists the choice and maps it onto key bindings.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// One selectable keyboard navigation style.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Keymap {
    /// Stable identifier persisted in settings (e.g. `"apple"`).
    pub id: &'static str,
    /// Title shown in the chooser (a proper name; not translated).
    pub name: &'static str,
    /// Short description of the style (e.g. `"Apple controls"`).
    pub tooltip: &'static str,
}

/// All keymaps, in chooser order. Apple is first (Vix's default bindings).
pub const KEYMAPS: &[Keymap] = &[
    Keymap { id: "apple", name: "Apple", tooltip: "Apple controls" },
    Keymap { id: "vscode", name: "macOS VSCode", tooltip: "VS Code (macOS) bindings" },
    Keymap { id: "emacs", name: "Emacs", tooltip: "Emacs chords" },
    Keymap { id: "vim", name: "Vim", tooltip: "Vim modes" },
];

/// Selection state for the keymap chooser overlay. The host commits on accept
/// and restores [`Chooser::original`] on cancel.
pub struct Chooser {
    /// Index into [`KEYMAPS`] of the highlighted keymap.
    pub selected: usize,
    /// Index of the keymap active when the chooser opened, restored on cancel.
    pub original: usize,
}

impl Chooser {
    /// Open the chooser highlighting `current_id` (or the first keymap if the id
    /// is not in [`KEYMAPS`]).
    #[must_use]
    pub fn open(current_id: &str) -> Self {
        let selected = KEYMAPS
            .iter()
            .position(|k| k.id == current_id)
            .unwrap_or(0);
        Chooser { selected, original: selected }
    }

    /// Highlight the previous keymap, wrapping around.
    pub fn up(&mut self) {
        self.selected = (self.selected + KEYMAPS.len() - 1) % KEYMAPS.len();
    }

    /// Highlight the next keymap, wrapping around.
    pub fn down(&mut self) {
        self.selected = (self.selected + 1) % KEYMAPS.len();
    }

    /// The highlighted keymap's id.
    #[must_use]
    pub fn selected_id(&self) -> &'static str {
        KEYMAPS[self.selected].id
    }

    /// The id of the keymap active when the chooser opened.
    #[must_use]
    pub fn original_id(&self) -> &'static str {
        KEYMAPS[self.original].id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_on_current_and_navigates() {
        let mut c = Chooser::open("emacs");
        assert_eq!(c.selected_id(), "emacs");
        assert_eq!(c.original_id(), "emacs");
        c.down();
        assert_eq!(c.selected_id(), "vim");
        c.up();
        assert_eq!(c.selected_id(), "emacs");
    }

    #[test]
    fn unknown_id_defaults_to_first() {
        let c = Chooser::open("zz");
        assert_eq!(c.selected_id(), KEYMAPS[0].id);
        assert_eq!(KEYMAPS[0].id, "apple");
    }

    #[test]
    fn navigation_wraps() {
        let mut c = Chooser::open("apple");
        c.up();
        assert_eq!(c.selected_id(), KEYMAPS[KEYMAPS.len() - 1].id);
    }
}
