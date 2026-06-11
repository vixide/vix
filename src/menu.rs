//! Top menu bar with keyboard-navigable dropdowns.
//!
//! Menu items carry an `action` string that `App::run_action` dispatches; the
//! command palette reuses the very same action names. Display text is stored as
//! an i18n key (see `locales/`) and translated at render time via [`Item::label`]
//! and [`MenuDef::title`], so the bar follows the active locale.

/// A single dropdown entry.
pub struct Item {
    /// i18n key for the displayed label (e.g. `"menu.item.file.new"`).
    pub label: &'static str,
    /// Action identifier dispatched by `App::run_action` (e.g. `"file.new"`).
    pub action: &'static str,
    /// Keyboard shortcut shown right-aligned; never translated.
    pub shortcut: &'static str,
}

/// Sentinel `action` marking a non-selectable separator row in a dropdown.
pub const SEPARATOR: &str = "menu.separator";

impl Item {
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
}

/// A dropdown separator (divider line).
const SEP: Item = Item { label: "", action: SEPARATOR, shortcut: "" };

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
    Item { label: "menu.item.file.new", action: "file.new", shortcut: "Ctrl+N" },
    SEP,
    Item { label: "menu.item.file.open", action: "file.open", shortcut: "Ctrl+O" },
    Item { label: "menu.item.file.open_recent", action: "file.open_recent", shortcut: "Ctrl+Shift+O" },
    Item { label: "menu.item.file.save", action: "file.save", shortcut: "Ctrl+S" },
    Item { label: "menu.item.file.save_as", action: "file.save_as", shortcut: "Ctrl+Shift+S" },
    SEP,
    Item { label: "menu.item.file.close", action: "file.close", shortcut: "Ctrl+W" },
    SEP,
    Item { label: "menu.item.file.quit", action: "file.quit", shortcut: "Ctrl+Q" },
];

const EDIT: &[Item] = &[
    Item { label: "menu.item.edit.undo", action: "edit.undo", shortcut: "Ctrl+Z" },
    Item { label: "menu.item.edit.redo", action: "edit.redo", shortcut: "Ctrl+Y" },
    SEP,
    Item { label: "menu.item.edit.cut", action: "edit.cut", shortcut: "Ctrl+X" },
    Item { label: "menu.item.edit.copy", action: "edit.copy", shortcut: "Ctrl+C" },
    Item { label: "menu.item.edit.paste", action: "edit.paste", shortcut: "Ctrl+V" },
    SEP,
    Item { label: "menu.item.edit.toggle_comment", action: "edit.toggle_comment", shortcut: "Ctrl+/" },
    SEP,
    Item { label: "menu.item.edit.find", action: "edit.find", shortcut: "Ctrl+F" },
    Item { label: "menu.item.edit.replace", action: "edit.replace", shortcut: "Ctrl+R" },
];

const VIEW: &[Item] = &[
    Item { label: "menu.item.view.theme", action: "view.theme", shortcut: "" },
    Item { label: "menu.item.view.locale", action: "view.locale", shortcut: "" },
    Item { label: "menu.item.view.keyway", action: "view.keyway", shortcut: "" },
    SEP,
    Item { label: "menu.item.view.left_dock", action: "view.left_dock", shortcut: "Ctrl+B" },
    Item { label: "menu.item.view.right_dock", action: "view.right_dock", shortcut: "" },
    SEP,
    Item { label: "menu.item.view.line_numbers", action: "view.line_numbers", shortcut: "" },
    Item { label: "menu.item.view.whitespace", action: "view.whitespace", shortcut: "" },
    Item { label: "menu.item.view.soft_wrap", action: "view.soft_wrap", shortcut: "" },
];

const VIX: &[Item] = &[
    Item { label: "menu.item.vix.about", action: "vix.about", shortcut: "" },
    Item { label: "menu.item.vix.website", action: "vix.website", shortcut: "" },
    Item { label: "menu.item.vix.email", action: "vix.email", shortcut: "" },
];

const TOOLS: &[Item] = &[
    Item { label: "menu.item.tools.calendar", action: "tools.calendar", shortcut: "" },
    Item { label: "menu.item.tools.nerd_palette", action: "tools.nerd_palette", shortcut: "" },
    SEP,
    Item { label: "menu.item.tools.palette", action: "tools.palette", shortcut: "Ctrl+P" },
];

const HELP: &[Item] = &[
    Item { label: "menu.item.help.shortcuts", action: "help.shortcuts", shortcut: "F1" },
];

/// The full menu bar, left to right.
pub const MENUS: &[MenuDef] = &[
    MenuDef { name: "menu.vix", items: VIX },
    MenuDef { name: "menu.file", items: FILE },
    MenuDef { name: "menu.edit", items: EDIT },
    MenuDef { name: "menu.view", items: VIEW },
    MenuDef { name: "menu.tools", items: TOOLS },
    MenuDef { name: "menu.help", items: HELP },
];

/// Open/highlight state for the menu bar.
#[derive(Default)]
pub struct Menu {
    /// Which top-level menu is open, if any.
    pub open: Option<usize>,
    /// Highlighted item within the open dropdown.
    pub item: usize,
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
            self.open = Some(0);
            self.item = 0;
        }
    }

    /// Close any open dropdown.
    pub fn close(&mut self) {
        self.open = None;
        self.item = 0;
    }

    /// Open the menu at index `i` (no-op if out of range).
    pub fn open_index(&mut self, i: usize) {
        if i < MENUS.len() {
            self.open = Some(i);
            self.item = 0;
        }
    }

    /// Move to the previous top-level menu, wrapping around.
    pub fn left(&mut self) {
        if let Some(i) = self.open {
            self.open = Some((i + MENUS.len() - 1) % MENUS.len());
            self.item = 0;
        }
    }

    /// Move to the next top-level menu, wrapping around.
    pub fn right(&mut self) {
        if let Some(i) = self.open {
            self.open = Some((i + 1) % MENUS.len());
            self.item = 0;
        }
    }

    /// Highlight the previous selectable item, skipping separators and wrapping.
    pub fn up(&mut self) {
        if let Some(i) = self.open {
            let items = MENUS[i].items;
            let len = items.len();
            let mut j = self.item;
            for _ in 0..len {
                j = (j + len - 1) % len;
                if !items[j].is_separator() {
                    break;
                }
            }
            self.item = j;
        }
    }

    /// Highlight the next selectable item, skipping separators and wrapping.
    pub fn down(&mut self) {
        if let Some(i) = self.open {
            let items = MENUS[i].items;
            let len = items.len();
            let mut j = self.item;
            for _ in 0..len {
                j = (j + 1) % len;
                if !items[j].is_separator() {
                    break;
                }
            }
            self.item = j;
        }
    }

    /// The action of the highlighted item, unless it is a separator.
    #[must_use]
    pub fn selected_action(&self) -> Option<&'static str> {
        self.open.and_then(|i| {
            let it = &MENUS[i].items[self.item];
            (!it.is_separator()).then_some(it.action)
        })
    }
}
