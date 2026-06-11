//! Data for the keyboard-shortcut help overlay.
//!
//! Each [`Row`] pairs a key combo (shown verbatim, never translated) with an
//! i18n key for its description (translated by the host). Pure data, so this
//! crate has no dependencies and the host owns rendering.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// One help row.
#[derive(Clone, Copy)]
pub struct Row {
    /// Key combo, shown verbatim (e.g. `"Ctrl+P"`).
    pub keys: &'static str,
    /// i18n key for the description (e.g. `"help.command_palette"`).
    pub desc: &'static str,
}

/// All help rows, in display order.
pub const ROWS: &[Row] = &[
    Row { keys: "Ctrl+P", desc: "help.command_palette" },
    Row { keys: "Ctrl+O", desc: "help.open_file" },
    Row { keys: "Ctrl+S / Ctrl+Shift+S", desc: "help.save" },
    Row { keys: "Ctrl+N / Ctrl+W", desc: "help.new_close" },
    Row { keys: "Ctrl+Q", desc: "help.quit" },
    Row { keys: "Ctrl+Z / Ctrl+Shift+Z", desc: "help.undo_redo" },
    Row { keys: "Ctrl+X / Ctrl+C / Ctrl+V", desc: "help.cut_copy_paste" },
    Row { keys: "Ctrl+A", desc: "help.select_all" },
    Row { keys: "Ctrl+F / Ctrl+R", desc: "help.find_replace" },
    Row { keys: "F3 / Shift+F3", desc: "help.find_next_prev" },
    Row { keys: "Ctrl+B / Ctrl+E", desc: "help.toggle_focus_explorer" },
    Row { keys: "Ctrl+Shift+F", desc: "help.search_project" },
    Row { keys: "F12", desc: "help.goto_definition" },
    Row { keys: "Alt+Left / Alt+Right", desc: "help.position_history" },
    Row { keys: "F10 / Alt+F,E,T,H", desc: "help.menu_bar" },
    Row { keys: "F1", desc: "help.this_help" },
    Row { keys: "Mouse", desc: "help.mouse" },
];
