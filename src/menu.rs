//! Top menu bar with keyboard-navigable dropdowns.
//!
//! Menu items carry an `action` string that `App::run_action` dispatches; the
//! command palette reuses the very same action names.

pub struct Item {
    pub label: &'static str,
    pub action: &'static str,
    pub shortcut: &'static str,
}

pub struct MenuDef {
    pub name: &'static str,
    pub items: &'static [Item],
}

const FILE: &[Item] = &[
    Item { label: "New", action: "file.new", shortcut: "Ctrl+N" },
    Item { label: "Open\u{2026}", action: "file.open", shortcut: "Ctrl+O" },
    Item { label: "Save", action: "file.save", shortcut: "Ctrl+S" },
    Item { label: "Save As\u{2026}", action: "file.save_as", shortcut: "Ctrl+Shift+S" },
    Item { label: "Close", action: "file.close", shortcut: "Ctrl+W" },
    Item { label: "Quit", action: "file.quit", shortcut: "Ctrl+Q" },
];

const EDIT: &[Item] = &[
    Item { label: "Undo", action: "edit.undo", shortcut: "Ctrl+Z" },
    Item { label: "Redo", action: "edit.redo", shortcut: "Ctrl+Y" },
    Item { label: "Cut", action: "edit.cut", shortcut: "Ctrl+X" },
    Item { label: "Copy", action: "edit.copy", shortcut: "Ctrl+C" },
    Item { label: "Paste", action: "edit.paste", shortcut: "Ctrl+V" },
    Item { label: "Find", action: "edit.find", shortcut: "Ctrl+F" },
    Item { label: "Find & Replace", action: "edit.replace", shortcut: "Ctrl+R" },
];

const TOOLS: &[Item] = &[
    Item { label: "Calendar", action: "tools.calendar", shortcut: "" },
    Item { label: "Command Palette", action: "tools.palette", shortcut: "Ctrl+P" },
    Item { label: "Toggle Line Numbers", action: "tools.line_numbers", shortcut: "" },
    Item { label: "Toggle Explorer", action: "view.explorer", shortcut: "Ctrl+B" },
    Item { label: "Toggle Messages", action: "view.messages", shortcut: "" },
];

const HELP: &[Item] = &[
    Item { label: "Website", action: "help.website", shortcut: "" },
    Item { label: "Email Us", action: "help.email", shortcut: "" },
    Item { label: "About STRIDE", action: "help.about", shortcut: "" },
];

pub const MENUS: &[MenuDef] = &[
    MenuDef { name: "File", items: FILE },
    MenuDef { name: "Edit", items: EDIT },
    MenuDef { name: "Tools", items: TOOLS },
    MenuDef { name: "Help", items: HELP },
];

#[derive(Default)]
pub struct Menu {
    /// Which top-level menu is open, if any.
    pub open: Option<usize>,
    /// Highlighted item within the open dropdown.
    pub item: usize,
}

impl Menu {
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub fn toggle(&mut self) {
        if self.open.is_some() {
            self.close();
        } else {
            self.open = Some(0);
            self.item = 0;
        }
    }

    pub fn close(&mut self) {
        self.open = None;
        self.item = 0;
    }

    pub fn open_index(&mut self, i: usize) {
        if i < MENUS.len() {
            self.open = Some(i);
            self.item = 0;
        }
    }

    pub fn left(&mut self) {
        if let Some(i) = self.open {
            self.open = Some((i + MENUS.len() - 1) % MENUS.len());
            self.item = 0;
        }
    }

    pub fn right(&mut self) {
        if let Some(i) = self.open {
            self.open = Some((i + 1) % MENUS.len());
            self.item = 0;
        }
    }

    pub fn up(&mut self) {
        if let Some(i) = self.open {
            let len = MENUS[i].items.len();
            self.item = (self.item + len - 1) % len;
        }
    }

    pub fn down(&mut self) {
        if let Some(i) = self.open {
            let len = MENUS[i].items.len();
            self.item = (self.item + 1) % len;
        }
    }

    /// The action of the highlighted item, if a menu is open.
    pub fn selected_action(&self) -> Option<&'static str> {
        self.open.map(|i| MENUS[i].items[self.item].action)
    }
}
