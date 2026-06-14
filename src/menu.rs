//! Top menu bar with keyboard-navigable dropdowns (one level of submenus).
//!
//! Menu items carry an `action` string that `App::run_action` dispatches; the
//! command palette reuses the very same action names. Display text is stored as
//! an i18n key (see `locales/`) and translated at render time via [`Item::label`]
//! and [`MenuDef::title`], so the bar follows the active locale. An item may
//! instead open a nested submenu (e.g. View → Editor, Edit → Find).

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
    Item::leaf("menu.item.file.save", "file.save", "Ctrl S"),
    Item::leaf("menu.item.file.save_as", "file.save_as", "Ctrl Shift S"),
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
    Item::sub("menu.item.edit.move_menu", EDIT_MOVE),
    Item::sub("menu.item.edit.go_menu", EDIT_GO),
    Item::sub("menu.item.edit.find_menu", EDIT_FIND),
    Item::sub("menu.item.edit.case", EDIT_CASE),
    SEP,
    Item::leaf("menu.item.edit.toggle_comment", "edit.toggle_comment", "Ctrl /"),
];

/// Cursor jump commands, grouped under Edit → Go.
const EDIT_GO: &[Item] = &[
    Item::leaf("menu.item.edit.go_line", "nav.goto_line", ""),
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
const EDIT_MOVE: &[Item] = &[
    Item::leaf("menu.item.edit.move_up", "edit.move_line_up", "Alt ↑"),
    Item::leaf("menu.item.edit.move_down", "edit.move_line_down", "Alt ↓"),
];

/// Selection commands, grouped under Edit → Select.
const EDIT_SELECT: &[Item] = &[
    Item::leaf("menu.item.edit.select_more", "edit.select_more", "Ctrl Shift →"),
    Item::leaf("menu.item.edit.select_less", "edit.select_less", "Ctrl Shift ←"),
    SEP,
    Item::leaf("menu.item.edit.select_line", "edit.select_line", ""),
    Item::leaf("menu.item.edit.select_paragraph", "edit.select_paragraph", ""),
    Item::leaf("menu.item.edit.select_section", "edit.select_section", ""),
    SEP,
    Item::leaf("menu.item.edit.select_all", "edit.select_all", "Ctrl A"),
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

/// Dock/status-bar visibility toggles, grouped under View → Layout.
const VIEW_LAYOUT: &[Item] = &[
    Item::leaf("menu.item.view.left_dock", "view.left_dock", "Ctrl B"),
    Item::leaf("menu.item.view.right_dock", "view.right_dock", ""),
    Item::leaf("menu.item.view.bottom_dock", "view.bottom_dock", ""),
    Item::leaf("menu.item.view.status_bar", "view.status_bar", ""),
];

/// Editor display toggles, grouped under View → Editor.
const VIEW_EDITOR: &[Item] = &[
    Item::leaf("menu.item.view.line_numbers", "view.line_numbers", ""),
    Item::leaf("menu.item.view.whitespace", "view.whitespace", ""),
    Item::leaf("menu.item.view.scrollbar", "view.scrollbar", ""),
    Item::leaf("menu.item.view.soft_wrap", "view.soft_wrap", ""),
    SEP,
    Item::leaf("menu.item.view.spellcheck", "view.spellcheck", ""),
    SEP,
    Item::leaf("menu.item.view.next_tab", "tab.next", "Ctrl Tab"),
    Item::leaf("menu.item.view.prev_tab", "tab.prev", "Ctrl Shift Tab"),
];

const VIEW: &[Item] = &[
    Item::leaf("menu.item.view.theme", "view.theme", ""),
    Item::leaf("menu.item.view.locale", "view.locale", ""),
    Item::leaf("menu.item.view.keymap", "view.keymap", ""),
    SEP,
    Item::sub("menu.item.view.layout", VIEW_LAYOUT),
    Item::sub("menu.item.view.editor", VIEW_EDITOR),
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

const TOOLS: &[Item] = &[
    Item::leaf("menu.item.tools.palette", "tools.palette", "Ctrl P"),
    Item::sub("menu.item.tools.lsp", TOOLS_LSP),
    Item::leaf("menu.item.tools.dashboard", "tools.dashboard", ""),
    Item::leaf("menu.item.tools.system_info", "tools.system_info", ""),
    Item::leaf("menu.item.tools.file_info", "tools.file_info", ""),
    Item::leaf("menu.item.tools.contacts", "tools.contacts", ""),
    SEP,
    Item::leaf("menu.item.tools.run_command", "tools.run_command", ""),
    Item::leaf("menu.item.tools.cancel_command", "tools.cancel_command", ""),
    SEP,
    Item::leaf("menu.item.tools.calendar", "tools.calendar", ""),
    Item::leaf("menu.item.tools.nerd_palette", "tools.nerd_palette", ""),
    Item::leaf("menu.item.tools.ascii", "tools.ascii", ""),
    Item::leaf("menu.item.tools.html_chars", "tools.html_chars", ""),
    Item::leaf("menu.item.tools.x11_colors", "tools.x11_colors", ""),
];

/// Language-server (LSP) actions, grouped under Tools → Language Server.
const TOOLS_LSP: &[Item] = &[
    Item::leaf("menu.item.lsp.definition", "nav.goto_definition", "F12"),
    Item::leaf("menu.item.lsp.hover", "lsp.hover", ""),
    Item::leaf("menu.item.lsp.complete", "lsp.complete", "Ctrl Space"),
];

const AI: &[Item] = &[
    Item::leaf("menu.item.ai.summarize", "ai.summarize", ""),
    Item::leaf("menu.item.ai.explain", "ai.explain", ""),
    Item::leaf("menu.item.ai.annotate", "ai.annotate", ""),
];

const GIT: &[Item] = &[
    Item::leaf("menu.item.git.status", "git.status", ""),
    Item::leaf("menu.item.git.changes", "git.changes", ""),
    Item::leaf("menu.item.git.log", "git.log", ""),
    SEP,
    Item::leaf("menu.item.git.new_branch", "git.new_branch", ""),
    Item::leaf("menu.item.git.switch_branch", "git.switch_branch", ""),
    Item::leaf("menu.item.git.merge_branch", "git.merge_branch", ""),
    SEP,
    Item::leaf("menu.item.git.pull", "git.pull", ""),
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

/// The full menu bar, left to right.
pub const MENUS: &[MenuDef] = &[
    MenuDef { name: "menu.vix", items: VIX },
    MenuDef { name: "menu.file", items: FILE },
    MenuDef { name: "menu.edit", items: EDIT },
    MenuDef { name: "menu.view", items: VIEW },
    MenuDef { name: "menu.tools", items: TOOLS },
    MenuDef { name: "menu.ai", items: AI },
    MenuDef { name: "menu.git", items: GIT },
    MenuDef { name: "menu.help", items: HELP },
];

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
    /// When `Some`, the highlighted item's submenu is open and this is the
    /// highlighted index within it.
    pub sub: Option<usize>,
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
    }

    /// Open the menu at index `i` (no-op if out of range). No item is highlighted
    /// yet — the user picks one by arrowing, hovering, or typing.
    pub fn open_index(&mut self, i: usize) {
        if i < MENUS.len() {
            self.open = Some(i);
            self.item = None;
            self.sub = None;
        }
    }

    /// The submenu items of the currently highlighted top item, if it has one.
    #[must_use]
    pub fn submenu_items(&self) -> Option<&'static [Item]> {
        let i = self.open?;
        let it = self.item?;
        MENUS[i].items[it].submenu
    }

    /// Move to the previous top-level menu; or, if a submenu is open, close it.
    pub fn left(&mut self) {
        let Some(i) = self.open else { return };
        if self.sub.is_some() {
            self.sub = None;
            return;
        }
        let n = MENUS.len();
        self.open_index((i + n - 1) % n);
    }

    /// Move to the next top-level menu; or, if the highlighted item has a closed
    /// submenu, open it.
    pub fn right(&mut self) {
        let Some(i) = self.open else { return };
        if self.sub.is_none() {
            if let Some(it) = self.item {
                if let Some(sub) = MENUS[i].items[it].submenu {
                    self.sub = Some(first_selectable(sub));
                    return;
                }
            }
        } else {
            return;
        }
        let n = MENUS.len();
        self.open_index((i + 1) % n);
    }

    /// Highlight the previous selectable item (in the submenu if open). With
    /// nothing highlighted yet, highlights the last selectable item.
    pub fn up(&mut self) {
        let Some(i) = self.open else { return };
        let items = MENUS[i].items;
        match self.item {
            Some(it) => {
                if let (Some(sidx), Some(sub)) = (self.sub, items[it].submenu) {
                    self.sub = Some(prev_selectable(sub, sidx));
                } else {
                    self.item = Some(prev_selectable(items, it));
                }
            }
            None => self.item = Some(prev_selectable(items, 0)),
        }
    }

    /// Highlight the next selectable item (in the submenu if open). With nothing
    /// highlighted yet, highlights the first selectable item.
    pub fn down(&mut self) {
        let Some(i) = self.open else { return };
        let items = MENUS[i].items;
        match self.item {
            Some(it) => {
                if let (Some(sidx), Some(sub)) = (self.sub, items[it].submenu) {
                    self.sub = Some(next_selectable(sub, sidx));
                } else {
                    self.item = Some(next_selectable(items, it));
                }
            }
            None => self.item = Some(first_selectable(items)),
        }
    }

    /// Activate the highlighted item: open its submenu (returning `None`) or
    /// return the leaf action to run. Does nothing when nothing is highlighted.
    pub fn enter(&mut self) -> Option<&'static str> {
        let i = self.open?;
        let items = MENUS[i].items;
        let it_idx = self.item?;
        if let Some(sidx) = self.sub {
            let sub = items[it_idx].submenu?;
            let it = &sub[sidx];
            return (!it.is_separator()).then_some(it.action);
        }
        let it = &items[it_idx];
        if let Some(sub) = it.submenu {
            self.sub = Some(first_selectable(sub));
            return None;
        }
        (!it.is_separator()).then_some(it.action)
    }

    /// Type-ahead: highlight the next item whose label starts with `c` (cycling
    /// from the current selection), within the open submenu if one is open, else
    /// the top dropdown. With nothing highlighted yet, searches from the top.
    /// Lets the user press e.g. `S`, `S` to step Save → Save As.
    pub fn type_ahead(&mut self, c: char) {
        let Some(i) = self.open else { return };
        let items = MENUS[i].items;
        if let Some(it) = self.item {
            if let (Some(sidx), Some(sub)) = (self.sub, items[it].submenu) {
                if let Some(j) = label_starting(sub, sidx, c) {
                    self.sub = Some(j);
                }
            } else if let Some(j) = label_starting(items, it, c) {
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
        let items = MENUS[i].items;
        let it_idx = self.item?;
        if let Some(sidx) = self.sub {
            let sub = items[it_idx].submenu?;
            let it = &sub[sidx];
            return (!it.is_separator()).then_some(it.action);
        }
        let it = &items[it_idx];
        if it.has_submenu() {
            return None;
        }
        (!it.is_separator()).then_some(it.action)
    }
}
