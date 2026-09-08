//! A single `(action id -> i18n title key)` catalog for every
//! [`App::run_action`](https://github.com/vixide/vix) id that has no menu
//! leaf of its own.
//!
//! Most action ids are already titled somewhere: by their `vix_menu::Item`
//! leaf (the menu tree doubles as a title catalog for anything the menu bar
//! can dispatch), or by their own `palette::COMMANDS` entry (the command
//! palette's hand-curated `>` list, mostly but not only menu-derived —
//! `nav.goto_workspace_symbol` has no menu leaf but is already there). This
//! crate exists only for the ids that are titled *nowhere*: bare cursor and
//! selection primitives (`cursor_down`, `select_word_left`, …), `Ctrl X`
//! chord-only Emacs actions, per-keymap motion aliases, and a handful of
//! actions a menu leaf was once added *purely* to make findable (T147) —
//! `keybindings.reload` no longer needs that workaround now that this
//! catalog covers it directly.
//!
//! Two hosts read it, each trying their own primary source first so an id
//! titled elsewhere is never duplicated:
//!
//! - **`App::action_title`** (`src/app.rs`, backs F1 help and the
//!   keybinding editor) tries the menu tree, then falls back to
//!   [`title_key`].
//! - **The command palette's `>` mode** (`App::catalog_palette_entries` in
//!   `src/app/command_palette.rs`) appends every `CATALOG` entry whose id
//!   isn't already in `palette::COMMANDS`.
//!
//! `vix_keybindings`' [`shortcuts_for`](https://github.com/vixide/vix)-based
//! UI (none exists yet) can pair its `action_id` results with [`title_key`]
//! the same way.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// One catalog entry: an action id and the i18n key for its title.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Action {
    /// The action id, exactly as `App::run_action` matches it.
    pub id: &'static str,
    /// i18n key for the human title (e.g. `"action.cursor_down"`).
    pub title: &'static str,
}

/// Every action id with no menu leaf, paired with its i18n title key.
pub const CATALOG: &[Action] = &[
    Action {
        id: "add_tab",
        title: "action.add_tab",
    },
    Action {
        id: "backspace",
        title: "action.backspace",
    },
    Action {
        id: "center",
        title: "action.center",
    },
    Action {
        id: "clear_info",
        title: "action.clear_info",
    },
    Action {
        id: "clear_status",
        title: "action.clear_status",
    },
    Action {
        id: "command_mode",
        title: "action.command_mode",
    },
    Action {
        id: "copy",
        title: "action.copy",
    },
    Action {
        id: "copy_line",
        title: "action.copy_line",
    },
    Action {
        id: "cursor_down",
        title: "action.cursor_down",
    },
    Action {
        id: "cursor_end",
        title: "action.cursor_end",
    },
    Action {
        id: "cursor_left",
        title: "action.cursor_left",
    },
    Action {
        id: "cursor_page_down",
        title: "action.cursor_page_down",
    },
    Action {
        id: "cursor_page_up",
        title: "action.cursor_page_up",
    },
    Action {
        id: "cursor_right",
        title: "action.cursor_right",
    },
    Action {
        id: "cursor_start",
        title: "action.cursor_start",
    },
    Action {
        id: "cursor_to_view_bottom",
        title: "action.cursor_to_view_bottom",
    },
    Action {
        id: "cursor_to_view_center",
        title: "action.cursor_to_view_center",
    },
    Action {
        id: "cursor_to_view_top",
        title: "action.cursor_to_view_top",
    },
    Action {
        id: "cursor_up",
        title: "action.cursor_up",
    },
    Action {
        id: "cut",
        title: "action.cut",
    },
    Action {
        id: "cut_line",
        title: "action.cut_line",
    },
    Action {
        id: "cycle_autocomplete_back",
        title: "action.cycle_autocomplete_back",
    },
    Action {
        id: "delete",
        title: "action.delete",
    },
    Action {
        id: "delete_line",
        title: "action.delete_line",
    },
    Action {
        id: "delete_sub_word_left",
        title: "action.delete_sub_word_left",
    },
    Action {
        id: "delete_sub_word_right",
        title: "action.delete_sub_word_right",
    },
    Action {
        id: "delete_word_left",
        title: "action.delete_word_left",
    },
    Action {
        id: "delete_word_right",
        title: "action.delete_word_right",
    },
    Action {
        id: "deselect",
        title: "action.deselect",
    },
    Action {
        id: "diff_next",
        title: "action.diff_next",
    },
    Action {
        id: "diff_previous",
        title: "action.diff_previous",
    },
    Action {
        id: "duplicate",
        title: "action.duplicate",
    },
    Action {
        id: "duplicate_line",
        title: "action.duplicate_line",
    },
    Action {
        id: "edit.keyboard_quit",
        title: "action.edit.keyboard_quit",
    },
    Action {
        id: "end",
        title: "action.end",
    },
    Action {
        id: "end_of_line",
        title: "action.end_of_line",
    },
    Action {
        id: "escape",
        title: "action.escape",
    },
    Action {
        id: "file.open_path",
        title: "action.file.open_path",
    },
    Action {
        id: "find",
        title: "action.find",
    },
    Action {
        id: "find_literal",
        title: "action.find_literal",
    },
    Action {
        id: "find_next",
        title: "action.find_next",
    },
    Action {
        id: "find_previous",
        title: "action.find_previous",
    },
    Action {
        id: "first_split",
        title: "action.first_split",
    },
    Action {
        id: "first_tab",
        title: "action.first_tab",
    },
    Action {
        id: "force_quit",
        title: "action.force_quit",
    },
    Action {
        id: "half_page_down",
        title: "action.half_page_down",
    },
    Action {
        id: "half_page_up",
        title: "action.half_page_up",
    },
    Action {
        id: "hsplit",
        title: "action.hsplit",
    },
    Action {
        id: "indent_line",
        title: "action.indent_line",
    },
    Action {
        id: "indent_selection",
        title: "action.indent_selection",
    },
    Action {
        id: "insert_newline",
        title: "action.insert_newline",
    },
    Action {
        id: "insert_tab",
        title: "action.insert_tab",
    },
    Action {
        id: "join_lines",
        title: "action.join_lines",
    },
    Action {
        id: "jump_line",
        title: "action.jump_line",
    },
    Action {
        id: "jump_to_matching_brace",
        title: "action.jump_to_matching_brace",
    },
    Action {
        id: "last_split",
        title: "action.last_split",
    },
    Action {
        id: "last_tab",
        title: "action.last_tab",
    },
    Action {
        id: "motion.char_left",
        title: "action.motion.char_left",
    },
    Action {
        id: "motion.char_right",
        title: "action.motion.char_right",
    },
    Action {
        id: "motion.delete_forward",
        title: "action.motion.delete_forward",
    },
    Action {
        id: "motion.end",
        title: "action.motion.end",
    },
    Action {
        id: "motion.home",
        title: "action.motion.home",
    },
    Action {
        id: "motion.line_down",
        title: "action.motion.line_down",
    },
    Action {
        id: "motion.line_up",
        title: "action.motion.line_up",
    },
    Action {
        id: "motion.page_down",
        title: "action.motion.page_down",
    },
    Action {
        id: "motion.page_up",
        title: "action.motion.page_up",
    },
    Action {
        id: "move_lines_down",
        title: "action.move_lines_down",
    },
    Action {
        id: "move_lines_up",
        title: "action.move_lines_up",
    },
    Action {
        id: "nav.back",
        title: "action.nav.back",
    },
    Action {
        id: "nav.forward",
        title: "action.nav.forward",
    },
    Action {
        id: "nav.switch_buffer",
        title: "action.nav.switch_buffer",
    },
    Action {
        id: "next_split",
        title: "action.next_split",
    },
    Action {
        id: "next_tab",
        title: "action.next_tab",
    },
    Action {
        id: "none",
        title: "action.none",
    },
    Action {
        id: "open_file",
        title: "action.open_file",
    },
    Action {
        id: "outdent_line",
        title: "action.outdent_line",
    },
    Action {
        id: "outdent_selection",
        title: "action.outdent_selection",
    },
    Action {
        id: "page_down",
        title: "action.page_down",
    },
    Action {
        id: "page_up",
        title: "action.page_up",
    },
    Action {
        id: "paragraph_next",
        title: "action.paragraph_next",
    },
    Action {
        id: "paragraph_previous",
        title: "action.paragraph_previous",
    },
    Action {
        id: "paste",
        title: "action.paste",
    },
    Action {
        id: "paste_primary",
        title: "action.paste_primary",
    },
    Action {
        id: "previous_split",
        title: "action.previous_split",
    },
    Action {
        id: "previous_tab",
        title: "action.previous_tab",
    },
    Action {
        id: "quit",
        title: "action.quit",
    },
    Action {
        id: "quit_all",
        title: "action.quit_all",
    },
    Action {
        id: "redo",
        title: "action.redo",
    },
    Action {
        id: "remove_all_multi_cursors",
        title: "action.remove_all_multi_cursors",
    },
    Action {
        id: "remove_duplicate_lines",
        title: "action.remove_duplicate_lines",
    },
    Action {
        id: "remove_multi_cursor",
        title: "action.remove_multi_cursor",
    },
    Action {
        id: "reset_search",
        title: "action.reset_search",
    },
    Action {
        id: "reverse_lines",
        title: "action.reverse_lines",
    },
    Action {
        id: "save",
        title: "action.save",
    },
    Action {
        id: "save_all",
        title: "action.save_all",
    },
    Action {
        id: "save_as",
        title: "action.save_as",
    },
    Action {
        id: "scroll_down",
        title: "action.scroll_down",
    },
    Action {
        id: "scroll_up",
        title: "action.scroll_up",
    },
    Action {
        id: "select_all",
        title: "action.select_all",
    },
    Action {
        id: "select_down",
        title: "action.select_down",
    },
    Action {
        id: "select_left",
        title: "action.select_left",
    },
    Action {
        id: "select_line",
        title: "action.select_line",
    },
    Action {
        id: "select_page_down",
        title: "action.select_page_down",
    },
    Action {
        id: "select_page_up",
        title: "action.select_page_up",
    },
    Action {
        id: "select_right",
        title: "action.select_right",
    },
    Action {
        id: "select_sub_word_left",
        title: "action.select_sub_word_left",
    },
    Action {
        id: "select_sub_word_right",
        title: "action.select_sub_word_right",
    },
    Action {
        id: "select_to_end",
        title: "action.select_to_end",
    },
    Action {
        id: "select_to_end_of_line",
        title: "action.select_to_end_of_line",
    },
    Action {
        id: "select_to_paragraph_next",
        title: "action.select_to_paragraph_next",
    },
    Action {
        id: "select_to_paragraph_previous",
        title: "action.select_to_paragraph_previous",
    },
    Action {
        id: "select_to_start",
        title: "action.select_to_start",
    },
    Action {
        id: "select_to_start_of_line",
        title: "action.select_to_start_of_line",
    },
    Action {
        id: "select_to_start_of_text",
        title: "action.select_to_start_of_text",
    },
    Action {
        id: "select_to_start_of_text_toggle",
        title: "action.select_to_start_of_text_toggle",
    },
    Action {
        id: "select_up",
        title: "action.select_up",
    },
    Action {
        id: "select_word_left",
        title: "action.select_word_left",
    },
    Action {
        id: "select_word_right",
        title: "action.select_word_right",
    },
    Action {
        id: "shell_mode",
        title: "action.shell_mode",
    },
    Action {
        id: "shuffle",
        title: "action.shuffle",
    },
    Action {
        id: "skip_multi_cursor",
        title: "action.skip_multi_cursor",
    },
    Action {
        id: "skip_multi_cursor_back",
        title: "action.skip_multi_cursor_back",
    },
    Action {
        id: "sort_lines",
        title: "action.sort_lines",
    },
    Action {
        id: "sort_unique",
        title: "action.sort_unique",
    },
    Action {
        id: "spawn_multi_cursor",
        title: "action.spawn_multi_cursor",
    },
    Action {
        id: "spawn_multi_cursor_down",
        title: "action.spawn_multi_cursor_down",
    },
    Action {
        id: "spawn_multi_cursor_select",
        title: "action.spawn_multi_cursor_select",
    },
    Action {
        id: "spawn_multi_cursor_up",
        title: "action.spawn_multi_cursor_up",
    },
    Action {
        id: "spell.suggest",
        title: "action.spell.suggest",
    },
    Action {
        id: "start",
        title: "action.start",
    },
    Action {
        id: "start_of_line",
        title: "action.start_of_line",
    },
    Action {
        id: "start_of_text",
        title: "action.start_of_text",
    },
    Action {
        id: "start_of_text_toggle",
        title: "action.start_of_text_toggle",
    },
    Action {
        id: "sub_word_left",
        title: "action.sub_word_left",
    },
    Action {
        id: "sub_word_right",
        title: "action.sub_word_right",
    },
    Action {
        id: "suspend",
        title: "action.suspend",
    },
    Action {
        id: "toggle_diff_gutter",
        title: "action.toggle_diff_gutter",
    },
    Action {
        id: "toggle_help",
        title: "action.toggle_help",
    },
    Action {
        id: "toggle_key_menu",
        title: "action.toggle_key_menu",
    },
    Action {
        id: "tools.line_numbers",
        title: "action.tools.line_numbers",
    },
    Action {
        id: "trim_trailing_whitespace",
        title: "action.trim_trailing_whitespace",
    },
    Action {
        id: "undo",
        title: "action.undo",
    },
    Action {
        id: "unhighlight_search",
        title: "action.unhighlight_search",
    },
    Action {
        id: "unsplit",
        title: "action.unsplit",
    },
    Action {
        id: "view.explorer",
        title: "action.view.explorer",
    },
    Action {
        id: "view.messages",
        title: "action.view.messages",
    },
    Action {
        id: "view.toggle_explorer_focus",
        title: "action.view.toggle_explorer_focus",
    },
    Action {
        id: "view.toggle_menu",
        title: "action.view.toggle_menu",
    },
    Action {
        id: "vim.append",
        title: "action.vim.append",
    },
    Action {
        id: "vim.append_end",
        title: "action.vim.append_end",
    },
    Action {
        id: "vim.insert",
        title: "action.vim.insert",
    },
    Action {
        id: "vim.insert_line_start",
        title: "action.vim.insert_line_start",
    },
    Action {
        id: "vim.open_above",
        title: "action.vim.open_above",
    },
    Action {
        id: "vim.open_below",
        title: "action.vim.open_below",
    },
    Action {
        id: "vsplit",
        title: "action.vsplit",
    },
    Action {
        id: "word_left",
        title: "action.word_left",
    },
    Action {
        id: "word_right",
        title: "action.word_right",
    },
];

/// The i18n title key for `action_id`, if it's in the catalog.
///
/// Returns `None` for an id titled some other way (typically a
/// `vix_menu::Item` leaf) — this crate only covers the gap, not every
/// action id in the app.
#[must_use]
pub fn title_key(action_id: &str) -> Option<&'static str> {
    CATALOG.iter().find(|a| a.id == action_id).map(|a| a.title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_has_a_non_empty_id_and_title() {
        for a in CATALOG {
            assert!(!a.id.is_empty());
            assert!(!a.title.is_empty());
        }
    }

    #[test]
    fn every_id_is_unique() {
        let mut ids: Vec<&str> = CATALOG.iter().map(|a| a.id).collect();
        let before = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), before, "duplicate action id in CATALOG");
    }

    #[test]
    fn title_key_finds_a_known_entry() {
        assert_eq!(title_key("cursor_down"), Some("action.cursor_down"));
    }

    #[test]
    fn title_key_is_none_for_an_unknown_id() {
        assert_eq!(title_key("no.such.action"), None);
    }
}
