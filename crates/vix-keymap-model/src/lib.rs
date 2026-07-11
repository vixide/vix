//! The available keyboard navigation styles ("keymaps").
//!
//! A *keymap* is a whole-keyboard philosophy for driving the editor, menus, and
//! file explorer. Exactly one is active at a time:
//!
//! - **Apple** — modifier keys trigger system-style actions (e.g. `control-o`).
//! - **`VSCode` macOS / Windows** — VS Code's signature shortcuts (e.g.
//!   `control-p` Quick Open, `control-shift-p` Command Palette).
//! - **Emacs** — layered "chord" sequences run functions (e.g. `control-x-f`).
//! - **Vi** — modal editing, where keys mean different things per mode.
//! - **Spacemacs** — Vi-style modal editing plus a `Space` leader for menus
//!   (e.g. `SPC f f` find file, `SPC g s` git status).
//!
//! This crate is pure data: it lists the keymaps. The host persists the choice
//! (the View → Keymap submenu is built from [`KEYMAPS`]) and maps it onto key
//! bindings.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// One selectable keyboard navigation style.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Keymap {
    /// Stable identifier persisted in settings (e.g. `"apple"`).
    pub id: &'static str,
    /// Title shown in the menu (a proper name; not translated).
    pub name: &'static str,
    /// Short description of the style (e.g. `"Apple controls"`).
    pub tooltip: &'static str,
}

/// All keymaps, in menu order. Apple is first (Vix's default bindings).
pub const KEYMAPS: &[Keymap] = &[
    Keymap {
        id: "apple",
        name: "Apple",
        tooltip: "Apple controls",
    },
    Keymap {
        id: "vscode-macos",
        name: "VSCode macOS",
        tooltip: "VS Code (macOS) bindings",
    },
    Keymap {
        id: "vscode-windows",
        name: "VSCode Windows",
        tooltip: "VS Code (Windows) bindings",
    },
    Keymap {
        id: "emacs",
        name: "Emacs",
        tooltip: "Emacs chords",
    },
    Keymap {
        id: "vi",
        name: "Vi",
        tooltip: "Vi modes",
    },
    Keymap {
        id: "spacemacs",
        name: "Spacemacs",
        tooltip: "Vi modes + Space leader",
    },
    Keymap {
        id: "intellij-macos",
        name: "IntelliJ macOS",
        tooltip: "IntelliJ (macOS) bindings",
    },
    Keymap {
        id: "intellij-windows",
        name: "IntelliJ Windows",
        tooltip: "IntelliJ (Windows) bindings",
    },
    Keymap {
        id: "eclipse",
        name: "Eclipse",
        tooltip: "Eclipse (Windows) bindings",
    },
    Keymap {
        id: "sublime",
        name: "Sublime Text",
        tooltip: "Sublime Text bindings",
    },
];

/// The keymap with the given `id`, if known.
#[must_use]
pub fn by_id(id: &str) -> Option<&'static Keymap> {
    KEYMAPS.iter().find(|k| k.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_lookups_work() {
        for k in KEYMAPS {
            assert_eq!(by_id(k.id).map(|m| m.id), Some(k.id));
        }
        assert!(by_id("nope").is_none());
        assert_eq!(KEYMAPS[0].id, "apple");
    }
}
