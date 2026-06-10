//! Available keyboard navigation styles ("keyways") and the chooser selection
//! state.
//!
//! A *keyway* is a whole-keyboard philosophy for driving the editor, menus, and
//! file explorer. Exactly one is active at a time:
//!
//! - **Apple** — modifier keys trigger system-style actions (e.g. `control-o`).
//! - **Emacs** — layered "chord" sequences run functions (e.g. `control-x-f`).
//! - **Vim** — modal editing, where keys mean different things per mode.
//!
//! This crate is pure data: it lists the keyways and tracks which one is
//! highlighted. The host persists the choice and maps it onto key bindings.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// One selectable keyboard navigation style.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Keyway {
    /// Stable identifier persisted in settings (e.g. `"apple"`).
    pub id: &'static str,
    /// Title shown in the chooser (a proper name; not translated).
    pub name: &'static str,
    /// Short description of the style (e.g. `"Apple controls"`).
    pub tooltip: &'static str,
}

/// All keyways, in chooser order. Apple is first (Vix's default bindings).
pub const KEYWAYS: &[Keyway] = &[
    Keyway { id: "apple", name: "Apple", tooltip: "Apple controls" },
    Keyway { id: "emacs", name: "Emacs", tooltip: "Emacs chords" },
    Keyway { id: "vim", name: "Vim", tooltip: "Vim modes" },
];

/// Selection state for the keyway chooser overlay. The host commits on accept
/// and restores [`Chooser::original`] on cancel.
pub struct Chooser {
    /// Index into [`KEYWAYS`] of the highlighted keyway.
    pub selected: usize,
    /// Index of the keyway active when the chooser opened, restored on cancel.
    pub original: usize,
}

impl Chooser {
    /// Open the chooser highlighting `current_id` (or the first keyway if the id
    /// is not in [`KEYWAYS`]).
    #[must_use]
    pub fn open(current_id: &str) -> Self {
        let selected = KEYWAYS
            .iter()
            .position(|k| k.id == current_id)
            .unwrap_or(0);
        Chooser { selected, original: selected }
    }

    /// Highlight the previous keyway, wrapping around.
    pub fn up(&mut self) {
        self.selected = (self.selected + KEYWAYS.len() - 1) % KEYWAYS.len();
    }

    /// Highlight the next keyway, wrapping around.
    pub fn down(&mut self) {
        self.selected = (self.selected + 1) % KEYWAYS.len();
    }

    /// The highlighted keyway's id.
    #[must_use]
    pub fn selected_id(&self) -> &'static str {
        KEYWAYS[self.selected].id
    }

    /// The id of the keyway active when the chooser opened.
    #[must_use]
    pub fn original_id(&self) -> &'static str {
        KEYWAYS[self.original].id
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
        assert_eq!(c.selected_id(), KEYWAYS[0].id);
        assert_eq!(KEYWAYS[0].id, "apple");
    }

    #[test]
    fn navigation_wraps() {
        let mut c = Chooser::open("apple");
        c.up();
        assert_eq!(c.selected_id(), KEYWAYS[KEYWAYS.len() - 1].id);
    }
}
