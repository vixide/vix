//! Top menu bar with keyboard-navigable dropdowns (one level of submenus).
//!
//! Menu items carry an `action` string that `App::run_action` dispatches; the
//! command palette reuses the very same action names. Display text is stored as
//! an i18n key (see `locales/`) and translated at render time via [`Item::label`]
//! and [`MenuDef::title`], so the bar follows the active locale. An item may
//! instead open a nested submenu (e.g. View → Editor, Edit → Find).

#![warn(clippy::pedantic)]

/// A single dropdown entry: a leaf action, a separator, or a submenu.
pub struct Item {
    /// i18n key for the displayed label (e.g. `"menu.item.file.new"`).
    pub label: &'static str,
    /// Action identifier dispatched by `App::run_action` (e.g. `"file.new"`).
    /// Empty for separators and submenu parents.
    pub action: &'static str,
    /// Keyboard shortcut shown right-aligned; never translated.
    pub shortcut: &'static str,
    /// When set, selecting this item opens a nested submenu instead of running
    /// an action.
    pub submenu: Option<&'static [Item]>,
}

/// Sentinel `action` marking a non-selectable separator row in a dropdown.
pub const SEPARATOR: &str = "menu.separator";

impl Item {
    /// A leaf item that runs `action` when selected.
    const fn leaf(label: &'static str, action: &'static str, shortcut: &'static str) -> Item {
        Item { label, action, shortcut, submenu: None }
    }

    /// An item that opens a nested submenu when selected.
    const fn sub(label: &'static str, items: &'static [Item]) -> Item {
        Item { label, action: "", shortcut: "", submenu: Some(items) }
    }

    /// The label translated into the active locale.
    #[must_use]
    pub fn label(&self) -> String {
        t!(self.label).to_string()
    }

    /// Whether this entry is a separator (a non-selectable divider line).
    #[must_use]
    pub fn is_separator(&self) -> bool {
        self.action == SEPARATOR
    }

    /// Whether this entry opens a nested submenu.
    #[must_use]
    pub fn has_submenu(&self) -> bool {
        self.submenu.is_some()
    }
}

/// A dropdown separator (divider line).
const SEP: Item = Item { label: "", action: SEPARATOR, shortcut: "", submenu: None };

/// A top-level menu and its items.
pub struct MenuDef {
    /// i18n key for the menu name (e.g. `"menu.file"`).
    pub name: &'static str,
    /// The dropdown entries.
    pub items: &'static [Item],
}

impl MenuDef {
    /// The menu name translated into the active locale.
    #[must_use]
    pub fn title(&self) -> String {
        t!(self.name).to_string()
    }
}

const FILE: &[Item] = &[
    Item::leaf("menu.item.file.new", "file.new", "Ctrl N"),
    SEP,
    Item::leaf("menu.item.file.open", "file.open", "Ctrl O"),
    Item::leaf("menu.item.file.open_recent", "file.open_recent", "Ctrl Shift O"),
    Item::leaf("menu.item.file.switch_project", "file.switch_project", ""),
    SEP,
    Item::leaf("menu.item.file.save", "file.save", "Ctrl S"),
    Item::leaf("menu.item.file.save_as", "file.save_as", "Ctrl Shift S"),
    Item::leaf("menu.item.file.rename", "file.rename", ""),
    SEP,
    Item::leaf("menu.item.file.close", "file.close", "Ctrl W"),
    Item::leaf("menu.item.file.close_all", "file.close_all", "Ctrl Shift W"),
    Item::leaf("menu.item.file.reopen_closed", "file.reopen_closed", "Ctrl Shift T"),
];

/// Find-related items, grouped under Edit → Find.
const EDIT_FIND: &[Item] = &[
    Item::leaf("menu.item.edit.find", "edit.find", "Ctrl F"),
    Item::leaf("menu.item.edit.find_next", "edit.find_next", "Ctrl G"),
    Item::leaf("menu.item.edit.find_prev", "edit.find_prev", "Ctrl Shift G"),
    Item::leaf("menu.item.edit.find_selection", "search.next_selection", "Alt N"),
    Item::leaf("menu.item.edit.toggle_highlight", "toggle_highlight_search", ""),
    SEP,
    Item::leaf("menu.item.edit.find_in_files", "search.workspace", "Ctrl Shift F"),
    Item::leaf("menu.item.edit.replace_in_files", "search.workspace_replace", ""),
    Item::leaf("menu.item.edit.search_workspace_dock", "search.workspace_dock", ""),
];

const EDIT: &[Item] = &[
    Item::leaf("menu.item.edit.undo", "edit.undo", "Ctrl Z"),
    Item::leaf("menu.item.edit.redo", "edit.redo", "Ctrl Shift Z"),
    SEP,
    Item::leaf("menu.item.edit.cut", "edit.cut", "Ctrl X"),
    Item::leaf("menu.item.edit.copy", "edit.copy", "Ctrl C"),
    Item::leaf("menu.item.edit.paste", "edit.paste", "Ctrl V"),
    SEP,
    Item::sub("menu.item.edit.select_menu", EDIT_SELECT),
    Item::sub("menu.item.edit.lines_menu", EDIT_MOVE),
    Item::sub("menu.item.edit.go_menu", EDIT_GO),
    Item::sub("menu.item.edit.find_menu", EDIT_FIND),
    Item::sub("menu.item.edit.case", EDIT_CASE),
    Item::sub("menu.item.edit.mode", EDIT_MODE),
    SEP,
    Item::leaf("menu.item.edit.toggle_comment", "edit.toggle_comment", "Ctrl /"),
    SEP,
    Item::leaf("menu.item.edit.record_macro", "toggle_macro", ""),
    Item::leaf("menu.item.edit.play_macro", "play_macro", ""),
    Item::leaf("menu.item.edit.save_macro", "macro.save", ""),
    Item::leaf("menu.item.edit.play_saved_macro", "macro.play_saved", ""),
];

/// Cursor jump commands, grouped under Edit → Go.
const EDIT_GO: &[Item] = &[
    Item::leaf("menu.item.edit.recent_locations", "nav.recent_locations", ""),
    Item::leaf("menu.item.edit.go_line", "nav.goto_line", ""),
    SEP,
    Item::leaf("menu.item.edit.bookmark_toggle", "bookmark.toggle", ""),
    Item::leaf("menu.item.edit.bookmark_next", "bookmark.next", ""),
    Item::leaf("menu.item.edit.bookmark_prev", "bookmark.prev", ""),
    Item::leaf("menu.item.edit.bookmark_list", "bookmark.list", ""),
    SEP,
    Item::leaf("menu.item.edit.line_start", "edit.line_start", ""),
    Item::leaf("menu.item.edit.line_end", "edit.line_end", ""),
    SEP,
    Item::leaf("menu.item.edit.para_start", "edit.para_start", ""),
    Item::leaf("menu.item.edit.para_end", "edit.para_end", ""),
    SEP,
    Item::leaf("menu.item.edit.section_start", "edit.section_start", ""),
    Item::leaf("menu.item.edit.section_end", "edit.section_end", ""),
    SEP,
    Item::leaf("menu.item.edit.go_first", "edit.go_first", ""),
    Item::leaf("menu.item.edit.go_last", "edit.go_last", ""),
];

/// Line-move commands, grouped under Edit → Move.
/// Line operations, grouped under Edit → Lines.
const EDIT_MOVE: &[Item] = &[
    Item::leaf("menu.item.edit.move_up", "edit.move_line_up", "Alt ↑"),
    Item::leaf("menu.item.edit.move_down", "edit.move_line_down", "Alt ↓"),
    SEP,
    Item::leaf("menu.item.edit.duplicate", "edit.duplicate_line", ""),
    Item::leaf("menu.item.edit.join", "edit.join_lines", ""),
    SEP,
    Item::leaf("menu.item.edit.sort", "edit.sort_lines", ""),
    Item::leaf("menu.item.edit.sort_unique", "edit.sort_unique", ""),
    Item::leaf("menu.item.edit.reverse", "edit.reverse_lines", ""),
    Item::leaf("menu.item.edit.dedupe", "edit.remove_duplicate_lines", ""),
    Item::leaf("menu.item.edit.trim", "edit.trim_trailing_whitespace", ""),
];

/// Selection commands, grouped under Edit → Select.
const EDIT_SELECT: &[Item] = &[
    Item::leaf("menu.item.edit.select_more", "edit.select_more", "Ctrl Shift →"),
    Item::leaf("menu.item.edit.select_less", "edit.select_less", "Ctrl Shift ←"),
    SEP,
    Item::leaf("menu.item.edit.select_line", "edit.select_line", ""),
    Item::leaf("menu.item.edit.select_paragraph", "edit.select_paragraph", ""),
    Item::leaf("menu.item.edit.select_section", "edit.select_section", ""),
    Item::leaf("menu.item.edit.select_all_occurrences", "edit.select_all_occurrences", ""),
    Item::leaf("menu.item.edit.column_select_down", "edit.column_select_down", "Alt Shift ↓"),
    Item::leaf("menu.item.edit.column_select_up", "edit.column_select_up", "Alt Shift ↑"),
    Item::leaf("menu.item.edit.select_all", "edit.select_all", "Ctrl A"),
];

/// Type-specific edit surfaces, grouped under Edit → Mode. Each opens the active
/// buffer in a structured editor overlay.
const EDIT_MODE: &[Item] = &[
    Item::leaf("menu.item.edit.mode.table", "tools.edit_table", ""),
    Item::leaf("menu.item.edit.mode.outline", "tools.edit_outline", ""),
    Item::leaf("menu.item.edit.mode.json", "tools.edit_json", ""),
    Item::leaf("menu.item.edit.mode.yaml", "tools.edit_yaml", ""),
    Item::leaf("menu.item.edit.mode.bytes", "tools.edit_bytes", ""),
];

/// Case transforms applied to the selection, grouped under Edit → Case.
const EDIT_CASE: &[Item] = &[
    Item::leaf("menu.item.edit.case_upper", "edit.case_upper", ""),
    Item::leaf("menu.item.edit.case_lower", "edit.case_lower", ""),
    Item::leaf("menu.item.edit.case_title", "edit.case_title", ""),
    Item::leaf("menu.item.edit.case_kebab", "edit.case_kebab", ""),
    Item::leaf("menu.item.edit.case_snake", "edit.case_snake", ""),
    Item::leaf("menu.item.edit.case_camel", "edit.case_camel", ""),
    Item::leaf("menu.item.edit.case_pascal", "edit.case_pascal", ""),
];

/// Editor split commands, grouped under View → Split.
const VIEW_SPLIT: &[Item] = &[
    Item::leaf("menu.item.view.split_horizontal", "view.split_horizontal", ""),
    Item::leaf("menu.item.view.split_vertical", "view.split_vertical", ""),
    SEP,
    Item::leaf("menu.item.view.focus_other_pane", "view.focus_other_pane", "F6"),
    Item::leaf("menu.item.view.unsplit", "view.unsplit", ""),
];

/// Dock/status-bar visibility toggles, grouped under View → Layout.
const VIEW_LAYOUT: &[Item] = &[
    Item::leaf("menu.item.view.left_dock", "view.left_dock", "Ctrl B"),
    Item::leaf("menu.item.view.right_dock", "view.right_dock", ""),
    Item::leaf("menu.item.view.bottom_dock", "view.bottom_dock", ""),
    Item::leaf("menu.item.view.status_bar", "view.status_bar", ""),
    Item::leaf("menu.item.view.breadcrumbs", "view.breadcrumbs", ""),
    Item::leaf("menu.item.view.outline_dock", "view.outline_dock", ""),
    Item::leaf("menu.item.view.zen", "view.zen", ""),
];

/// Editor display toggles, grouped under View → Editor.
/// Terminal font-zoom commands, grouped under View → Zoom. Best-effort: works on
/// terminals that honor a font-resize escape (xterm/urxvt); others zoom via their
/// own keybindings.
const VIEW_ZOOM: &[Item] = &[
    Item::leaf("menu.item.view.zoom_in", "view.zoom_in", ""),
    Item::leaf("menu.item.view.zoom_out", "view.zoom_out", ""),
    Item::leaf("menu.item.view.zoom_reset", "view.zoom_reset", ""),
];

const VIEW_EDITOR: &[Item] = &[
    Item::leaf("menu.item.view.line_numbers", "view.line_numbers", ""),
    Item::leaf("menu.item.view.whitespace", "view.whitespace", ""),
    Item::leaf("menu.item.view.scrollbar", "view.scrollbar", ""),
    Item::leaf("menu.item.view.soft_wrap", "view.soft_wrap", ""),
    Item::leaf("menu.item.view.overwrite", "toggle_overwrite_mode", ""),
    Item::leaf("menu.item.view.ruler", "toggle_ruler", ""),
    SEP,
    Item::leaf("menu.item.view.fold_toggle", "editor.fold_toggle", ""),
    Item::leaf("menu.item.view.fold_all", "editor.fold_all", ""),
    Item::leaf("menu.item.view.unfold_all", "editor.unfold_all", ""),
    SEP,
    Item::leaf("menu.item.view.inlay_hints", "view.inlay_hints", ""),
    Item::leaf("menu.item.view.spellcheck", "view.spellcheck", ""),
    SEP,
    Item::leaf("menu.item.view.auto_pair", "view.auto_pair", ""),
    Item::leaf("menu.item.view.trim_on_save", "view.trim_on_save", ""),
    Item::leaf("menu.item.view.final_newline_on_save", "view.final_newline_on_save", ""),
    SEP,
    Item::leaf("menu.item.view.next_tab", "tab.next", "Ctrl Tab"),
    Item::leaf("menu.item.view.prev_tab", "tab.prev", "Ctrl Shift Tab"),
];

/// Keyboard navigation styles, grouped under View → Keymap. The labels are the
/// proper-noun keymap names (not translated); the actions carry the keymap id
/// after `view.keymap:`. Kept in sync with `crate::keymap_model::KEYMAPS` (a unit
/// test guards the ids).
const VIEW_KEYMAP: &[Item] = &[
    Item::leaf("menu.name.apple", "view.keymap:apple", ""),
    Item::leaf("menu.name.vscode", "view.keymap:vscode", ""),
    Item::leaf("menu.name.emacs", "view.keymap:emacs", ""),
    Item::leaf("menu.name.vim", "view.keymap:vi", ""),
    Item::leaf("menu.name.spacemacs", "view.keymap:spacemacs", ""),
    Item::leaf("menu.name.jetbrains_mac", "view.keymap:jetbrains-mac", ""),
    Item::leaf("menu.name.jetbrains_win", "view.keymap:jetbrains-win", ""),
    Item::leaf("menu.name.eclipse", "view.keymap:eclipse", ""),
];

const VIX: &[Item] = &[
    Item::leaf("menu.item.vix.about", "vix.about", ""),
    Item::leaf("menu.item.vix.website", "vix.website", ""),
    Item::leaf("menu.item.vix.email", "vix.email", ""),
    SEP,
    Item::leaf("menu.item.vix.settings", "vix.settings", ""),
    SEP,
    Item::leaf("menu.item.file.quit", "file.quit", "Ctrl Q"),
];

/// UUID versions, grouped under Tools → Insert → UUID. Labels are the bare
/// version digit (RFC 4122 / RFC 9562 v1–v8).
const TOOLS_INSERT_UUID: &[Item] = &[
    Item::leaf("menu.item.tools.insert.uuid.v1", "tools.insert.uuid.v1", ""),
    Item::leaf("menu.item.tools.insert.uuid.v2", "tools.insert.uuid.v2", ""),
    Item::leaf("menu.item.tools.insert.uuid.v3", "tools.insert.uuid.v3", ""),
    Item::leaf("menu.item.tools.insert.uuid.v4", "tools.insert.uuid.v4", ""),
    Item::leaf("menu.item.tools.insert.uuid.v5", "tools.insert.uuid.v5", ""),
    Item::leaf("menu.item.tools.insert.uuid.v6", "tools.insert.uuid.v6", ""),
    Item::leaf("menu.item.tools.insert.uuid.v7", "tools.insert.uuid.v7", ""),
    Item::leaf("menu.item.tools.insert.uuid.v8", "tools.insert.uuid.v8", ""),
];

/// ZID sizes, grouped under Tools → Insert → ZID. Labels show the bit width
/// and the resulting hex length.
const TOOLS_INSERT_ZID: &[Item] = &[
    Item::leaf("menu.item.tools.insert.zid.128", "tools.insert.zid.128", ""),
    Item::leaf("menu.item.tools.insert.zid.256", "tools.insert.zid.256", ""),
    Item::leaf("menu.item.tools.insert.zid.512", "tools.insert.zid.512", ""),
];

/// HTML snippets, grouped under Tools → Insert → HTML. Each inserts a small HTML
/// template at the cursor. The item labels are shared with the Markdown submenu
/// (same display text), so they reuse the `…insert.markdown.*` label keys.
const TOOLS_INSERT_HTML: &[Item] = &[
    Item::leaf("menu.item.tools.insert.markdown.headline1", "tools.insert.html.headline1", ""),
    Item::leaf("menu.item.tools.insert.markdown.headline2", "tools.insert.html.headline2", ""),
    Item::leaf("menu.item.tools.insert.markdown.headline3", "tools.insert.html.headline3", ""),
    Item::leaf("menu.item.tools.insert.markdown.link", "tools.insert.html.link", ""),
    Item::leaf("menu.item.tools.insert.markdown.list", "tools.insert.html.list", ""),
    Item::leaf("menu.item.tools.insert.markdown.table", "tools.insert.html.table", ""),
];

/// Markdown snippets, grouped under Tools → Insert → Markdown. Each inserts a
/// small Markdown template at the cursor.
const TOOLS_INSERT_MARKDOWN: &[Item] = &[
    Item::leaf("menu.item.tools.insert.markdown.headline1", "tools.insert.markdown.headline1", ""),
    Item::leaf("menu.item.tools.insert.markdown.headline2", "tools.insert.markdown.headline2", ""),
    Item::leaf("menu.item.tools.insert.markdown.headline3", "tools.insert.markdown.headline3", ""),
    Item::leaf("menu.item.tools.insert.markdown.link", "tools.insert.markdown.link", ""),
    Item::leaf("menu.item.tools.insert.markdown.list", "tools.insert.markdown.list", ""),
    Item::leaf("menu.item.tools.insert.markdown.table", "tools.insert.markdown.table", ""),
    Item::leaf("menu.item.tools.insert.markdown.todos", "tools.insert.markdown.todos", ""),
];

/// Lorem ipsum placeholder snippets, grouped under Tools → Insert → Lorem ipsum.
const TOOLS_INSERT_LOREM: &[Item] = &[
    Item::leaf("menu.item.tools.insert.lorem.words", "tools.insert.lorem.words", ""),
    Item::leaf("menu.item.tools.insert.lorem.sentence", "tools.insert.lorem.sentence", ""),
    Item::leaf("menu.item.tools.insert.lorem.paragraph", "tools.insert.lorem.paragraph", ""),
];

/// Date/time presets, grouped under Tools → Insert → Date/Time. Each inserts the
/// current local time formatted to the named standard.
const TOOLS_INSERT_DATETIME: &[Item] = &[
    Item::leaf("menu.name.iso8601", "tools.insert.datetime.iso8601", ""),
    Item::leaf("menu.name.rfc3339", "tools.insert.datetime.rfc3339", ""),
    Item::leaf("menu.item.tools.insert.datetime.epoch", "tools.insert.datetime.epoch", ""),
];

/// Insert helpers, grouped under Tools → Insert.
/// SQL (`PostgreSQL`) snippets, grouped under Tools → Insert → SQL. Each inserts a
/// ready-to-edit statement at the cursor.
const TOOLS_INSERT_SQL: &[Item] = &[
    Item::leaf("menu.item.tools.insert.sql.alter_role", "tools.insert.sql.alter_role", ""),
    Item::leaf("menu.item.tools.insert.sql.create_extension", "tools.insert.sql.create_extension", ""),
    Item::leaf("menu.item.tools.insert.sql.create_function", "tools.insert.sql.create_function", ""),
    Item::leaf("menu.item.tools.insert.sql.create_user", "tools.insert.sql.create_user", ""),
    Item::leaf("menu.item.tools.insert.sql.grant_create", "tools.insert.sql.grant_create", ""),
    Item::leaf("menu.item.tools.insert.sql.grant_usage", "tools.insert.sql.grant_usage", ""),
    Item::leaf("menu.item.tools.insert.sql.create_table", "tools.insert.sql.create_table", ""),
];

/// Org/LaTeX markup snippets, grouped under Tools → Insert → LaTeX. Each inserts
/// a ready-to-edit construct at the cursor.
const TOOLS_INSERT_LATEX: &[Item] = &[
    Item::leaf("menu.item.tools.insert.latex.headline", "tools.insert.latex.headline", ""),
    Item::leaf("menu.item.tools.insert.latex.subheadline", "tools.insert.latex.subheadline", ""),
    Item::leaf("menu.item.tools.insert.latex.link", "tools.insert.latex.link", ""),
    Item::leaf("menu.item.tools.insert.latex.bold", "tools.insert.latex.bold", ""),
    Item::leaf("menu.item.tools.insert.latex.italic", "tools.insert.latex.italic", ""),
    Item::leaf("menu.item.tools.insert.latex.underline", "tools.insert.latex.underline", ""),
    Item::leaf("menu.item.tools.insert.latex.table", "tools.insert.latex.table", ""),
    Item::leaf("menu.item.tools.insert.latex.deadline", "tools.insert.latex.deadline", ""),
    Item::leaf("menu.item.tools.insert.latex.scheduled", "tools.insert.latex.scheduled", ""),
    Item::leaf("menu.item.tools.insert.latex.time_range", "tools.insert.latex.time_range", ""),
    Item::leaf("menu.item.tools.insert.latex.timestamp", "tools.insert.latex.timestamp", ""),
    Item::leaf("menu.item.tools.insert.latex.timestamp_repeater", "tools.insert.latex.timestamp_repeater", ""),
    Item::leaf("menu.item.tools.insert.latex.quote", "tools.insert.latex.quote", ""),
    Item::leaf("menu.item.tools.insert.latex.verse", "tools.insert.latex.verse", ""),
    Item::leaf("menu.item.tools.insert.latex.center", "tools.insert.latex.center", ""),
    Item::leaf("menu.item.tools.insert.latex.drawer", "tools.insert.latex.drawer", ""),
];

/// Org-mode snippets, grouped under Tools → Insert → Org.
const TOOLS_INSERT_ORG: &[Item] = &[
    Item::leaf("menu.item.tools.insert.org.title", "tools.insert.org.title", ""),
    Item::leaf("menu.item.tools.insert.org.author", "tools.insert.org.author", ""),
    Item::leaf("menu.item.tools.insert.org.headline", "tools.insert.org.headline", ""),
    Item::leaf("menu.item.tools.insert.org.subheadline", "tools.insert.org.subheadline", ""),
    Item::leaf("menu.item.tools.insert.org.link", "tools.insert.org.link", ""),
    Item::leaf("menu.item.tools.insert.org.image", "tools.insert.org.image", ""),
    Item::leaf("menu.item.tools.insert.org.list", "tools.insert.org.list", ""),
    Item::leaf("menu.item.tools.insert.org.ordered_list", "tools.insert.org.ordered_list", ""),
    Item::leaf("menu.item.tools.insert.org.check_list", "tools.insert.org.check_list", ""),
    Item::leaf("menu.item.tools.insert.org.table", "tools.insert.org.table", ""),
    Item::leaf("menu.item.tools.insert.org.todo", "tools.insert.org.todo", ""),
    Item::leaf("menu.item.tools.insert.org.done", "tools.insert.org.done", ""),
    Item::leaf("menu.item.tools.insert.org.deadline", "tools.insert.org.deadline", ""),
    Item::leaf("menu.item.tools.insert.org.scheduled", "tools.insert.org.scheduled", ""),
    Item::leaf("menu.item.tools.insert.org.time_range", "tools.insert.org.time_range", ""),
    Item::leaf("menu.item.tools.insert.org.timestamp", "tools.insert.org.timestamp", ""),
    Item::leaf("menu.item.tools.insert.org.timestamp_repeater", "tools.insert.org.timestamp_repeater", ""),
    Item::leaf("menu.item.tools.insert.org.drawer", "tools.insert.org.drawer", ""),
];

/// Org inline emphasis markers, grouped under Tools → Insert → Markers. Each
/// toggles the marker character around the selection.
const TOOLS_INSERT_MARKERS: &[Item] = &[
    Item::leaf("menu.item.tools.insert.marker.bold", "tools.insert.marker.bold", ""),
    Item::leaf("menu.item.tools.insert.marker.italic", "tools.insert.marker.italic", ""),
    Item::leaf("menu.item.tools.insert.marker.underline", "tools.insert.marker.underline", ""),
    Item::leaf("menu.item.tools.insert.marker.strikethrough", "tools.insert.marker.strikethrough", ""),
    Item::leaf("menu.item.tools.insert.marker.code", "tools.insert.marker.code", ""),
    Item::leaf("menu.item.tools.insert.marker.verbatim", "tools.insert.marker.verbatim", ""),
];

/// Org block constructs, grouped under Tools → Insert → Begin-End. Each toggles a
/// `#+BEGIN_…`/`#+END_…` block around the selection.
const TOOLS_INSERT_BLOCK: &[Item] = &[
    Item::leaf("menu.item.tools.insert.block.comment", "tools.insert.block.comment", ""),
    Item::leaf("menu.item.tools.insert.block.center", "tools.insert.block.center", ""),
    Item::leaf("menu.item.tools.insert.block.quote", "tools.insert.block.quote", ""),
    Item::leaf("menu.item.tools.insert.block.verse", "tools.insert.block.verse", ""),
];

const TOOLS_INSERT: &[Item] = &[
    Item::sub("menu.item.tools.insert.uuid", TOOLS_INSERT_UUID),
    Item::sub("menu.item.tools.insert.zid", TOOLS_INSERT_ZID),
    Item::sub("menu.name.html", TOOLS_INSERT_HTML),
    Item::sub("menu.item.tools.insert.markdown", TOOLS_INSERT_MARKDOWN),
    Item::sub("menu.name.sql", TOOLS_INSERT_SQL),
    Item::sub("menu.name.latex", TOOLS_INSERT_LATEX),
    Item::sub("menu.name.org", TOOLS_INSERT_ORG),
    Item::sub("menu.item.tools.insert.markers", TOOLS_INSERT_MARKERS),
    Item::sub("menu.item.tools.insert.block", TOOLS_INSERT_BLOCK),
    Item::sub("menu.name.lorem", TOOLS_INSERT_LOREM),
    Item::sub("menu.item.tools.insert.datetime", TOOLS_INSERT_DATETIME),
];

/// Checksum digests, grouped under Tools → Checksum. Each replaces the selection
/// (or whole buffer) with its hex digest.
const TOOLS_CHECKSUM: &[Item] = &[
    Item::leaf("menu.item.tools.checksum.sha256", "tools.checksum.sha256", ""),
    Item::leaf("menu.item.tools.checksum.sha512", "tools.checksum.sha512", ""),
    Item::leaf("menu.name.md5", "tools.checksum.md5", ""),
    Item::leaf("menu.name.crc32", "tools.checksum.crc32", ""),
];

// Tools → Convert: each entry converts the selection (or whole buffer). Format
// names (CSV/TSV/JSON/…) route through shared `menu.name.*` keys that hold the
// (locale-neutral) name; only Encode/Decode carry descriptive translations.
const TOOLS_CONVERT_BASE64: &[Item] = &[
    Item::leaf("menu.item.tools.convert.encode", "tools.convert.base64.encode", ""),
    Item::leaf("menu.item.tools.convert.decode", "tools.convert.base64.decode", ""),
];
const TOOLS_CONVERT_URL: &[Item] = &[
    Item::leaf("menu.item.tools.convert.encode", "tools.convert.url.encode", ""),
    Item::leaf("menu.item.tools.convert.decode", "tools.convert.url.decode", ""),
];
const TOOLS_CONVERT_CSV: &[Item] = &[
    Item::leaf("menu.name.json", "tools.convert.csv.json", ""),
    Item::leaf("menu.name.tsv", "tools.convert.csv.tsv", ""),
];
const TOOLS_CONVERT_TSV: &[Item] = &[
    Item::leaf("menu.name.csv", "tools.convert.tsv.csv", ""),
    Item::leaf("menu.name.json", "tools.convert.tsv.json", ""),
];
const TOOLS_CONVERT_JSON: &[Item] = &[
    Item::leaf("menu.name.csv", "tools.convert.json.csv", ""),
    Item::leaf("menu.name.tsv", "tools.convert.json.tsv", ""),
    Item::leaf("menu.name.yaml", "tools.convert.json.yaml", ""),
    Item::leaf("menu.name.toml", "tools.convert.json.toml", ""),
];
const TOOLS_CONVERT_TOML: &[Item] = &[Item::leaf("menu.name.json", "tools.convert.toml.json", "")];
const TOOLS_CONVERT_YAML: &[Item] = &[Item::leaf("menu.name.json", "tools.convert.yaml.json", "")];
const TOOLS_CONVERT_NUMBER: &[Item] = &[
    Item::leaf("menu.name.dec", "tools.convert.number.dec", ""),
    Item::leaf("menu.name.hex", "tools.convert.number.hex", ""),
    Item::leaf("menu.name.bin", "tools.convert.number.bin", ""),
    Item::leaf("menu.name.oct", "tools.convert.number.oct", ""),
];
const TOOLS_CONVERT_HTML: &[Item] = &[Item::leaf("menu.name.markdown", "tools.convert.html.markdown", "")];
const TOOLS_CONVERT_MARKDOWN: &[Item] = &[Item::leaf("menu.name.html", "tools.convert.markdown.html", "")];

/// In-place reformatters, grouped under Tools → Format.
const TOOLS_FORMAT: &[Item] = &[
    Item::leaf("menu.item.tools.format.json_pretty", "tools.format.json_pretty", ""),
    Item::leaf("menu.item.tools.format.json_minify", "tools.format.json_minify", ""),
    Item::leaf("menu.name.yaml", "tools.format.yaml", ""),
    Item::leaf("menu.name.toml", "tools.format.toml", ""),
];

/// Converters, grouped under Tools → Convert.
const TOOLS_CONVERT: &[Item] = &[
    Item::sub("menu.name.base64", TOOLS_CONVERT_BASE64),
    Item::sub("menu.name.csv", TOOLS_CONVERT_CSV),
    Item::sub("menu.name.html", TOOLS_CONVERT_HTML),
    Item::sub("menu.name.json", TOOLS_CONVERT_JSON),
    Item::sub("menu.name.markdown", TOOLS_CONVERT_MARKDOWN),
    Item::sub("menu.name.toml", TOOLS_CONVERT_TOML),
    Item::sub("menu.name.tsv", TOOLS_CONVERT_TSV),
    Item::sub("menu.name.url", TOOLS_CONVERT_URL),
    Item::sub("menu.name.yaml", TOOLS_CONVERT_YAML),
    Item::sub("menu.item.tools.convert.number", TOOLS_CONVERT_NUMBER),
    SEP,
    Item::leaf("menu.item.tools.convert.jwt", "tools.convert.jwt", ""),
];

/// Character pickers, grouped under Tools → Characters.
const TOOLS_CHARACTERS: &[Item] = &[
    Item::leaf("menu.item.tools.nerd_palette", "tools.nerd_palette", ""),
    Item::leaf("menu.item.tools.html_chars", "tools.html_chars", ""),
    Item::leaf("menu.item.tools.ascii", "tools.ascii", ""),
];

/// Information panels, grouped under Tools → About.
const TOOLS_ABOUT: &[Item] = &[
    Item::leaf("menu.item.tools.about.text", "tools.text_info", ""),
    Item::leaf("menu.item.tools.about.file", "tools.file_info", ""),
    Item::leaf("menu.item.tools.about.workspace", "tools.dashboard", ""),
    Item::leaf("menu.item.tools.about.system", "tools.system_info", ""),
];

const TOOLS: &[Item] = &[
    Item::leaf("menu.item.tools.palette", "tools.palette", "Ctrl P"),
    Item::leaf("menu.item.tools.run_command", "tools.run_command", ""),
    Item::leaf("menu.item.tools.cancel_command", "tools.cancel_command", ""),
    Item::leaf("menu.item.tools.tasks", "tools.tasks", ""),
    Item::leaf("menu.item.tools.test", "tools.test", ""),
    Item::leaf("menu.item.tools.test_panel", "tools.test_panel", ""),
    Item::leaf("menu.item.tools.terminal", "tools.terminal", ""),
    Item::leaf("menu.item.tools.diff", "tools.diff", ""),
    SEP,
    Item::sub("menu.item.tools.about", TOOLS_ABOUT),
    Item::sub("menu.item.tools.lsp", TOOLS_LSP),
    Item::sub("menu.item.tools.insert", TOOLS_INSERT),
    Item::sub("menu.item.tools.checksum", TOOLS_CHECKSUM),
    Item::sub("menu.item.tools.convert", TOOLS_CONVERT),
    Item::sub("menu.item.tools.format", TOOLS_FORMAT),
    Item::leaf("menu.item.tools.color_converter", "tools.color_converter", ""),
    Item::leaf("menu.item.tools.convert.unit", "tools.convert.unit", ""),
    Item::leaf("menu.item.tools.markdown_preview", "tools.markdown_preview", ""),
    Item::leaf("menu.item.tools.qrcode", "tools.qrcode", ""),
    Item::leaf("menu.item.tools.snippets", "tools.snippets", ""),
    SEP,
    Item::leaf("menu.item.tools.contacts", "tools.contacts", ""),
    Item::leaf("menu.item.tools.calendar", "tools.calendar", ""),
    Item::leaf("menu.item.tools.clock", "tools.clock", ""),
    Item::leaf("menu.item.tools.pomodoro", "tools.pomodoro", ""),
    Item::leaf("menu.item.tools.calculator", "tools.calculator", ""),
    Item::leaf("menu.item.tools.regex_tester", "tools.regex_tester", ""),
    Item::sub("menu.item.tools.characters", TOOLS_CHARACTERS),
    Item::leaf("menu.item.tools.x11_colors", "tools.x11_colors", ""),
];

/// Language-server (LSP) actions, grouped under Tools → Language Server.
const TOOLS_LSP: &[Item] = &[
    Item::leaf("menu.item.lsp.definition", "nav.goto_definition", "F12"),
    Item::leaf("menu.item.lsp.implementation", "nav.goto_implementation", ""),
    Item::leaf("menu.item.lsp.type_definition", "nav.goto_type_definition", ""),
    Item::leaf("menu.item.lsp.references", "lsp.references", ""),
    Item::leaf("menu.item.lsp.rename", "lsp.rename", "F2"),
    Item::leaf("menu.item.lsp.linked_edit", "lsp.linked_edit", ""),
    Item::leaf("menu.item.lsp.code_action", "lsp.code_action", ""),
    Item::leaf("menu.item.lsp.code_lens", "lsp.code_lens", ""),
    Item::leaf("menu.item.lsp.highlight", "lsp.highlight", ""),
    Item::leaf("menu.item.lsp.expand_selection", "lsp.expand_selection", ""),
    Item::leaf("menu.item.lsp.shrink_selection", "lsp.shrink_selection", ""),
    Item::leaf("menu.item.lsp.format", "lsp.format", ""),
    Item::leaf("menu.item.lsp.hover", "lsp.hover", ""),
    Item::leaf("menu.item.lsp.signature_help", "lsp.signature_help", ""),
    Item::leaf("menu.item.lsp.complete", "lsp.complete", "Ctrl Space"),
    Item::leaf("menu.item.lsp.document_symbols", "lsp.document_symbols", ""),
    Item::leaf("menu.item.lsp.workspace_symbols", "lsp.workspace_symbols", ""),
    Item::leaf("menu.item.lsp.diagnostics", "lsp.diagnostics", ""),
];

const AI: &[Item] = &[
    Item::leaf("menu.item.ai.chat", "ai.chat", ""),
    SEP,
    Item::leaf("menu.item.ai.summarize", "ai.summarize", ""),
    Item::leaf("menu.item.ai.explain", "ai.explain", ""),
    Item::leaf("menu.item.ai.define", "ai.define", ""),
    SEP,
    Item::leaf("menu.item.ai.annotate", "ai.annotate", ""),
    Item::leaf("menu.item.ai.improve", "ai.improve", ""),
];

/// Log views, grouped under Git → Log.
const GIT_LOG: &[Item] = &[
    Item::leaf("menu.item.git.log_graph", "git.log_graph", ""),
    Item::leaf("menu.item.git.log_1_day", "git.log_since_1_day_ago", ""),
    Item::leaf("menu.item.git.log_1_week", "git.log_since_1_week_ago", ""),
    Item::leaf("menu.item.git.log_1_month", "git.log_since_1_month_ago", ""),
    Item::leaf("menu.item.git.log_all", "git.log", ""),
];

/// Branch commands, grouped under Git → Branch.
const GIT_BRANCH: &[Item] = &[
    Item::leaf("menu.item.git.new_branch", "git.new_branch", ""),
    Item::leaf("menu.item.git.switch_branch", "git.switch_branch", ""),
    Item::leaf("menu.item.git.merge_branch", "git.merge_branch", ""),
    Item::leaf("menu.item.git.delete_branch", "git.delete_branch", ""),
    Item::leaf("menu.item.git.edit_description", "git.edit_description", ""),
];

/// Merge-conflict resolutions, grouped under Git → Resolve.
const GIT_RESOLVE: &[Item] = &[
    Item::leaf("menu.item.git.conflict_ours", "git.conflict_ours", ""),
    Item::leaf("menu.item.git.conflict_theirs", "git.conflict_theirs", ""),
    Item::leaf("menu.item.git.conflict_both", "git.conflict_both", ""),
];

/// Pull strategies, grouped under Git → Pull.
const GIT_PULL: &[Item] = &[
    Item::leaf("menu.item.git.pull_ff", "git.pull_ff", ""),
    Item::leaf("menu.item.git.pull_rebase", "git.pull_rebase", ""),
    Item::leaf("menu.item.git.pull_merge", "git.pull_merge", ""),
    Item::leaf("menu.item.git.pull_squash", "git.pull_squash", ""),
];

const GIT: &[Item] = &[
    Item::leaf("menu.item.git.status", "git.status", ""),
    Item::leaf("menu.item.git.changes", "git.changes", ""),
    Item::sub("menu.item.git.log", GIT_LOG),
    Item::leaf("menu.item.git.grep", "git.grep", ""),
    Item::leaf("menu.item.git.blame", "git.blame", ""),
    Item::leaf("menu.item.git.blame_inline", "git.blame_inline", ""),
    SEP,
    Item::leaf("menu.item.git.diff_next", "git.diff_next", ""),
    Item::leaf("menu.item.git.diff_prev", "git.diff_prev", ""),
    Item::leaf("menu.item.git.stage_hunk", "git.stage_hunk", ""),
    Item::leaf("menu.item.git.unstage_hunk", "git.unstage_hunk", ""),
    Item::leaf("menu.item.git.revert_hunk", "git.revert_hunk", ""),
    SEP,
    Item::leaf("menu.item.git.conflict_next", "git.conflict_next", ""),
    Item::sub("menu.item.git.resolve", GIT_RESOLVE),
    SEP,
    Item::sub("menu.item.git.branch", GIT_BRANCH),
    SEP,
    Item::leaf("menu.item.git.amend", "git.amend", ""),
    Item::leaf("menu.item.git.stash", "git.stash", ""),
    Item::leaf("menu.item.git.stash_pop", "git.stash_pop", ""),
    SEP,
    Item::sub("menu.item.git.pull", GIT_PULL),
    Item::leaf("menu.item.git.push", "git.push", ""),
    Item::leaf("menu.item.git.fetch", "git.fetch", ""),
    SEP,
    Item::leaf("menu.item.git.init", "git.init", ""),
    Item::leaf("menu.item.git.clone", "git.clone", ""),
];

const HELP: &[Item] = &[
    Item::leaf("menu.item.help.welcome", "help.welcome", ""),
    Item::leaf("menu.item.help.shortcuts", "help.shortcuts", "F1"),
];

/// Available theme names for the View → Theme submenu, set once by the host at
/// startup (before the menu is first used).
static THEME_NAMES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

/// The fully-built menu bar, cached on first use.
static MENUS_CELL: std::sync::OnceLock<Vec<MenuDef>> = std::sync::OnceLock::new();

/// Provide the theme names that populate the View → Theme submenu. Call once at
/// startup, before [`menus`] is first called; later calls are ignored (the menu
/// is built and cached on first use).
pub fn set_theme_names(names: Vec<String>) {
    let _ = THEME_NAMES.set(names);
}

/// The full menu bar, left to right. Built once: every menu is static except
/// View → Theme, whose items are the runtime theme list (see [`set_theme_names`]).
#[must_use]
pub fn menus() -> &'static [MenuDef] {
    MENUS_CELL.get_or_init(build_menus).as_slice()
}

/// Build the View → Theme submenu items from the available theme names, leaking
/// them to `'static`. Falls back to the bundled Dark/Light when none are set.
fn theme_submenu() -> &'static [Item] {
    let names = THEME_NAMES.get().cloned().unwrap_or_default();
    let mut items: Vec<Item> = names
        .iter()
        .map(|n| {
            let label: &'static str = Box::leak(n.clone().into_boxed_str());
            let action: &'static str = Box::leak(format!("view.theme:{n}").into_boxed_str());
            Item::leaf(label, action, "")
        })
        .collect();
    if items.is_empty() {
        items.push(Item::leaf("Dark", "view.theme:Dark", ""));
        items.push(Item::leaf("Light", "view.theme:Light", ""));
    }
    Box::leak(items.into_boxed_slice())
}

/// The View → Locale submenu: one item per bundled locale (endonym label),
/// dispatching `view.locale:<code>`.
fn locale_submenu() -> &'static [Item] {
    let items: Vec<Item> = crate::locale_model::LOCALES
        .iter()
        .map(|l| {
            let action: &'static str = Box::leak(format!("view.locale:{}", l.code).into_boxed_str());
            Item::leaf(l.name, action, "")
        })
        .collect();
    Box::leak(items.into_boxed_slice())
}

/// The View → Time Zone submenu: one item per IANA zone, ordered by UTC offset
/// then name, labeled `UTC±HH:MM  Name` and dispatching `view.time_zone:<name>`.
fn time_zone_submenu() -> &'static [Item] {
    let mut zones: Vec<&'static crate::time_zone_model::Zone> =
        crate::time_zone_model::ZONES.iter().collect();
    zones.sort_by(|a, b| {
        a.std_offset_minutes
            .cmp(&b.std_offset_minutes)
            .then_with(|| a.name.cmp(b.name))
    });
    let items: Vec<Item> = zones
        .iter()
        .map(|z| {
            let label: &'static str =
                Box::leak(format!("{}  {}", z.offset_label(), z.name).into_boxed_str());
            let action: &'static str =
                Box::leak(format!("view.time_zone:{}", z.name).into_boxed_str());
            Item::leaf(label, action, "")
        })
        .collect();
    Box::leak(items.into_boxed_slice())
}

/// Debugger commands (DAP), grouped under the top-level Debug menu.
const DEBUG: &[Item] = &[
    Item::leaf("menu.item.debug.start", "debug.start", ""),
    Item::leaf("menu.item.debug.stop", "debug.stop", ""),
    SEP,
    Item::leaf("menu.item.debug.toggle_breakpoint", "debug.toggle_breakpoint", ""),
    SEP,
    Item::leaf("menu.item.debug.continue", "debug.continue", ""),
    Item::leaf("menu.item.debug.step_over", "debug.step_over", ""),
    Item::leaf("menu.item.debug.step_into", "debug.step_into", ""),
    Item::leaf("menu.item.debug.step_out", "debug.step_out", ""),
    Item::leaf("menu.item.debug.pause", "debug.pause", ""),
    SEP,
    Item::leaf("menu.item.debug.watch", "debug.watch", ""),
    Item::leaf("menu.item.debug.repl", "debug.repl", ""),
    Item::leaf("menu.item.debug.panel", "debug.panel", ""),
];

fn build_menus() -> Vec<MenuDef> {
    let view_items: &'static [Item] = Box::leak(
        vec![
            Item::sub("menu.item.view.keymap", VIEW_KEYMAP),
            Item::sub("menu.item.view.theme", theme_submenu()),
            Item::sub("menu.item.view.locale", locale_submenu()),
            Item::sub("menu.item.view.time_zone", time_zone_submenu()),
            SEP,
            Item::sub("menu.item.view.split", VIEW_SPLIT),
            Item::sub("menu.item.view.layout", VIEW_LAYOUT),
            Item::sub("menu.item.view.editor", VIEW_EDITOR),
            Item::sub("menu.item.view.zoom", VIEW_ZOOM),
        ]
        .into_boxed_slice(),
    );
    vec![
        MenuDef { name: "menu.vix", items: VIX },
        MenuDef { name: "menu.file", items: FILE },
        MenuDef { name: "menu.edit", items: EDIT },
        MenuDef { name: "menu.view", items: view_items },
        MenuDef { name: "menu.tools", items: TOOLS },
        MenuDef { name: "menu.ai", items: AI },
        MenuDef { name: "menu.git", items: GIT },
        MenuDef { name: "menu.debug", items: DEBUG },
        MenuDef { name: "menu.help", items: HELP },
    ]
}

/// Index of the first non-separator item in `items` (0 if none).
fn first_selectable(items: &[Item]) -> usize {
    items.iter().position(|it| !it.is_separator()).unwrap_or(0)
}

/// The next item after `from` (cycling) whose translated label starts with `c`
/// (ASCII case-insensitive), skipping separators. `None` if none match.
fn label_starting(items: &[Item], from: usize, c: char) -> Option<usize> {
    let target = c.to_ascii_lowercase();
    let len = items.len();
    (1..=len).map(|step| (from + step) % len).find(|&j| {
        !items[j].is_separator()
            && items[j]
                .label()
                .chars()
                .next()
                .is_some_and(|fc| fc.to_ascii_lowercase() == target)
    })
}

/// The next non-separator index after `from`, wrapping around.
fn next_selectable(items: &[Item], from: usize) -> usize {
    let len = items.len();
    let mut j = from;
    for _ in 0..len {
        j = (j + 1) % len;
        if !items[j].is_separator() {
            break;
        }
    }
    j
}

/// The previous non-separator index before `from`, wrapping around.
fn prev_selectable(items: &[Item], from: usize) -> usize {
    let len = items.len();
    let mut j = from;
    for _ in 0..len {
        j = (j + len - 1) % len;
        if !items[j].is_separator() {
            break;
        }
    }
    j
}

/// Open/highlight state for the menu bar, including a one-level submenu.
#[derive(Default)]
pub struct Menu {
    /// Which top-level menu is open, if any.
    pub open: Option<usize>,
    /// Highlighted item within the open dropdown, or `None` when the dropdown has
    /// just opened and nothing is highlighted yet (the user must arrow, hover, or
    /// type to pick an item).
    pub item: Option<usize>,
    /// The highlighted index within the open submenu, or `None` when the submenu
    /// has just opened and nothing is highlighted yet. Only meaningful while
    /// [`Self::sub_open`] is true.
    pub sub: Option<usize>,
    /// Whether the highlighted item's submenu is open (independent of whether a
    /// submenu row is highlighted — opening one highlights nothing).
    pub sub_open: bool,
    /// The highlighted index within the open sub-submenu (third level), or `None`
    /// when it has just opened. Only meaningful while [`Self::subsub_open`] is true.
    pub subsub: Option<usize>,
    /// Whether the highlighted submenu item's own submenu (third level) is open.
    pub subsub_open: bool,
}

impl Menu {
    /// Whether a dropdown is currently open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// Toggle the first menu open, or close the open one.
    pub fn toggle(&mut self) {
        if self.open.is_some() {
            self.close();
        } else {
            self.open_index(0);
        }
    }

    /// Close any open dropdown.
    pub fn close(&mut self) {
        self.open = None;
        self.item = None;
        self.sub = None;
        self.sub_open = false;
        self.subsub = None;
        self.subsub_open = false;
    }

    /// Open the menu at index `i` (no-op if out of range). No item is highlighted
    /// yet — the user picks one by arrowing, hovering, or typing.
    pub fn open_index(&mut self, i: usize) {
        if i < menus().len() {
            self.open = Some(i);
            self.item = None;
            self.sub = None;
            self.sub_open = false;
            self.subsub = None;
            self.subsub_open = false;
        }
    }

    /// Highlight top-level dropdown item `idx`, collapsing any open submenu and
    /// sub-submenu. Used when the pointer moves to a different top item, so a
    /// later [`Self::right`] can open the new item's submenu (a stale
    /// `subsub_open` would otherwise make `right` a no-op).
    pub fn highlight_item(&mut self, idx: usize) {
        self.item = Some(idx);
        self.sub = None;
        self.sub_open = false;
        self.subsub = None;
        self.subsub_open = false;
    }

    /// Highlight submenu row `idx`, collapsing any open sub-submenu.
    pub fn highlight_sub(&mut self, idx: usize) {
        self.sub = Some(idx);
        self.subsub = None;
        self.subsub_open = false;
    }

    /// Whether the highlighted item's submenu is open.
    #[must_use]
    pub fn submenu_open(&self) -> bool {
        self.sub_open
    }

    /// The submenu items of the currently highlighted top item, if it has one.
    #[must_use]
    pub fn submenu_items(&self) -> Option<&'static [Item]> {
        let i = self.open?;
        let it = self.item?;
        menus()[i].items[it].submenu
    }

    /// Whether the highlighted submenu item's own (third-level) submenu is open.
    #[must_use]
    pub fn subsubmenu_open(&self) -> bool {
        self.subsub_open
    }

    /// The third-level submenu items of the currently highlighted submenu item, if
    /// it has any.
    #[must_use]
    pub fn subsubmenu_items(&self) -> Option<&'static [Item]> {
        let sub = self.submenu_items()?;
        let sidx = self.sub?;
        sub.get(sidx).and_then(|it| it.submenu)
    }

    /// Move to the previous top-level menu; or, if a deeper level is open, close
    /// the deepest one.
    pub fn left(&mut self) {
        let Some(i) = self.open else { return };
        if self.subsub_open {
            self.subsub_open = false;
            self.subsub = None;
            return;
        }
        if self.sub_open {
            self.sub_open = false;
            self.sub = None;
            return;
        }
        let n = menus().len();
        self.open_index((i + n - 1) % n);
    }

    /// Move to the next top-level menu; or, if the highlighted item has a closed
    /// submenu, open it (without highlighting any item yet). Works at every depth.
    pub fn right(&mut self) {
        let Some(i) = self.open else { return };
        if self.subsub_open {
            return;
        }
        if self.sub_open {
            // Open the third level (highlighting its first item) if the highlighted
            // submenu row has one.
            if let Some(ss) = self.subsubmenu_items() {
                self.subsub_open = true;
                self.subsub = Some(first_selectable(ss));
            }
            return;
        }
        if let Some(it) = self.item
            && let Some(sub) = menus()[i].items[it].submenu {
                self.sub_open = true;
                self.sub = Some(first_selectable(sub));
                return;
            }
        let n = menus().len();
        self.open_index((i + 1) % n);
    }

    /// Highlight the previous selectable item in the deepest open level. With
    /// nothing highlighted yet, highlights the last selectable item.
    pub fn up(&mut self) {
        let Some(i) = self.open else { return };
        let items = menus()[i].items;
        if self.subsub_open {
            if let Some(ss) = self.subsubmenu_items() {
                self.subsub = Some(prev_selectable(ss, self.subsub.unwrap_or(0)));
            }
            return;
        }
        if self.sub_open {
            if let Some(sub) = self.item.and_then(|it| items[it].submenu) {
                self.sub = Some(prev_selectable(sub, self.sub.unwrap_or(0)));
            }
            return;
        }
        // From the first item, Up moves to the menu title (nothing highlighted);
        // from the title (None), Up wraps to the last item.
        match self.item {
            Some(it) if it == first_selectable(items) => self.item = None,
            Some(it) => self.item = Some(prev_selectable(items, it)),
            None => self.item = Some(prev_selectable(items, 0)),
        }
    }

    /// Highlight the next selectable item in the deepest open level. With nothing
    /// highlighted yet, highlights the first selectable item.
    pub fn down(&mut self) {
        let Some(i) = self.open else { return };
        let items = menus()[i].items;
        if self.subsub_open {
            if let Some(ss) = self.subsubmenu_items() {
                self.subsub = Some(match self.subsub {
                    Some(s) => next_selectable(ss, s),
                    None => first_selectable(ss),
                });
            }
            return;
        }
        if self.sub_open {
            if let Some(sub) = self.item.and_then(|it| items[it].submenu) {
                self.sub = Some(match self.sub {
                    Some(s) => next_selectable(sub, s),
                    None => first_selectable(sub),
                });
            }
            return;
        }
        match self.item {
            Some(it) => self.item = Some(next_selectable(items, it)),
            None => self.item = Some(first_selectable(items)),
        }
    }

    /// Activate the highlighted item: open its submenu (returning `None`) or
    /// return the leaf action to run. Does nothing when nothing is highlighted.
    pub fn enter(&mut self) -> Option<&'static str> {
        let i = self.open?;
        let items = menus()[i].items;
        let it_idx = self.item?;
        if self.subsub_open {
            let ss = self.subsubmenu_items()?;
            let it = &ss[self.subsub?];
            return (!it.is_separator()).then_some(it.action);
        }
        if self.sub_open {
            let sub = items[it_idx].submenu?;
            let it = &sub[self.sub?];
            if it.submenu.is_some() {
                // Open the third level; highlight nothing yet.
                self.subsub_open = true;
                self.subsub = None;
                return None;
            }
            return (!it.is_separator()).then_some(it.action);
        }
        let it = &items[it_idx];
        if it.submenu.is_some() {
            // Open the submenu; highlight nothing yet.
            self.sub_open = true;
            self.sub = None;
            return None;
        }
        (!it.is_separator()).then_some(it.action)
    }

    /// Type-ahead: highlight the next item whose label starts with `c` (cycling
    /// from the current selection), within the deepest open level. With nothing
    /// highlighted yet, searches from the top. Lets the user press e.g. `S`, `S`
    /// to step Save → Save As.
    pub fn type_ahead(&mut self, c: char) {
        let Some(i) = self.open else { return };
        let items = menus()[i].items;
        if self.subsub_open {
            if let Some(ss) = self.subsubmenu_items() {
                let from = self.subsub.unwrap_or_else(|| ss.len().saturating_sub(1));
                if let Some(j) = label_starting(ss, from, c) {
                    self.subsub = Some(j);
                }
            }
            return;
        }
        if self.sub_open {
            if let Some(sub) = self.item.and_then(|it| items[it].submenu) {
                let from = self.sub.unwrap_or_else(|| sub.len().saturating_sub(1));
                if let Some(j) = label_starting(sub, from, c) {
                    self.sub = Some(j);
                }
            }
            return;
        }
        if let Some(it) = self.item {
            if let Some(j) = label_starting(items, it, c) {
                self.item = Some(j);
            }
        } else if let Some(j) = label_starting(items, items.len().saturating_sub(1), c) {
            // Start the search just before index 0 so the first match from the
            // top wins.
            self.item = Some(j);
        }
    }

    /// The action of the highlighted leaf item, or `None` for a separator, a
    /// submenu parent, or nothing highlighted. Non-mutating (unlike [`Menu::enter`]).
    #[must_use]
    pub fn selected_action(&self) -> Option<&'static str> {
        let i = self.open?;
        let items = menus()[i].items;
        let it_idx = self.item?;
        if self.subsub_open {
            let ss = self.subsubmenu_items()?;
            let it = &ss[self.subsub?];
            return (!it.is_separator()).then_some(it.action);
        }
        if self.sub_open {
            let sub = items[it_idx].submenu?;
            let it = &sub[self.sub?];
            if it.has_submenu() {
                return None;
            }
            return (!it.is_separator()).then_some(it.action);
        }
        let it = &items[it_idx];
        if it.has_submenu() {
            return None;
        }
        (!it.is_separator()).then_some(it.action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The View → Keymap submenu must stay in sync with the keymap model: one
    /// item per keymap, each action `view.keymap:<id>` in list order.
    #[test]
    fn keymap_submenu_matches_model() {
        let ids: Vec<&str> = VIEW_KEYMAP
            .iter()
            .map(|it| it.action.strip_prefix("view.keymap:").expect("keymap action prefix"))
            .collect();
        let model: Vec<&str> = crate::keymap_model::KEYMAPS.iter().map(|k| k.id).collect();
        assert_eq!(ids, model);
    }

    /// Index of the Tools menu and of its Generate item, derived from the live
    /// menu so the test is independent of ordering.
    fn tools_and_generate() -> (usize, usize) {
        let tools = menus().iter().position(|m| m.name == "menu.tools").expect("tools menu");
        let gen_idx = menus()[tools]
            .items
            .iter()
            .position(|it| it.label == "menu.item.tools.insert")
            .expect("generate item");
        (tools, gen_idx)
    }

    /// Tools → Insert → UUID offers exactly v1…v8, dispatching the matching
    /// `tools.insert.uuid.vN` actions.
    #[test]
    fn generate_uuid_submenu_lists_all_versions() {
        let actions: Vec<&str> = TOOLS_INSERT_UUID.iter().map(|it| it.action).collect();
        assert_eq!(
            actions,
            (1..=8).map(|n| format!("tools.insert.uuid.v{n}")).collect::<Vec<_>>()
        );
    }

    /// Keyboard navigation can descend all three levels: Tools → Insert → UUID
    /// → "4" and `enter` returns the v4 action; `left` walks back up level by level.
    #[test]
    fn three_level_navigation_reaches_uuid_leaf() {
        let (tools, gen_idx) = tools_and_generate();
        let mut m = Menu::default();
        m.open_index(tools);
        m.item = Some(gen_idx);
        m.right(); // open Generate
        assert!(m.sub_open && !m.subsub_open);
        // Highlight the UUID submenu row (index 0) and open it.
        m.sub = Some(0);
        m.right();
        assert!(m.subsub_open, "third level should be open");
        // Opening highlights the first version (v1); step down to the 4th.
        assert_eq!(m.subsub, Some(0), "third level highlights its first item");
        for _ in 0..3 {
            m.down();
        }
        assert_eq!(m.enter(), Some("tools.insert.uuid.v4"));
        // Walking left collapses one level at a time.
        m.left();
        assert!(!m.subsub_open && m.sub_open);
        m.left();
        assert!(!m.sub_open);
    }

    #[test]
    fn edit_mode_submenu_hosts_the_edit_surfaces() {
        let edit = menus().iter().find(|m| m.name == "menu.edit").unwrap();
        let mode = edit
            .items
            .iter()
            .find(|it| it.label == "menu.item.edit.mode")
            .and_then(|it| it.submenu)
            .expect("Edit has a Mode submenu");
        let actions: Vec<&str> = mode.iter().map(|it| it.action).collect();
        assert_eq!(
            actions,
            vec!["tools.edit_table", "tools.edit_outline", "tools.edit_json", "tools.edit_yaml", "tools.edit_bytes"]
        );
        // The edit surfaces no longer live in the Tools menu.
        let tools = menus().iter().find(|m| m.name == "menu.tools").unwrap();
        assert!(!tools.items.iter().any(|it| it.action == "tools.edit_table"));
    }

    #[test]
    fn right_highlights_first_submenu_item() {
        let (tools, gen_idx) = tools_and_generate();
        let mut m = Menu::default();
        m.open_index(tools);
        m.item = Some(gen_idx);
        m.right();
        assert!(m.sub_open, "submenu opens");
        assert_eq!(m.sub, Some(first_selectable(menus()[tools].items[gen_idx].submenu.unwrap())));
    }

    #[test]
    fn up_from_first_item_highlights_the_title() {
        let mut m = Menu::default();
        m.open_index(0);
        m.down(); // first item highlighted
        let first = first_selectable(menus()[0].items);
        assert_eq!(m.item, Some(first));
        m.up(); // moves up to the title (nothing highlighted)
        assert_eq!(m.item, None, "Up from the first item highlights the menu title");
        m.up(); // from the title, Up wraps to the last item
        assert_eq!(m.item, Some(prev_selectable(menus()[0].items, 0)));
    }

    /// Regression: after descending into a third-level submenu, moving the
    /// pointer to a *different* top item (via `highlight_item`) must still let
    /// `right` open that item's submenu — a stale `subsub_open` used to block it.
    #[test]
    fn reanchoring_after_three_levels_reopens_submenu() {
        let (tools, gen_idx) = tools_and_generate();
        let mut m = Menu::default();
        m.open_index(tools);
        m.highlight_item(gen_idx);
        m.right(); // Generate submenu
        m.highlight_sub(0); // UUID row
        m.right(); // third level open
        assert!(m.subsub_open);
        // Move to another top item that has a submenu (find one after Generate).
        let other = menus()[tools]
            .items
            .iter()
            .position(|it| it.has_submenu() && it.label != "menu.item.tools.insert")
            .expect("another submenu item");
        m.highlight_item(other);
        m.right();
        assert!(m.sub_open, "the new item's submenu must open");
        assert!(!m.subsub_open, "no stale third level remains");
    }
}
