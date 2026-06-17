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
    Item::leaf("menu.item.edit.recent_locations", "nav.recent_locations", ""),
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

/// Editor split commands, grouped under View → Split.
const VIEW_SPLIT: &[Item] = &[
    Item::leaf("menu.item.view.split_vertical", "view.split_vertical", ""),
    Item::leaf("menu.item.view.split_horizontal", "view.split_horizontal", ""),
    Item::leaf("menu.item.view.focus_other_pane", "view.focus_other_pane", "F6"),
    Item::leaf("menu.item.view.unsplit", "view.unsplit", ""),
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

/// Keyboard navigation styles, grouped under View → Keymap. The labels are the
/// proper-noun keymap names (not translated); the actions carry the keymap id
/// after `view.keymap:`. Kept in sync with `vix_keymap_model::KEYMAPS` (a unit
/// test guards the ids).
const VIEW_KEYMAP: &[Item] = &[
    Item::leaf("Apple", "view.keymap:apple", ""),
    Item::leaf("macOS VSCode", "view.keymap:vscode", ""),
    Item::leaf("Emacs", "view.keymap:emacs", ""),
    Item::leaf("Vim", "view.keymap:vim", ""),
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

/// UUID versions, grouped under Tools → Generate → UUID. Labels are the bare
/// version digit (RFC 4122 / RFC 9562 v1–v8).
const TOOLS_GENERATE_UUID: &[Item] = &[
    Item::leaf("1", "tools.generate.uuid.v1", ""),
    Item::leaf("2", "tools.generate.uuid.v2", ""),
    Item::leaf("3", "tools.generate.uuid.v3", ""),
    Item::leaf("4", "tools.generate.uuid.v4", ""),
    Item::leaf("5", "tools.generate.uuid.v5", ""),
    Item::leaf("6", "tools.generate.uuid.v6", ""),
    Item::leaf("7", "tools.generate.uuid.v7", ""),
    Item::leaf("8", "tools.generate.uuid.v8", ""),
];

/// Generators, grouped under Tools → Generate.
const TOOLS_GENERATE: &[Item] = &[
    Item::sub("menu.item.tools.generate.uuid", TOOLS_GENERATE_UUID),
    Item::leaf("menu.item.tools.generate.zid", "tools.generate.zid", ""),
];

/// Checksum digests, grouped under Tools → Checksum. Each replaces the selection
/// (or whole buffer) with its hex digest.
const TOOLS_CHECKSUM: &[Item] = &[
    Item::leaf("menu.item.tools.checksum.sha256", "tools.checksum.sha256", ""),
    Item::leaf("menu.item.tools.checksum.sha512", "tools.checksum.sha512", ""),
];

// Tools → Convert: each entry converts the selection (or whole buffer). Format
// names (CSV/TSV/JSON/…) are shown literally; only Encode/Decode are translated.
const TOOLS_CONVERT_BASE64: &[Item] = &[
    Item::leaf("menu.item.tools.convert.encode", "tools.convert.base64.encode", ""),
    Item::leaf("menu.item.tools.convert.decode", "tools.convert.base64.decode", ""),
];
const TOOLS_CONVERT_URL: &[Item] = &[
    Item::leaf("menu.item.tools.convert.encode", "tools.convert.url.encode", ""),
    Item::leaf("menu.item.tools.convert.decode", "tools.convert.url.decode", ""),
];
const TOOLS_CONVERT_CSV: &[Item] = &[
    Item::leaf("JSON", "tools.convert.csv.json", ""),
    Item::leaf("TSV", "tools.convert.csv.tsv", ""),
];
const TOOLS_CONVERT_TSV: &[Item] = &[
    Item::leaf("CSV", "tools.convert.tsv.csv", ""),
    Item::leaf("JSON", "tools.convert.tsv.json", ""),
];
const TOOLS_CONVERT_JSON: &[Item] = &[
    Item::leaf("CSV", "tools.convert.json.csv", ""),
    Item::leaf("TSV", "tools.convert.json.tsv", ""),
    Item::leaf("YAML", "tools.convert.json.yaml", ""),
    Item::leaf("TOML", "tools.convert.json.toml", ""),
];
const TOOLS_CONVERT_TOML: &[Item] = &[Item::leaf("JSON", "tools.convert.toml.json", "")];
const TOOLS_CONVERT_YAML: &[Item] = &[Item::leaf("JSON", "tools.convert.yaml.json", "")];
const TOOLS_CONVERT_HTML: &[Item] = &[Item::leaf("Markdown", "tools.convert.html.markdown", "")];
const TOOLS_CONVERT_MARKDOWN: &[Item] = &[Item::leaf("HTML", "tools.convert.markdown.html", "")];

/// Converters, grouped under Tools → Convert.
const TOOLS_CONVERT: &[Item] = &[
    Item::sub("Base64", TOOLS_CONVERT_BASE64),
    Item::sub("CSV", TOOLS_CONVERT_CSV),
    Item::sub("HTML", TOOLS_CONVERT_HTML),
    Item::sub("JSON", TOOLS_CONVERT_JSON),
    Item::sub("Markdown", TOOLS_CONVERT_MARKDOWN),
    Item::sub("TOML", TOOLS_CONVERT_TOML),
    Item::sub("TSV", TOOLS_CONVERT_TSV),
    Item::sub("URL", TOOLS_CONVERT_URL),
    Item::sub("YAML", TOOLS_CONVERT_YAML),
];

const TOOLS: &[Item] = &[
    Item::leaf("menu.item.tools.palette", "tools.palette", "Ctrl P"),
    Item::sub("menu.item.tools.lsp", TOOLS_LSP),
    Item::sub("menu.item.tools.generate", TOOLS_GENERATE),
    Item::sub("menu.item.tools.checksum", TOOLS_CHECKSUM),
    Item::sub("menu.item.tools.convert", TOOLS_CONVERT),
    Item::leaf("menu.item.tools.color_converter", "tools.color_converter", ""),
    Item::leaf("menu.item.tools.convert.unit", "tools.convert.unit", ""),
    Item::leaf("menu.item.tools.calculator", "tools.calculator", ""),
    Item::leaf("menu.item.tools.pomodoro", "tools.pomodoro", ""),
    SEP,
    Item::leaf("menu.item.tools.text_info", "tools.text_info", ""),
    Item::leaf("menu.item.tools.file_info", "tools.file_info", ""),
    Item::leaf("menu.item.tools.dashboard", "tools.dashboard", ""),
    Item::leaf("menu.item.tools.system_info", "tools.system_info", ""),
    SEP,
    Item::leaf("menu.item.tools.run_command", "tools.run_command", ""),
    Item::leaf("menu.item.tools.cancel_command", "tools.cancel_command", ""),
    SEP,
    Item::leaf("menu.item.tools.contacts", "tools.contacts", ""),
    Item::leaf("menu.item.tools.calendar", "tools.calendar", ""),
    Item::leaf("menu.item.tools.clock", "tools.clock", ""),
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

const GIT: &[Item] = &[
    Item::leaf("menu.item.git.status", "git.status", ""),
    Item::leaf("menu.item.git.changes", "git.changes", ""),
    Item::sub("menu.item.git.log", GIT_LOG),
    Item::leaf("menu.item.git.grep", "git.grep", ""),
    Item::leaf("menu.item.git.blame", "git.blame", ""),
    SEP,
    Item::sub("menu.item.git.branch", GIT_BRANCH),
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
    let items: Vec<Item> = vix_locale_model::LOCALES
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
    let mut zones: Vec<&'static vix_time_zone_model::Zone> =
        vix_time_zone_model::ZONES.iter().collect();
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
            // Open the third level if the highlighted submenu row has one.
            if self.subsubmenu_items().is_some() {
                self.subsub_open = true;
                self.subsub = None;
            }
            return;
        }
        if let Some(it) = self.item {
            if menus()[i].items[it].submenu.is_some() {
                self.sub_open = true;
                self.sub = None;
                return;
            }
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
        match self.item {
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
        let model: Vec<&str> = vix_keymap_model::KEYMAPS.iter().map(|k| k.id).collect();
        assert_eq!(ids, model);
    }

    /// Index of the Tools menu and of its Generate item, derived from the live
    /// menu so the test is independent of ordering.
    fn tools_and_generate() -> (usize, usize) {
        let tools = menus().iter().position(|m| m.name == "menu.tools").expect("tools menu");
        let gen = menus()[tools]
            .items
            .iter()
            .position(|it| it.label == "menu.item.tools.generate")
            .expect("generate item");
        (tools, gen)
    }

    /// Tools → Generate → UUID offers exactly v1…v8, dispatching the matching
    /// `tools.generate.uuid.vN` actions.
    #[test]
    fn generate_uuid_submenu_lists_all_versions() {
        let actions: Vec<&str> = TOOLS_GENERATE_UUID.iter().map(|it| it.action).collect();
        assert_eq!(
            actions,
            (1..=8).map(|n| format!("tools.generate.uuid.v{n}")).collect::<Vec<_>>()
        );
    }

    /// Keyboard navigation can descend all three levels: Tools → Generate → UUID
    /// → "4" and `enter` returns the v4 action; `left` walks back up level by level.
    #[test]
    fn three_level_navigation_reaches_uuid_leaf() {
        let (tools, gen) = tools_and_generate();
        let mut m = Menu::default();
        m.open_index(tools);
        m.item = Some(gen);
        m.right(); // open Generate
        assert!(m.sub_open && !m.subsub_open);
        // Highlight the UUID submenu row (index 0) and open it.
        m.sub = Some(0);
        m.right();
        assert!(m.subsub_open, "third level should be open");
        // Step down to the 4th version and activate it.
        for _ in 0..4 {
            m.down();
        }
        assert_eq!(m.enter(), Some("tools.generate.uuid.v4"));
        // Walking left collapses one level at a time.
        m.left();
        assert!(!m.subsub_open && m.sub_open);
        m.left();
        assert!(!m.sub_open);
    }
}
