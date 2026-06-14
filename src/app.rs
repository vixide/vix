//! Application state and event handling.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use include_dir::{include_dir, Dir};
use ratatui::layout::Rect;
use vix_editor::actions::{
    Copy as CopyAction, Cut as CutAction, Paste as PasteAction, Redo as RedoAction,
    ToggleComment, Undo as UndoAction,
};
use vix_editor::selection::Selection;
use ratatui_image::picker::Picker;
use regex::Regex;

use crate::editor::{is_image_path, Editor, Tab, SEARCH_MARK};
use crate::explorer::Explorer;
use crate::menu::{menus, Menu};
use crate::messages::{Level, Messages};
use crate::palette::{self, Action as PAction, Entry, Mode as PMode, Palette};
use crate::workspace_search::{Hit, WorkspaceSearch};
use crate::query::{Decision, QueryReplace};
use crate::search::{Field, SearchBar};
use crate::settings::Settings;

/// The repo's `themes/` directory, embedded into the binary so its themes are
/// available in the chooser without the user installing anything.
static BUNDLED_THEMES: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/themes");

/// Parse every bundled `*.json` theme. Malformed files are skipped.
fn bundled_themes() -> Vec<crate::theme::CustomTheme> {
    BUNDLED_THEMES
        .files()
        .filter(|f| f.path().extension().and_then(|e| e.to_str()) == Some("json"))
        .filter_map(|f| f.contents_utf8().and_then(vix_theme_model::parse_theme))
        .collect()
}

/// Which dock is being resized by an in-progress edge drag.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DockResize {
    /// The left dock (explorer); drag its right edge.
    Left,
    /// The right dock (messages); drag its left edge.
    Right,
    /// The bottom dock; drag its top edge.
    Bottom,
}

/// Which pane currently has keyboard focus.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    /// The center editor.
    Editor,
    /// The left file explorer.
    Explorer,
    /// The right message drawer.
    Messages,
    /// The bottom dock (log/output/data panel).
    BottomDock,
}

/// Which kind of single-line prompt is open.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    /// Open-file prompt.
    Open,
    /// Save-as prompt.
    SaveAs,
    /// Rename the active file (input seeded with its current name).
    Rename,
    /// Run a shell command, streaming its output to the bottom dock.
    RunCommand,
    /// Search the workspace, listing hits in the bottom dock (click-to-jump).
    SearchToDock,
    /// Enter a git commit message for the staged changes.
    GitCommit,
    /// Enter a name for a new topic branch to create and switch to.
    GitNewBranch,
    /// Enter a repository URL to clone into the workspace.
    GitClone,
    /// Enter the file-explorer "include" path regex filter.
    ExplorerInclude,
    /// Enter the file-explorer "exclude" path regex filter.
    ExplorerExclude,
}

/// A single-line input prompt (open / save-as).
pub struct Prompt {
    /// Which prompt this is.
    pub kind: PromptKind,
    /// Title shown in the prompt border.
    pub title: String,
    /// Current input text.
    pub input: String,
    /// Case-sensitive matching (`Alt+C`); only used by `SearchToDock`.
    pub case_sensitive: bool,
    /// Regex matching (`Alt+R`); only used by `SearchToDock`.
    pub regex: bool,
}

impl Prompt {
    /// A prompt of `kind` with the given border `title` and empty input.
    fn new(kind: PromptKind, title: String) -> Self {
        Prompt { kind, title, input: String::new(), case_sensitive: false, regex: false }
    }

    /// Set the initial input text.
    fn with_input(mut self, input: String) -> Self {
        self.input = input;
        self
    }
}

/// Output from a running command, streamed from its reader thread.
enum CmdMsg {
    /// One line of merged stdout/stderr.
    Line(String),
    /// The command finished with this exit code (`None` if killed/unknown).
    Done(Option<i32>),
}

/// A computed workspace-dashboard metric, sent from a background thread.
enum DashMsg {
    /// Human-readable disk usage from `du`.
    Disk(String),
    /// Recursive file count under the workspace root.
    Files(u64),
    /// Commit count reachable from HEAD.
    Commits(u64),
}

/// A command running in a background thread, streaming into the bottom dock.
struct RunningCommand {
    /// Receiver for the reader thread's output.
    rx: std::sync::mpsc::Receiver<CmdMsg>,
    /// The child process, shared so it can be reaped by the reader and killed by
    /// the app.
    child: std::sync::Arc<std::sync::Mutex<std::process::Child>>,
}

/// Result of a background AI text transform whose output replaces editor text.
enum AiMsg {
    /// The CLI finished successfully, carrying its full stdout.
    Done(String),
    /// The CLI failed (non-zero exit, or it died before producing output).
    Failed,
}

/// Where a finished AI transform should write its result.
#[derive(Clone, Copy)]
enum AiTarget {
    /// Replace the whole buffer.
    Whole,
    /// Replace this character range `[start, end)`.
    Range(usize, usize),
}

/// A background AI transform (e.g. Annotate, Improve) whose captured output
/// replaces the selection — or whole buffer — it was launched from.
struct AiReplace {
    /// Receiver for the captured result.
    rx: std::sync::mpsc::Receiver<AiMsg>,
    /// Index of the tab to write the result back into.
    tab: usize,
    /// The range (or whole buffer) to replace when the result arrives.
    target: AiTarget,
    /// Localized action label, for status messages.
    label: String,
}

/// An in-progress paste, processed one source at a time so a name conflict can
/// pause for an (o)verwrite / (s)kip / (c)ancel decision.
pub struct PasteOp {
    /// Destination directory.
    pub target: PathBuf,
    /// Whether this is a cut (move) rather than a copy.
    pub cut: bool,
    /// Remaining sources to process.
    pub queue: VecDeque<PathBuf>,
    /// Overwrite every conflict without asking.
    pub overwrite_all: bool,
    /// Skip every conflict without asking.
    pub skip_all: bool,
    /// The source currently awaiting a conflict decision, if any.
    pub conflict: Option<PathBuf>,
}

/// A yes/no confirmation (currently only used for delete).
pub struct Confirm {
    /// Prompt text.
    pub message: String,
    /// Paths the confirmed action will act on.
    pub paths: Vec<PathBuf>,
}

/// What an [`UnsavedPrompt`] is guarding: closing the active tab, or quitting the
/// whole program (which walks every dirty tab in turn).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnsavedMode {
    /// Closing the active tab.
    CloseTab,
    /// Quitting; each dirty tab is resolved before the program exits.
    Quit,
}

/// The git changes panel: a list of changed files with stage/unstage/commit
/// actions. The file list is read live from the cached `git status`.
pub struct GitPanel {
    /// Index of the highlighted row in the changed-files list.
    pub selected: usize,
}

/// One row of the right-click context menu: an i18n label key and the action to
/// run (or [`SEP_ACTION`] for a separator).
type ContextItem = (&'static str, &'static str);

/// Sentinel action marking a context-menu separator.
const SEP_ACTION: &str = "menu.separator";

/// The editor right-click context menu items (clipboard / selection / find),
/// dispatched through [`App::run_action`].
pub const CONTEXT_ITEMS: &[ContextItem] = &[
    ("menu.item.edit.cut", "edit.cut"),
    ("menu.item.edit.copy", "edit.copy"),
    ("menu.item.edit.paste", "edit.paste"),
    ("menu.separator", SEP_ACTION),
    ("menu.item.edit.select_all", "edit.select_all"),
    ("menu.item.edit.select_more", "edit.select_more"),
    ("menu.item.edit.select_less", "edit.select_less"),
    ("menu.separator", SEP_ACTION),
    ("menu.item.edit.find", "edit.find"),
    ("menu.item.edit.find_next", "edit.find_next"),
    ("menu.item.edit.find_prev", "edit.find_prev"),
];

/// Right-click context-menu overlay state: where it is and which row is selected.
pub struct ContextMenu {
    /// Highlighted row index into [`CONTEXT_ITEMS`].
    pub selected: usize,
    /// Top-left screen position (clamped on render).
    pub x: u16,
    /// Top-left screen position.
    pub y: u16,
}

/// The branch switcher: a list of local branches to check out.
pub struct BranchChooser {
    /// Local branch names (current first).
    pub branches: Vec<String>,
    /// Index of the highlighted branch.
    pub selected: usize,
    /// When true, the chosen branch is merged into the current branch; otherwise
    /// it is checked out.
    pub merge: bool,
}

/// The spell-suggestion popup (Ctrl+;): corrections for the misspelled word at
/// the cursor, plus Add-to-dictionary / Ignore actions.
pub struct SpellSuggest {
    /// The misspelled word.
    pub word: String,
    /// Its char range `[start, end)` in the buffer.
    pub span: (usize, usize),
    /// Suggested corrections (may be empty).
    pub suggestions: Vec<String>,
    /// Index of the highlighted suggestion.
    pub selected: usize,
}

/// A modal "you have unsaved changes" prompt offering **Save**, **Don't Save**,
/// and **Cancel**. Raised when closing a tab or quitting with a dirty buffer.
pub struct UnsavedPrompt {
    /// Whether this prompt guards a tab close or a quit.
    pub mode: UnsavedMode,
    /// Display name of the buffer being asked about.
    pub name: String,
}

/// A modal info dialog: a title, a body, and a single **Ok** button. Used by the
/// Vix menu's About / Website / Email items.
///
/// When `editor` is `Some`, the body is shown in a selectable/copyable text field
/// (Website/Email — select with the mouse or keyboard, `Ctrl+C` to copy) and only
/// Esc / clicking Ok closes it. When `None` it is a plain text dialog (About),
/// dismissed with Enter, Esc, or a click.
pub struct Dialog {
    /// Title shown in the dialog border.
    pub title: String,
    /// Body text (version string, URL, or email address).
    pub body: String,
    /// Selectable/copyable text field for the body, when applicable.
    pub editor: Option<crate::editor::CodeEditor>,
}

/// Locale chooser overlay state (View -> Locale), re-exported from
/// [`vix_locale_chooser`]. Moving the selection previews the language live;
/// Enter commits and persists it, Esc reverts.
pub use vix_locale_chooser::Chooser as LocaleChooser;

/// Time Zone chooser overlay state (Tools -> Time Zone), re-exported from
/// [`vix_time_zone_chooser`]. A filterable list; Enter sets the application-wide
/// active zone in [`vix_time_zone_model`] and persists it, Esc cancels.
pub use vix_time_zone_chooser::Chooser as TimeZoneChooser;

/// Nerd Font palette overlay state (Tools -> Nerd Font Palette), re-exported from
/// [`vix_nerd_font_picker`]. Arrow keys move within the glyph grid; Enter (or a
/// click) inserts the highlighted glyph into the active editor, Esc closes.
pub use vix_nerd_font_picker::Palette as NerdPalette;

/// ASCII panel overlay state (Tools -> ASCII), re-exported from
/// [`vix_ascii_character_picker`]. Arrow keys move within the table; Enter (or a click)
/// inserts the highlighted character into the active editor, Esc closes.
pub use vix_ascii_character_picker::Panel as AsciiPanel;

/// X11 color palette overlay state (Tools -> X11 Colors), re-exported from
/// [`vix_x11_color_picker`]. Arrow keys move within the table; Enter (or a click)
/// inserts the highlighted color's hex into the active editor, Esc closes.
pub use vix_x11_color_picker::Panel as X11Panel;

/// HTML character palette overlay state (Tools -> HTML Characters), re-exported
/// from [`vix_html_character_picker`]. Arrow keys move within the table; Enter
/// (or a click) inserts the highlighted entity reference into the editor, Esc
/// closes.
pub use vix_html_character_picker::Panel as HtmlPanel;

/// System Information panel overlay state (Tools -> System Information),
/// re-exported from [`vix_system_information_panel`]. Arrow keys move within the
/// table; Enter (or a click) inserts the highlighted value into the active
/// editor, Esc closes.
pub use vix_system_information_panel::Panel as SystemInfoPanel;

/// Workspace dashboard overlay state (Tools -> Workspace Dashboard), re-exported from
/// [`vix_workspace_dashboard_panel`]. Its metrics fill in asynchronously; Esc closes.
pub use vix_workspace_dashboard_panel::Dashboard;

/// First-run welcome overlay state, re-exported from [`vix_welcome_panel`].
/// Scrollable, informational; Esc closes.
pub use vix_welcome_panel::Panel as WelcomePanel;

/// Contact-browser overlay state (Tools -> Contacts), re-exported from
/// [`vix_contact_panel`]. Lists the vCard files in a directory; Enter/click opens
/// the highlighted contact's [`VcardPanel`].
pub use vix_contact_panel::Panel as ContactPanel;

/// Single-vCard view overlay state, re-exported from [`vix_vcard_panel`]. Shows
/// one contact's fields; Enter/click inserts a value, Esc returns to the browser.
pub use vix_vcard_panel::Panel as VcardPanel;

/// File Information overlay state (Tools -> File Information), re-exported from
/// [`vix_file_information_panel`]. Arrow keys move within the table; Enter (or a
/// click) inserts the highlighted value into the active editor, Esc closes.
pub use vix_file_information_panel::Panel as FileInfoPanel;

/// Code-outline overlay state (Ctrl+Shift+O), re-exported from
/// [`vix_outline_panel`]. Lists the active buffer's symbols; Enter/click jumps to
/// one, Esc closes.
pub use vix_outline_panel::Outline;

/// The active keyboard navigation style, derived from `settings.keymap`. It
/// decides how raw key events are dispatched (see [`App::on_key`]): `Apple` uses
/// modifier shortcuts, `Vscode` mirrors VS Code's signature shortcuts, `Emacs`
/// uses `Ctrl` chords, `Vim` is modal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Keymap {
    /// Modifier-key shortcuts (the default), e.g. `Ctrl+O` to open.
    Apple,
    /// VS Code (macOS) shortcuts, e.g. `Ctrl+P` Quick Open, `Ctrl+Shift+P`
    /// Command Palette, `Ctrl+G` Go to Line.
    Vscode,
    /// `Ctrl` chords and the `Ctrl+X` prefix, e.g. `Ctrl+X Ctrl+F` to open.
    Emacs,
    /// Modal editing: a Normal mode for motions/commands and an Insert mode.
    Vim,
}

impl Keymap {
    /// Parse a persisted keymap id; anything unrecognized is [`Keymap::Apple`].
    fn from_id(id: &str) -> Self {
        match id {
            "vscode" => Keymap::Vscode,
            "emacs" => Keymap::Emacs,
            "vim" => Keymap::Vim,
            _ => Keymap::Apple,
        }
    }
}

/// A point in the position-history jump list: a file and a 1-based line/column.
#[derive(Clone, PartialEq, Eq)]
pub struct Location {
    /// File the position is in.
    pub path: PathBuf,
    /// 1-based line.
    pub line: usize,
    /// 1-based column.
    pub col: usize,
}

/// Recent-files chooser overlay state (File -> Open Recent). Lists previously
/// opened files; Enter (or a click) reopens the highlighted one.
pub struct RecentChooser {
    /// Recent file paths, most-recent first.
    pub entries: Vec<PathBuf>,
    /// Index of the highlighted entry.
    pub selected: usize,
}

/// Rectangles recorded during rendering, used for mouse hit-testing and for
/// telling the code editor which viewport to scroll within.
#[derive(Default)]
pub struct Layout {
    /// Menu-bar rectangle.
    pub menu: Rect,
    /// Open menu dropdown rectangle (valid while a menu is open).
    pub menu_dropdown: Rect,
    /// Open submenu dropdown rectangle (valid while a submenu is open).
    pub submenu_dropdown: Rect,
    /// Info-dialog text-field rectangle (valid while a text dialog is open).
    pub dialog_body: Rect,
    /// Tab-strip rectangle.
    pub tabs: Rect,
    /// Editor viewport rectangle.
    pub editor: Rect,
    /// Editor vertical-scrollbar rectangle (the column right of the editor text).
    pub scrollbar: Rect,
    /// Explorer pane rectangle.
    pub explorer: Rect,
    /// Message-drawer rectangle.
    pub messages: Rect,
    /// Bottom-dock rectangle (valid while the bottom dock is shown).
    pub bottom_dock: Rect,
    /// Row list rectangle of the open chooser overlay (theme/locale/keymap), so a
    /// click can hit-test which row was picked.
    pub chooser: Rect,
    /// Row-list rectangle of the open Time Zone chooser, so a click can hit-test
    /// which row was picked.
    pub tz_chooser: Rect,
    /// Scrollbar gutter rectangle of the open Time Zone chooser, for click/drag.
    pub tz_scrollbar: Rect,
    /// Glyph-grid rectangle of the open Nerd Font palette, so a click can
    /// hit-test which cell was picked.
    pub nerd_palette: Rect,
    /// Row-list rectangle of the open ASCII panel, so a click can hit-test which
    /// row was picked.
    pub ascii_panel: Rect,
    /// Row-list rectangle of the open X11 color palette, so a click can hit-test
    /// which row was picked.
    pub x11_panel: Rect,
    /// Row-list rectangle of the open HTML character palette, so a click can
    /// hit-test which row was picked.
    pub html_panel: Rect,
    /// Row-list rectangle of the open System Information panel, so a click can
    /// hit-test which row was picked.
    pub system_info: Rect,
    /// Text rectangle of the open welcome panel, for mouse-wheel scrolling.
    pub welcome: Rect,
    /// Row-list rectangle of the open File Information panel, for click hit-testing.
    pub file_info: Rect,
    /// Row-list rectangle of the open contact browser, for click hit-testing.
    pub contacts: Rect,
    /// Row-list rectangle of the open vCard view, for click hit-testing.
    pub vcard: Rect,
    /// Suggestion-list rectangle of the open spell-suggestion popup, so a click
    /// can hit-test which suggestion was picked.
    pub spell_suggest: Rect,
    /// Rectangle of the open right-click context menu, for click hit-testing.
    pub context_menu: Rect,
    /// File-list rectangle of the open git changes panel, so a click can hit-test
    /// which row was picked.
    pub git_panel: Rect,
    /// Status-bar git/branch segment rectangle, so a click opens the Git panel.
    pub git_status_bar: Rect,
    /// Row-list rectangle of the open outline panel, so a click can hit-test a row.
    pub outline: Rect,
    /// Inner content rectangle of the open find / replace box, so a click can
    /// focus the Find or Replace field.
    pub search: Rect,
    /// Inner content rectangle of the open calendar box, so a click can insert a
    /// date-time line or a calendar day.
    pub calendar: Rect,
    /// Inner content rectangle of the open clock box, so a click can hit-test
    /// which time row was picked.
    pub clock: Rect,
}

/// LSP hover tooltip overlay: the text the server returned for the symbol under
/// the cursor.
pub struct HoverPopup {
    /// Hover text (plain text / lightly-rendered markdown).
    pub text: String,
}

/// LSP completion overlay: the candidate list and the highlighted row.
pub struct CompletionPopup {
    /// Candidate items, in server order.
    pub items: Vec<vix_lsp::CompletionItem>,
    /// Index of the highlighted candidate.
    pub selected: usize,
}

/// The whole application state.
pub struct App {
    /// Workspace root directory.
    pub root: PathBuf,
    /// Tabbed text editor.
    pub editor: Editor,
    /// File explorer pane.
    pub explorer: Explorer,
    /// Message drawer.
    pub messages: Messages,
    /// Menu-bar state.
    pub menu: Menu,
    /// Command palette, when open.
    pub palette: Option<Palette>,
    /// Find / replace toolbar, when open.
    pub search: Option<SearchBar>,
    /// Interactive query-replace session, when active.
    pub query_replace: Option<QueryReplace>,
    /// Workspace-wide search panel, when open.
    pub workspace_search: Option<WorkspaceSearch>,
    /// Single-line prompt, when open.
    pub prompt: Option<Prompt>,
    /// In-progress paste operation, when active.
    pub paste: Option<PasteOp>,
    /// Pending confirmation, when active.
    pub confirm: Option<Confirm>,
    /// Pending unsaved-changes prompt (close tab / quit), when active.
    pub unsaved: Option<UnsavedPrompt>,
    /// Spell-suggestion popup (Ctrl+;), when open.
    pub spell_suggest: Option<SpellSuggest>,
    /// Editor right-click context menu, when open.
    pub context_menu: Option<ContextMenu>,
    /// Git changes panel (stage/unstage/commit), when open.
    pub git_panel: Option<GitPanel>,
    /// Git branch switcher, when open.
    pub branch_chooser: Option<BranchChooser>,
    /// Locale chooser overlay, when open.
    pub locale_chooser: Option<LocaleChooser>,
    /// Time Zone chooser overlay, when open.
    pub time_zone_chooser: Option<TimeZoneChooser>,
    /// Recent-files chooser overlay, when open.
    pub recent_chooser: Option<RecentChooser>,
    /// Nerd Font palette (character picker) overlay, when open.
    pub nerd_palette: Option<NerdPalette>,
    /// ASCII panel (reference table) overlay, when open.
    pub ascii_panel: Option<AsciiPanel>,
    /// X11 color palette overlay, when open.
    pub x11_panel: Option<X11Panel>,
    /// HTML character palette overlay, when open.
    pub html_panel: Option<HtmlPanel>,
    /// System Information panel overlay, when open.
    pub system_info: Option<SystemInfoPanel>,
    /// Workspace Dashboard overlay, when open.
    pub dashboard: Option<Dashboard>,
    /// Receiver for the dashboard's background metric computations.
    dashboard_rx: Option<std::sync::mpsc::Receiver<DashMsg>>,
    /// Code outline overlay, when open.
    pub outline: Option<Outline>,
    /// First-run welcome overlay, when shown.
    pub welcome: Option<WelcomePanel>,
    /// File Information overlay, when open.
    pub file_info: Option<FileInfoPanel>,
    /// Contact-browser overlay, when open.
    pub contacts: Option<ContactPanel>,
    /// Single-vCard view overlay, when open (above the contact browser).
    pub vcard: Option<VcardPanel>,
    /// LSP client: language-server process management and document sync.
    pub lsp: crate::lsp::Lsp,
    /// Last document revision pushed to a language server, keyed by file path, so
    /// edits sync once per change rather than once per frame.
    lsp_synced: std::collections::HashMap<PathBuf, u64>,
    /// LSP hover tooltip overlay, when shown.
    pub hover: Option<HoverPopup>,
    /// LSP completion overlay, when shown.
    pub completion: Option<CompletionPopup>,
    /// Modal info dialog (Vix menu About / Website / Email), when open.
    pub dialog: Option<Dialog>,
    /// Explorer clipboard: paths plus whether this is a cut (move) or copy.
    pub clip: Vec<PathBuf>,
    /// Whether [`App::clip`] holds a cut (move) rather than a copy.
    pub clip_cut: bool,
    /// Position-history jump list (Alt+Left / Alt+Right).
    pub nav_history: Vec<Location>,
    /// Current index into [`App::nav_history`].
    pub nav_idx: usize,
    /// Terminal image picker; `None` until set from a real terminal (so tests
    /// and headless use construct fine), and on terminals without graphics.
    pub picker: Option<Picker>,
    /// Persisted user settings.
    pub settings: Settings,
    /// Which pane has focus.
    pub focus: Focus,
    /// Whether the explorer pane is shown.
    pub show_explorer: bool,
    /// Whether the message drawer is shown.
    pub show_messages: bool,
    /// Whether the bottom status bar is shown.
    pub show_status_bar: bool,
    /// Whether the editor's right-side scroll bar is shown.
    pub show_scrollbar: bool,
    /// Whether the workspace root is a git work tree (checked once at startup).
    pub git_repo: bool,
    /// Cached current git branch (or short hash when detached), when in a repo.
    pub git_branch: Option<String>,
    /// Cached `git status` rows (changed files), refreshed on save / git actions.
    pub git_status: Vec<vix_git::FileStatus>,
    /// Cached HEAD blob text per file path, for the editor diff gutter. Cleared
    /// on save / git actions so it refetches.
    git_head_cache: std::collections::HashMap<PathBuf, String>,
    /// Whether spell-checking (red underline in comments/strings) is enabled.
    pub spellcheck: bool,
    /// Loaded spell checker for the active locale, when spell-checking is on and
    /// a dictionary was found.
    pub speller: Option<vix_spellcheck::SpellChecker>,
    /// Locale the loaded (or last-attempted) [`speller`](Self::speller) is for, so
    /// it is reloaded only on a locale change.
    speller_locale: Option<String>,
    /// Whether the bottom dock (log/output/data panel) is shown.
    pub show_bottom_dock: bool,
    /// Bottom-dock line buffer.
    pub bottom_dock: vix_bottom_dock::BottomDock,
    /// Whether the calendar box is shown.
    pub show_calendar: bool,
    /// Month navigation state for the calendar box.
    pub calendar: crate::calendar::Calendar,
    /// Whether the clock box is shown.
    pub show_clock: bool,
    /// Row-selection state for the clock box.
    pub clock: crate::clock::Clock,
    /// Whether the keyboard-help overlay is shown.
    pub show_help: bool,
    /// Status-bar text.
    pub status: String,
    /// Set to request application exit.
    pub should_quit: bool,
    /// Pane rectangles recorded during the last render.
    pub layout: Layout,
    /// File paths under the workspace root, for the palette file finder.
    file_index: Vec<PathBuf>,
    /// Cursor offset captured when the palette opened, so the `:` go-to-line
    /// preview can revert on cancel and the jump records the true origin.
    palette_origin: Option<usize>,
    /// The most recent search pattern (regex), so Find Next / Find Previous can
    /// repeat it after the find box has closed.
    last_search: Option<String>,
    /// Stack of recently closed file paths, most-recent last, for Reopen Closed
    /// Tab (`Ctrl+Shift+T`). Capped to a small number.
    closed_tabs: Vec<PathBuf>,
    /// The command currently streaming into the bottom dock, if any.
    running_command: Option<RunningCommand>,
    /// A background AI transform whose result will replace editor text, if any.
    ai_replace: Option<AiReplace>,
    /// True while the editor scrollbar thumb is being dragged, so the drag keeps
    /// scrolling even if the pointer drifts off the one-column track.
    scrollbar_active: bool,
    /// Which dock (if any) is being resized by an in-progress edge drag.
    dock_resize: Option<DockResize>,
    /// Emacs keymap: a `Ctrl+X` prefix has been pressed and the next key
    /// completes the chord. Always false in other keymaps.
    emacs_prefix: bool,
    /// Vim keymap: true in Insert mode, false in Normal mode. Meaningless in
    /// other keymaps.
    vim_insert: bool,
    /// Vim keymap: the in-progress `:` command-line text, when the command line
    /// is open.
    vim_cmd: Option<String>,
}

impl App {
    /// Build an app rooted at `root` using the given `settings`.
    ///
    /// The active locale and theme should already be applied by the caller
    /// (see `main`); the theme is (re)applied here so the first buffer is styled
    /// correctly, and the welcome messages are produced in the current locale.
    #[must_use]
    pub fn new(root: PathBuf, settings: Settings) -> Self {
        // Apply the saved theme before building any editor so the first buffer is
        // styled correctly. A theme value that is not a built-in mode is treated
        // as the name of a custom JSON theme.
        Self::apply_saved_theme(&settings.theme);
        // Populate the View → Theme submenu with the available theme names before
        // the menu bar is first rendered.
        let theme_names = vix_theme_model::theme_names(&Self::available_custom_themes());
        crate::menu::set_theme_names(theme_names);
        // Apply the saved time zone so the clock panel and status bar use it.
        vix_time_zone_model::set_active(&settings.time_zone);
        let editor = Editor::new(
            settings.line_numbers,
            settings.show_whitespace,
            settings.soft_wrap,
            settings.indent_string(),
        );
        let mut messages = Messages::default();
        messages.advice(t!("msg.welcome").to_string());
        messages.info(t!("msg.welcome_hint").to_string());

        let lsp = crate::lsp::Lsp::new(
            settings.lsp_enabled,
            settings.lsp_servers.clone(),
            &root,
        );

        App {
            explorer: Explorer::new(root.clone()),
            root,
            editor,
            messages,
            menu: Menu::default(),
            palette: None,
            search: None,
            query_replace: None,
            workspace_search: None,
            prompt: None,
            paste: None,
            confirm: None,
            unsaved: None,
            spell_suggest: None,
            context_menu: None,
            git_panel: None,
            branch_chooser: None,
            locale_chooser: None,
            time_zone_chooser: None,
            recent_chooser: None,
            nerd_palette: None,
            ascii_panel: None,
            x11_panel: None,
            html_panel: None,
            system_info: None,
            dashboard: None,
            dashboard_rx: None,
            outline: None,
            welcome: None,
            file_info: None,
            contacts: None,
            vcard: None,
            lsp,
            lsp_synced: std::collections::HashMap::new(),
            hover: None,
            completion: None,
            dialog: None,
            clip: Vec::new(),
            clip_cut: false,
            nav_history: Vec::new(),
            nav_idx: 0,
            picker: None,
            show_explorer: settings.show_explorer,
            show_messages: settings.show_messages,
            show_status_bar: settings.show_status_bar,
            show_scrollbar: settings.show_scrollbar,
            git_repo: false,
            git_branch: None,
            git_status: Vec::new(),
            git_head_cache: std::collections::HashMap::new(),
            spellcheck: settings.spellcheck,
            speller: None,
            speller_locale: None,
            show_bottom_dock: settings.show_bottom_dock,
            bottom_dock: vix_bottom_dock::BottomDock::with_scrollback(settings.scrollback),
            show_calendar: false,
            calendar: crate::calendar::Calendar::new(),
            show_clock: false,
            clock: crate::clock::Clock::new(),
            show_help: false,
            focus: Focus::Editor,
            status: t!("status.ready").to_string(),
            should_quit: false,
            layout: Layout::default(),
            settings,
            file_index: Vec::new(),
            palette_origin: None,
            last_search: None,
            closed_tabs: Vec::new(),
            running_command: None,
            ai_replace: None,
            scrollbar_active: false,
            dock_resize: None,
            emacs_prefix: false,
            vim_insert: false,
            vim_cmd: None,
        }
    }

    /// On first run, open the welcome screen and mark it seen (persisted on exit)
    /// so it does not reappear. Called by `main` after construction; kept out of
    /// [`App::new`] so tests build a clean, overlay-free app.
    pub fn maybe_show_welcome(&mut self) {
        if !self.settings.welcomed {
            self.welcome = Some(WelcomePanel::open(Self::welcome_lines()));
            self.settings.welcomed = true;
        }
    }

    /// Open a path given on the command line.
    pub fn open_initial(&mut self, path: PathBuf) {
        self.open_path(&path, false);
    }

    // ----- top-level event entry -----------------------------------------

    /// Handle a key event, routing it to the active modal layer or focused pane.
    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        // LSP completion popup captures navigation/accept/cancel keys; any other
        // key dismisses it and falls through to normal handling.
        if self.completion.is_some() && self.completion_key(key) {
            return;
        }
        // A hover tooltip is dismissed by the next keypress (Esc just dismisses).
        if self.hover.is_some() {
            self.hover = None;
            if key.code == KeyCode::Esc {
                return;
            }
        }
        // Modal layers, in priority order.
        if self.welcome.is_some() {
            self.welcome_key(key);
            return;
        }
        if self.show_help {
            match key.code {
                KeyCode::Esc | KeyCode::F(1) | KeyCode::Char('q') => self.show_help = false,
                _ => {}
            }
            return;
        }
        // The info dialog. A plain dialog (About) closes on Enter/Esc/Space/O. A
        // text-field dialog (Website/Email) closes on Esc; Ctrl+C copies the
        // selection, and other keys drive selection/navigation in the field.
        if self.dialog.is_some() {
            let has_field = self.dialog.as_ref().is_some_and(|d| d.editor.is_some());
            if !has_field {
                if matches!(
                    key.code,
                    KeyCode::Enter | KeyCode::Esc | KeyCode::Char(' ' | 'o' | 'O')
                ) {
                    self.dialog = None;
                }
                return;
            }
            if key.code == KeyCode::Esc {
                self.dialog = None;
                return;
            }
            let area = self.dialog_field_area();
            if let Some(ed) = self.dialog.as_mut().and_then(|d| d.editor.as_mut()) {
                if Self::ctrl(&key) && matches!(key.code, KeyCode::Char('c')) {
                    ed.apply(CopyAction {});
                } else {
                    let _ = ed.input(key, &area);
                }
            }
            return;
        }
        // While the calendar box is open it captures left/right to page months.
        if self.show_calendar {
            let ctrl = Self::ctrl(&key);
            let shift = Self::shift(&key);
            match key.code {
                KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => {
                    let sign: i64 = if matches!(key.code, KeyCode::Right | KeyCode::Down) { 1 } else { -1 };
                    if ctrl && shift {
                        self.calendar.move_years(sign); // Ctrl+Shift+arrows: year
                    } else if ctrl {
                        self.calendar.move_months(sign); // Ctrl+arrows: month
                    } else {
                        // Plain arrows move the selected day: Left/Right by a day,
                        // Up/Down by a week.
                        let step = if matches!(key.code, KeyCode::Up | KeyCode::Down) { 7 } else { 1 };
                        self.calendar.move_days(sign * step);
                    }
                }
                KeyCode::Enter => {
                    let text = self.calendar.selected_formatted(Self::locale_date_pattern());
                    let area = self.editor_view();
                    self.editor.insert_str(&text, area);
                    self.show_calendar = false;
                }
                KeyCode::Esc | KeyCode::Char('q') => self.show_calendar = false,
                _ => {}
            }
            return;
        }
        // While the clock box is open it captures up/down to pick a time row and
        // Enter to insert it.
        if self.show_clock {
            match key.code {
                KeyCode::Up => self.clock.up(),
                KeyCode::Down => self.clock.down(),
                KeyCode::Enter => {
                    let now = crate::clock::now_local();
                    if let Some(text) = self.clock.selected_value(&now) {
                        let area = self.editor_view();
                        self.editor.insert_str(&text, area);
                    }
                    self.show_clock = false;
                }
                KeyCode::Esc | KeyCode::Char('q') => self.show_clock = false,
                _ => {}
            }
            return;
        }
        if self.locale_chooser.is_some() {
            self.locale_key(key);
            return;
        }
        if self.time_zone_chooser.is_some() {
            self.time_zone_key(key);
            return;
        }
        if self.recent_chooser.is_some() {
            self.recent_key(key);
            return;
        }
        if self.nerd_palette.is_some() {
            self.nerd_key(key);
            return;
        }
        if self.ascii_panel.is_some() {
            self.ascii_key(key);
            return;
        }
        if self.x11_panel.is_some() {
            self.x11_key(key);
            return;
        }
        if self.html_panel.is_some() {
            self.html_key(key);
            return;
        }
        if self.system_info.is_some() {
            self.system_info_key(key);
            return;
        }
        if self.file_info.is_some() {
            self.file_info_key(key);
            return;
        }
        if self.vcard.is_some() {
            self.vcard_key(key);
            return;
        }
        if self.contacts.is_some() {
            self.contacts_key(key);
            return;
        }
        if self.dashboard.is_some() {
            if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
                self.close_dashboard();
            }
            return;
        }
        if self.outline.is_some() {
            self.outline_key(key);
            return;
        }
        if self.query_replace.is_some() {
            self.qr_key(key);
            return;
        }
        if self.workspace_search.is_some() {
            self.ps_key(key);
            return;
        }
        if self.confirm.is_some() {
            self.confirm_key(key);
            return;
        }
        if self.unsaved.is_some() {
            self.unsaved_key(key);
            return;
        }
        if self.spell_suggest.is_some() {
            self.spell_suggest_key(key);
            return;
        }
        if self.context_menu.is_some() {
            self.context_menu_key(key);
            return;
        }
        if self.git_panel.is_some() {
            self.git_panel_key(key);
            return;
        }
        if self.branch_chooser.is_some() {
            self.branch_key(key);
            return;
        }
        if self.paste.as_ref().is_some_and(|p| p.conflict.is_some()) {
            self.paste_key(key);
            return;
        }
        if self.prompt.is_some() {
            self.prompt_key(key);
            return;
        }
        if self.palette.is_some() {
            self.palette_key(key);
            return;
        }
        if self.search.is_some() {
            self.search_key(key);
            return;
        }
        if self.menu.is_open() {
            self.menu_key(key);
            return;
        }
        // Keymap-specific dispatch. Each keymap first gets a chance to consume the
        // key; `Emacs`/`Vim` then fall back to the shared keys (menu mnemonics and
        // function keys) before the focused pane handles it.
        match self.active_keymap() {
            Keymap::Apple => {
                if self.global_key(key) {
                    return;
                }
            }
            Keymap::Vscode => {
                if self.vscode_key(key) {
                    return;
                }
            }
            Keymap::Emacs => {
                if self.emacs_key(key) || self.global_shared_key(key) {
                    return;
                }
            }
            Keymap::Vim => {
                if self.vim_key(key) || self.global_shared_key(key) {
                    return;
                }
            }
        }
        match self.focus {
            Focus::Editor => self.editor_key(key),
            Focus::Explorer => self.explorer_key(key),
            Focus::Messages => self.messages_key(key),
            Focus::BottomDock => self.bottomdock_key(key),
        }
    }

    /// The keyboard navigation style currently in effect.
    fn active_keymap(&self) -> Keymap {
        Keymap::from_id(&self.settings.keymap)
    }

    /// A short keymap-mode indicator for the status bar (Vim's mode / command
    /// line, or Emacs's pending chord prefix), or `None` when there is nothing to
    /// show (e.g. the Apple keymap).
    #[must_use]
    pub fn mode_indicator(&self) -> Option<String> {
        match self.active_keymap() {
            Keymap::Vim => Some(if let Some(cmd) = &self.vim_cmd {
                format!(":{cmd}")
            } else if self.vim_insert {
                t!("status.vim_insert").to_string()
            } else {
                t!("status.vim_normal").to_string()
            }),
            Keymap::Emacs if self.emacs_prefix => Some("C-x-".to_string()),
            _ => None,
        }
    }

    fn ctrl(key: &KeyEvent) -> bool {
        key.modifiers.contains(KeyModifiers::CONTROL)
    }

    fn alt(key: &KeyEvent) -> bool {
        key.modifiers.contains(KeyModifiers::ALT)
    }

    fn shift(key: &KeyEvent) -> bool {
        key.modifiers.contains(KeyModifiers::SHIFT)
    }

    /// Global shortcuts available when no modal is active (Apple keymap). Returns
    /// true if the key was consumed.
    fn global_key(&mut self, key: KeyEvent) -> bool {
        self.apple_ctrl_key(key) || self.global_shared_key(key)
    }

    /// The Apple keymap's `Ctrl`-letter shortcuts. Returns true if consumed.
    fn apple_ctrl_key(&mut self, key: KeyEvent) -> bool {
        if Self::ctrl(&key) {
            if let KeyCode::Char(c) = key.code {
                match c.to_ascii_lowercase() {
                    'q' => self.run_action("file.quit"),
                    'n' => self.run_action("file.new"),
                    'o' if Self::shift(&key) => self.run_action("file.open_recent"),
                    'o' => self.run_action("file.open"),
                    's' if Self::shift(&key) => self.run_action("file.save_as"),
                    's' => self.run_action("file.save"),
                    'w' if Self::shift(&key) => self.run_action("file.close_all"),
                    'w' => self.run_action("file.close"),
                    't' if Self::shift(&key) => self.run_action("file.reopen_closed"),
                    'p' => self.run_action("tools.palette"),
                    'b' if Self::shift(&key) => self.run_action("nav.outline"),
                    'b' => self.run_action("view.explorer"),
                    'e' => self.toggle_focus_explorer_editor(),
                    'f' if Self::shift(&key) => self.run_action("search.workspace"),
                    'f' => self.run_action("edit.find"),
                    'g' if Self::shift(&key) => self.run_action("edit.find_prev"),
                    'g' => self.run_action("edit.find_next"),
                    'r' if Self::alt(&key) => self.run_action("edit.query_replace"),
                    'r' => self.run_action("edit.replace"),
                    // Many terminals emit the same control byte (0x1F) for
                    // Ctrl+/, Ctrl+7, and Ctrl+_, so accept all three for Comment.
                    '/' | '7' | '_' => self.run_action("edit.toggle_comment"),
                    ']' => self.run_action("edit.match_bracket"),
                    ';' => self.run_action("spell.suggest"),
                    _ => return false,
                }
                return true;
            }
        }
        false
    }

    // ----- keymap: VS Code (macOS) ----------------------------------------

    /// VS Code (macOS) keymap dispatch: VS Code's signature shortcuts — Quick
    /// Open (`Ctrl+P`), Command Palette (`Ctrl+Shift+P`), Go to Symbol
    /// (`Ctrl+Shift+O`), Go to Line (`Ctrl+G`) — plus the familiar editing
    /// chords, then the shared menu mnemonics and function keys. Returns true if
    /// the key was consumed.
    fn vscode_key(&mut self, key: KeyEvent) -> bool {
        self.vscode_ctrl_key(key) || self.global_shared_key(key)
    }

    /// The VS Code keymap's `Ctrl`-key shortcuts (VS Code's `Cmd` bindings, with
    /// `Ctrl` standing in for `Cmd`). Returns true if consumed.
    fn vscode_ctrl_key(&mut self, key: KeyEvent) -> bool {
        if Self::ctrl(&key) {
            if let KeyCode::Char(c) = key.code {
                match c.to_ascii_lowercase() {
                    'q' => self.run_action("file.quit"),
                    'n' => self.run_action("file.new"),
                    's' if Self::shift(&key) => self.run_action("file.save_as"),
                    's' => self.run_action("file.save"),
                    'w' if Self::shift(&key) => self.run_action("file.close_all"),
                    'w' => self.run_action("file.close"),
                    't' if Self::shift(&key) => self.run_action("file.reopen_closed"),
                    'p' if Self::shift(&key) => self.run_action("tools.palette"),
                    'p' => self.run_action("file.open"),
                    'o' if Self::shift(&key) => self.run_action("nav.goto_symbol"),
                    'g' if Self::shift(&key) => self.run_action("edit.find_prev"),
                    'g' => self.open_palette_seeded(":"),
                    'e' if Self::shift(&key) => self.toggle_focus_explorer_editor(),
                    'b' => self.run_action("view.explorer"),
                    'f' if Self::shift(&key) => self.run_action("search.workspace"),
                    'f' => self.run_action("edit.find"),
                    'r' => self.run_action("edit.replace"),
                    // Many terminals emit the same control byte (0x1F) for
                    // Ctrl+/, Ctrl+7, and Ctrl+_, so accept all three for Comment.
                    '/' | '7' | '_' => self.run_action("edit.toggle_comment"),
                    ']' => self.run_action("edit.match_bracket"),
                    _ => return false,
                }
                return true;
            }
        }
        false
    }

    /// Keys shared by every keymap: menu-bar mnemonics and function keys. Returns
    /// true if consumed.
    fn global_shared_key(&mut self, key: KeyEvent) -> bool {
        if Self::alt(&key) {
            if let KeyCode::Char(c) = key.code {
                // The Vix menu is index 0; the rest follow (File=1, …, Help=7).
                let idx = match c.to_ascii_lowercase() {
                    'f' => Some(1),
                    'e' => Some(2),
                    'v' => Some(3),
                    't' => Some(4),
                    'a' => Some(5),
                    'g' => Some(6),
                    'h' => Some(7),
                    _ => None,
                };
                if let Some(i) = idx {
                    self.menu.open_index(i);
                    return true;
                }
            }
        }
        match key.code {
            // Ctrl+Space triggers LSP completion (terminal support varies).
            KeyCode::Char(' ') if Self::ctrl(&key) => {
                self.run_action("lsp.complete");
                true
            }
            KeyCode::Tab if Self::ctrl(&key) => {
                self.run_action("tab.next");
                true
            }
            KeyCode::BackTab if Self::ctrl(&key) => {
                self.run_action("tab.prev");
                true
            }
            KeyCode::Right if Self::ctrl(&key) && Self::shift(&key) && self.focus == Focus::Editor => {
                self.run_action("edit.select_more");
                true
            }
            KeyCode::Left if Self::ctrl(&key) && Self::shift(&key) && self.focus == Focus::Editor => {
                self.run_action("edit.select_less");
                true
            }
            KeyCode::Left if Self::alt(&key) => {
                self.nav_back();
                true
            }
            KeyCode::Right if Self::alt(&key) => {
                self.nav_forward();
                true
            }
            KeyCode::Up if Self::alt(&key) && self.focus == Focus::Editor => {
                self.run_action("edit.move_line_up");
                true
            }
            KeyCode::Down if Self::alt(&key) && self.focus == Focus::Editor => {
                self.run_action("edit.move_line_down");
                true
            }
            KeyCode::F(1) => {
                self.show_help = true;
                true
            }
            KeyCode::F(12) => {
                self.run_action("nav.goto_definition");
                true
            }
            KeyCode::F(10) => {
                self.menu.toggle();
                true
            }
            KeyCode::F(3) if Self::shift(&key) => {
                self.find_step(false);
                true
            }
            KeyCode::F(3) => {
                self.find_step(true);
                true
            }
            KeyCode::Char('n' | 'N') if Self::alt(&key) && self.focus == Focus::Editor => {
                self.run_action("search.next_selection");
                true
            }
            KeyCode::Char('p' | 'P') if Self::alt(&key) && self.focus == Focus::Editor => {
                self.run_action("search.prev_selection");
                true
            }
            _ => false,
        }
    }

    fn toggle_focus_explorer_editor(&mut self) {
        self.focus = if self.focus == Focus::Explorer { Focus::Editor } else {
            if !self.show_explorer {
                self.show_explorer = true;
            }
            Focus::Explorer
        };
    }

    // ----- keymap: Emacs --------------------------------------------------

    /// Feed a key to the editor as if it were typed with no modifiers, but only
    /// when the editor pane is focused. Used to translate keymap motions
    /// (`Ctrl+F`, `l`, …) into the editor's existing handling.
    fn editor_motion(&mut self, code: KeyCode) {
        if self.focus == Focus::Editor {
            self.editor_key(KeyEvent::new(code, KeyModifiers::NONE));
        }
    }

    /// Emacs keymap dispatch: the `Ctrl+X` prefix and `Ctrl`-key chords. Returns
    /// true if the key was consumed (so it should not fall through).
    fn emacs_key(&mut self, key: KeyEvent) -> bool {
        // Second key of a `Ctrl+X …` chord.
        if self.emacs_prefix {
            self.emacs_prefix = false;
            if Self::ctrl(&key) {
                if let KeyCode::Char(c) = key.code {
                    match c.to_ascii_lowercase() {
                        'f' => self.run_action("file.open"),
                        's' => self.run_action("file.save"),
                        'c' => self.run_action("file.quit"),
                        _ => self.status = t!("status.emacs_no_chord").to_string(),
                    }
                    return true;
                }
            }
            if let KeyCode::Char('k') = key.code {
                self.run_action("file.close");
                return true;
            }
            self.status = t!("status.emacs_no_chord").to_string();
            return true;
        }
        if Self::ctrl(&key) {
            if let KeyCode::Char(c) = key.code {
                match c.to_ascii_lowercase() {
                    'x' => self.emacs_prefix = true,
                    'g' => self.status = t!("status.emacs_quit").to_string(),
                    's' => self.run_action("edit.find"),
                    'f' => self.editor_motion(KeyCode::Right),
                    'b' => self.editor_motion(KeyCode::Left),
                    'n' => self.editor_motion(KeyCode::Down),
                    'p' => self.editor_motion(KeyCode::Up),
                    'a' => self.editor_motion(KeyCode::Home),
                    'e' => self.editor_motion(KeyCode::End),
                    'v' => self.editor_motion(KeyCode::PageDown),
                    'd' => self.editor_motion(KeyCode::Delete),
                    _ => return false,
                }
                return true;
            }
        }
        false
    }

    // ----- keymap: Vim ----------------------------------------------------

    /// Vim keymap dispatch: Normal-mode motions/commands, Insert mode, and the
    /// `:` command line. Returns true if the key was consumed.
    fn vim_key(&mut self, key: KeyEvent) -> bool {
        if self.vim_cmd.is_some() {
            self.vim_cmd_key(key);
            return true;
        }
        if self.vim_insert {
            if key.code == KeyCode::Esc {
                self.vim_insert = false;
                return true;
            }
            // Let typing and shared keys flow through to the editor.
            return false;
        }
        // Normal mode. Defer modifier combos and function keys to the shared
        // handler (menu mnemonics, F10, …).
        if Self::ctrl(&key) || Self::alt(&key) || matches!(key.code, KeyCode::F(_)) {
            return false;
        }
        // `:` opens the command line from any pane (shown in the mode indicator).
        if key.code == KeyCode::Char(':') {
            self.vim_cmd = Some(String::new());
            return true;
        }
        // Other Normal-mode keys only make sense over the editor; elsewhere let the
        // focused pane keep its own navigation.
        if self.focus != Focus::Editor {
            return false;
        }
        match key.code {
            KeyCode::Char('h') | KeyCode::Left => self.editor_motion(KeyCode::Left),
            KeyCode::Char('j') | KeyCode::Down => self.editor_motion(KeyCode::Down),
            KeyCode::Char('k') | KeyCode::Up => self.editor_motion(KeyCode::Up),
            KeyCode::Char('l') | KeyCode::Right => self.editor_motion(KeyCode::Right),
            KeyCode::Char('0') => self.editor_motion(KeyCode::Home),
            KeyCode::Char('$') => self.editor_motion(KeyCode::End),
            KeyCode::Char('x') => self.editor_motion(KeyCode::Delete),
            KeyCode::Char('i') => self.vim_enter_insert(),
            KeyCode::Char('a') => {
                self.editor_motion(KeyCode::Right);
                self.vim_enter_insert();
            }
            KeyCode::Char('o') => {
                self.editor_motion(KeyCode::End);
                self.editor_motion(KeyCode::Enter);
                self.vim_enter_insert();
            }
            KeyCode::Char('O') => {
                self.editor_motion(KeyCode::Home);
                self.editor_motion(KeyCode::Enter);
                self.editor_motion(KeyCode::Up);
                self.vim_enter_insert();
            }
            _ => {}
        }
        // Swallow every other Normal-mode key so it never types into the buffer.
        true
    }

    fn vim_enter_insert(&mut self) {
        self.vim_insert = true;
    }

    /// Handle a key while the Vim `:` command line is open. The in-progress text
    /// is reflected live by the mode indicator, so this only mutates state.
    fn vim_cmd_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.vim_cmd = None,
            KeyCode::Enter => {
                let cmd = self.vim_cmd.take().unwrap_or_default();
                self.run_vim_command(cmd.trim());
            }
            KeyCode::Backspace => {
                // Backspacing past the empty `:` closes the command line.
                if let Some(s) = self.vim_cmd.as_mut() {
                    if s.pop().is_none() {
                        self.vim_cmd = None;
                    }
                }
            }
            KeyCode::Char(c) => {
                if let Some(s) = self.vim_cmd.as_mut() {
                    s.push(c);
                }
            }
            _ => {}
        }
    }

    /// Run a Vim ex command (the text after `:`).
    fn run_vim_command(&mut self, cmd: &str) {
        match cmd {
            "w" => self.run_action("file.save"),
            "q" => self.run_action("file.close"),
            // Vim force-quit discards unsaved changes without prompting.
            "q!" => self.should_quit = true,
            "wq" | "x" => {
                self.run_action("file.save");
                self.run_action("file.close");
            }
            "Ex" => {
                self.show_explorer = true;
                self.focus = Focus::Explorer;
            }
            "" => {}
            other => self.status = t!("status.vim_no_command", cmd = other).to_string(),
        }
    }

    // ----- action dispatch (menu + palette + shortcuts) ------------------

    /// Dispatch a named action (shared by the menu bar, command palette, and
    /// keyboard shortcuts).
    pub fn run_action(&mut self, action: &str) {
        match action {
            "file.new" => {
                self.editor.new_tab();
                self.focus = Focus::Editor;
                self.status = t!("status.new_buffer").into();
            }
            "file.open" => {
                self.prompt = Some(Prompt::new(PromptKind::Open, t!("prompt.open").to_string()));
            }
            "file.open_recent" => self.open_recent_chooser(),
            "file.save" => self.save(),
            "file.save_as" => {
                let cur = self
                    .editor
                    .active_tab()
                    .and_then(|t| t.path.clone())
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                self.prompt =
                    Some(Prompt::new(PromptKind::SaveAs, t!("prompt.save_as").to_string()).with_input(cur));
            }
            "file.rename" => self.open_rename_prompt(),
            "file.close" => self.request_close_active(),
            "file.close_all" => {
                for p in self.editor.close_all() {
                    self.push_closed_tab(p);
                }
                self.focus = Focus::Editor;
                self.status = t!("status.closed_all").into();
            }
            "file.reopen_closed" => self.reopen_closed_tab(),
            "file.quit" => self.request_quit(),
            "edit.undo" => {
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.apply(UndoAction {});
                    t.dirty = true;
                }
            }
            "edit.redo" => {
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.apply(RedoAction {});
                    t.dirty = true;
                }
            }
            "edit.cut" => {
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.apply(CutAction {});
                    t.dirty = true;
                    t.preview = false;
                }
            }
            "edit.copy" => {
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.apply(CopyAction {});
                }
            }
            "edit.paste" => {
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.apply(PasteAction {});
                    t.dirty = true;
                    t.preview = false;
                }
            }
            "edit.toggle_comment" => {
                if let Some(t) = self.editor.active_tab_mut() {
                    if !t.is_image() {
                        // Comments the cursor line, or every line touched by the
                        // selection; the editor picks the language's token.
                        t.editor.apply(ToggleComment {});
                        t.dirty = true;
                        t.preview = false;
                    }
                }
            }
            "edit.case_upper" => self.change_case(crate::case::upper),
            "edit.case_lower" => self.change_case(crate::case::lower),
            "edit.case_title" => self.change_case(crate::case::title),
            "edit.case_kebab" => self.change_case(crate::case::kebab),
            "edit.case_snake" => self.change_case(crate::case::snake),
            "edit.case_camel" => self.change_case(crate::case::camel),
            "edit.case_pascal" => self.change_case(crate::case::pascal),
            "edit.select_all" => self.editor.select_all(),
            "edit.duplicate_line" => self.editor.duplicate_line(),
            "edit.move_line_up" => {
                let area = self.editor_view();
                self.editor.move_line(false, area);
            }
            "edit.move_line_down" => {
                let area = self.editor_view();
                self.editor.move_line(true, area);
            }
            "edit.match_bracket" => {
                let area = self.editor_view();
                self.editor.jump_matching_bracket(area);
            }
            "edit.select_more" => {
                let area = self.editor_view();
                self.editor.select_word(true, area);
            }
            "edit.select_less" => {
                let area = self.editor_view();
                self.editor.select_word(false, area);
            }
            "edit.select_line" => {
                let area = self.editor_view();
                self.editor.select_line(area);
            }
            "edit.select_paragraph" => {
                let area = self.editor_view();
                self.editor.select_paragraph(area);
            }
            "edit.select_section" => {
                let area = self.editor_view();
                self.editor.select_section(area);
            }
            "edit.find" => self.start_search(false),
            "edit.find_next" => self.find_step(true),
            "edit.find_prev" => self.find_step(false),
            "edit.replace" => self.start_search(true),
            "edit.query_replace" => {
                self.start_search(true);
                if let Some(s) = self.search.as_mut() {
                    s.interactive = true;
                }
            }
            "search.workspace" => self.open_workspace_search(false),
            "search.workspace_replace" => self.open_workspace_search(true),
            "search.workspace_dock" => {
                self.prompt =
                    Some(Prompt::new(PromptKind::SearchToDock, t!("prompt.search_dock").to_string()));
            }
            "search.next_selection" => self.find_selection(true),
            "search.prev_selection" => self.find_selection(false),
            "edit.go_first" => {
                let area = self.editor_view();
                self.editor.cursor_document_start(area);
            }
            "edit.go_last" => {
                let area = self.editor_view();
                self.editor.cursor_document_end(area);
            }
            "edit.line_start" => self.editor.cursor_line_start(),
            "edit.line_end" => self.editor.cursor_line_end(),
            "edit.para_start" => {
                let area = self.editor_view();
                self.editor.cursor_paragraph_start(area);
            }
            "edit.para_end" => {
                let area = self.editor_view();
                self.editor.cursor_paragraph_end(area);
            }
            "edit.section_start" => {
                let area = self.editor_view();
                self.editor.cursor_section_start(area);
            }
            "edit.section_end" => {
                let area = self.editor_view();
                self.editor.cursor_section_end(area);
            }
            "nav.goto_line" => self.open_palette_seeded(":"),
            "nav.goto_definition" => self.goto_definition(),
            "lsp.hover" => self.lsp_hover(),
            "lsp.complete" => self.lsp_complete(),
            "nav.goto_symbol" => self.open_palette_seeded("@"),
            "nav.outline" => self.open_outline(),
            "explorer.filter_include" => {
                let cur = self.explorer.include_filter.clone();
                self.prompt = Some(
                    Prompt::new(PromptKind::ExplorerInclude, t!("prompt.explorer_include").to_string())
                        .with_input(cur),
                );
            }
            "explorer.filter_exclude" => {
                let cur = self.explorer.exclude_filter.clone();
                self.prompt = Some(
                    Prompt::new(PromptKind::ExplorerExclude, t!("prompt.explorer_exclude").to_string())
                        .with_input(cur),
                );
            }
            a if a.starts_with("view.theme:") => self.set_theme_by_name(&a["view.theme:".len()..]),
            "view.locale" => self.open_locale_chooser(),
            a if a.starts_with("view.keymap:") => self.set_keymap(&a["view.keymap:".len()..]),
            "tools.calendar" => {
                self.show_calendar = !self.show_calendar;
                // Always open on the present month; navigation is per-session.
                if self.show_calendar {
                    self.calendar.reset();
                }
            }
            "tools.nerd_palette" => self.open_nerd_palette(),
            "tools.ascii" => self.open_ascii_panel(),
            "tools.x11_colors" => self.open_x11_panel(),
            "tools.html_chars" => self.open_html_panel(),
            "tools.system_info" => self.open_system_info(),
            "tools.file_info" => self.open_file_info(),
            "tools.contacts" => self.open_contacts(),
            "tools.clock" => {
                self.show_clock = !self.show_clock;
                if self.show_clock {
                    self.clock.selected = 0;
                }
            }
            "view.time_zone" => self.open_time_zone_chooser(),
            "tools.dashboard" => self.open_dashboard(),
            "tools.run_command" => {
                self.prompt =
                    Some(Prompt::new(PromptKind::RunCommand, t!("prompt.run_command").to_string()));
            }
            "tools.cancel_command" => self.cancel_command(),
            "tools.palette" => self.open_palette(),
            // The left/right docks are the explorer and message drawers. Both the
            // old action ids and the new dock-named ones route to one method.
            "view.line_numbers" | "tools.line_numbers" => self.toggle_editor_line_numbers(),
            "view.whitespace" => self.toggle_editor_whitespace(),
            "view.soft_wrap" => self.toggle_editor_soft_wrap(),
            "view.left_dock" | "view.explorer" => self.toggle_left_dock(),
            "view.right_dock" | "view.messages" => self.toggle_right_dock(),
            "view.status_bar" => self.toggle_status_bar(),
            "view.scrollbar" => self.toggle_scrollbar(),
            "view.spellcheck" => self.toggle_spellcheck(),
            "spell.suggest" => self.open_spell_suggest(),
            "git.changes" => self.open_git_panel(),
            "git.push" => self.git_remote_command("git push"),
            "git.pull" => self.git_remote_command("git pull"),
            "git.fetch" => self.git_remote_command("git fetch"),
            "git.switch_branch" => self.open_branch_chooser(),
            "ai.summarize" => self.ai_summarize(),
            "ai.explain" => self.ai_explain(),
            "ai.define" => self.ai_define(),
            "ai.annotate" => self.ai_annotate(),
            "ai.improve" => self.ai_improve(),
            "git.merge_branch" => self.open_branch_chooser_mode(true),
            "git.init" => self.git_init(),
            "git.new_branch" => self.git_begin_new_branch(),
            "git.log" => self.git_log(),
            "git.status" => self.git_status_to_dock(),
            "git.clone" => self.git_begin_clone(),
            "view.bottom_dock" => self.toggle_bottom_dock(),
            "tab.next" => self.editor.next_tab(),
            "tab.prev" => self.editor.prev_tab(),
            "help.shortcuts" => self.show_help = true,
            "help.welcome" => self.open_welcome(),
            "vix.settings" => self.open_settings_file(),
            "vix.about" => {
                self.dialog = Some(Dialog {
                    title: t!("menu.item.vix.about").to_string(),
                    body: format!("Vix {}", env!("CARGO_PKG_VERSION")),
                    editor: None,
                });
            }
            "vix.website" => self.open_text_dialog(
                t!("menu.item.vix.website").to_string(),
                "https://github.com/vixide/vix",
            ),
            "vix.email" => self.open_text_dialog(
                t!("menu.item.vix.email").to_string(),
                "joel@joelparkerhenderson.com",
            ),
            other => self.messages.warn(t!("msg.unknown_action", action = other).to_string()),
        }
    }

    fn save(&mut self) {
        if self.editor.active_tab().is_some_and(Tab::is_image) {
            self.status = t!("status.image_readonly").into();
            return;
        }
        if self.editor.active_tab().and_then(|t| t.path.as_ref()).is_none() {
            self.run_action("file.save_as");
            return;
        }
        let opts = self.save_options();
        match self.editor.save_active(opts) {
            Ok(p) => {
                self.status = t!("status.saved", path = p.display()).to_string();
                self.refresh_git();
            }
            Err(e) => self.messages.error(t!("msg.save_failed", error = e).to_string()),
        }
    }

    /// On-save normalization options derived from the current settings.
    fn save_options(&self) -> crate::editor::SaveOptions {
        crate::editor::SaveOptions {
            trim_trailing_whitespace: self.settings.trim_trailing_whitespace,
            ensure_final_newline: self.settings.ensure_final_newline,
        }
    }

    // ----- view toggles ---------------------------------------------------

    /// Toggle the left dock (the file explorer). Revealing it also reveals the
    /// active file in the tree.
    fn toggle_left_dock(&mut self) {
        self.show_explorer = !self.show_explorer;
        self.settings.show_explorer = self.show_explorer;
        if self.show_explorer {
            if let Some(p) = self.editor.active_tab().and_then(|t| t.path.clone()) {
                self.explorer.reveal(&p);
            }
        }
    }

    /// Toggle the right dock (the message drawer).
    fn toggle_right_dock(&mut self) {
        self.show_messages = !self.show_messages;
        self.settings.show_messages = self.show_messages;
    }

    /// Toggle the bottom status bar, persisting the choice.
    fn toggle_status_bar(&mut self) {
        self.show_status_bar = !self.show_status_bar;
        self.settings.show_status_bar = self.show_status_bar;
    }

    /// Toggle the bottom dock (log/output/data panel), persisting the choice.
    fn toggle_bottom_dock(&mut self) {
        self.show_bottom_dock = !self.show_bottom_dock;
        self.settings.show_bottom_dock = self.show_bottom_dock;
        if !self.show_bottom_dock && self.focus == Focus::BottomDock {
            self.focus = Focus::Editor;
        }
    }

    /// Toggle the editor's right-side scroll bar, persisting the choice.
    fn toggle_scrollbar(&mut self) {
        self.show_scrollbar = !self.show_scrollbar;
        self.settings.show_scrollbar = self.show_scrollbar;
        self.status = if self.show_scrollbar {
            t!("status.scrollbar_on")
        } else {
            t!("status.scrollbar_off")
        }
        .to_string();
    }

    /// Apply a case transform to the current selection, re-selecting the result.
    /// No-op (with a status) when there is no selection or the tab is an image.
    fn change_case(&mut self, f: fn(&str) -> String) {
        let found = self.editor.active_tab().and_then(|t| {
            if t.is_image() {
                return None;
            }
            let (s, e) = t.editor.selection_span()?;
            (s != e).then(|| (s, e, t.editor.char_text(s, e)))
        });
        let Some((s, e, text)) = found else {
            self.status = t!("status.no_selection").into();
            return;
        };
        let new = f(&text);
        let new_len = new.chars().count();
        if let Some(t) = self.editor.active_tab_mut() {
            replace_char_span(t, (s, e), &new);
            t.editor.set_selection(Some(Selection::new(s, s + new_len)));
            t.dirty = true;
            t.preview = false;
        }
    }

    /// Refresh the cached git state (repo?, branch, changed files) for the workspace
    /// root. Cheap enough to call after saves and git actions; not per-frame.
    pub fn refresh_git(&mut self) {
        self.git_repo = vix_git::is_repo(&self.root);
        // HEAD may have moved (commit/checkout) or the working tree changed; drop
        // the cached HEAD blobs so the diff gutter refetches.
        self.git_head_cache.clear();
        if self.git_repo {
            self.git_branch = vix_git::branch(&self.root);
            self.git_status = vix_git::status(&self.root);
        } else {
            self.git_branch = None;
            self.git_status.clear();
        }
    }

    /// Recompute the editor diff gutter for the active tab: a colored bar on each
    /// line that differs from its committed (HEAD) version. The HEAD blob is
    /// fetched once per path and cached.
    pub fn refresh_git_gutter(&mut self) {
        if !self.git_repo {
            if let Some(t) = self.editor.active_tab_mut() {
                t.editor.clear_gutter_marks();
            }
            return;
        }
        let Some((path, current)) = self.editor.active_tab().and_then(|t| {
            if t.is_image() {
                return None;
            }
            t.path.clone().map(|p| (p, t.text()))
        }) else {
            if let Some(t) = self.editor.active_tab_mut() {
                t.editor.clear_gutter_marks();
            }
            return;
        };
        if !self.git_head_cache.contains_key(&path) {
            let head = path
                .strip_prefix(&self.root)
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .and_then(|rel| vix_git::head_blob(&self.root, &rel))
                .unwrap_or_default();
            self.git_head_cache.insert(path.clone(), head);
        }
        let marks = vix_git::diff_marks(&self.git_head_cache[&path], &current);
        let styled: Vec<(usize, &str)> = marks.iter().map(|&(line, m)| (line, gutter_hex(m))).collect();
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_gutter_marks(styled);
        }
    }

    /// Whether the working tree has uncommitted changes (derived from the cached
    /// `git status`).
    #[must_use]
    pub fn git_dirty(&self) -> bool {
        !self.git_status.is_empty()
    }

    /// The git change for a file `path` (absolute, under the workspace root), from
    /// the cached status — `None` when not in a repo or the file is unchanged.
    #[must_use]
    pub fn git_change_for(&self, path: &Path) -> Option<vix_git::Change> {
        if !self.git_repo {
            return None;
        }
        let rel = path.strip_prefix(&self.root).ok()?.to_string_lossy().replace('\\', "/");
        self.git_status.iter().find(|s| s.path == rel).and_then(vix_git::FileStatus::primary)
    }

    /// Toggle spell-checking (red underline in comments/strings), persisting the
    /// choice and refreshing the marks on the active buffer.
    fn toggle_spellcheck(&mut self) {
        self.spellcheck = !self.spellcheck;
        self.settings.spellcheck = self.spellcheck;
        if !self.spellcheck {
            self.speller = None;
            self.speller_locale = None;
            if let Some(t) = self.editor.active_tab_mut() {
                t.editor.clear_spell_marks();
            }
        }
        self.refresh_spellcheck();
        self.status = if self.spellcheck {
            t!("status.spellcheck_on")
        } else {
            t!("status.spellcheck_off")
        }
        .to_string();
    }

    /// Load the spell checker for the active UI locale if needed (enabled, and not
    /// already loaded for that locale). A missing dictionary leaves the checker
    /// unset, so spell-checking is silently inert until the locale changes.
    fn ensure_speller(&mut self) {
        if !self.spellcheck {
            return;
        }
        let locale = rust_i18n::locale().to_string();
        if self.speller_locale.as_deref() == Some(locale.as_str()) {
            return;
        }
        self.speller = vix_spellcheck::load_for(&self.settings.dictionary_path, &locale).ok();
        self.speller_locale = Some(locale);
    }

    /// Recompute the misspelled-word underlines on the active buffer. Scans only
    /// comment and string ranges; clears the marks when spell-checking is off, no
    /// dictionary loaded, or the tab is not editable text.
    pub fn refresh_spellcheck(&mut self) {
        self.ensure_speller();
        if self.speller.is_none() {
            if let Some(t) = self.editor.active_tab_mut() {
                t.editor.clear_spell_marks();
            }
            return;
        }
        // 1. Gather (base char offset, text) for each comment/string range.
        let chunks: Option<Vec<(usize, String)>> = self.editor.active_tab().and_then(|t| {
            if t.is_image() {
                return None;
            }
            Some(
                t.editor
                    .comment_string_ranges()
                    .into_iter()
                    .map(|(s, e)| (s, t.editor.char_text(s, e)))
                    .collect(),
            )
        });
        let Some(chunks) = chunks else {
            if let Some(t) = self.editor.active_tab_mut() {
                t.editor.clear_spell_marks();
            }
            return;
        };
        // 2. Compute misspelled spans with the checker (borrow scoped, then dropped).
        let spans = match self.speller.as_ref() {
            Some(sc) => {
                let mut spans = Vec::new();
                for (base, text) in &chunks {
                    spans.extend(sc.misspellings_in(text, *base));
                }
                spans
            }
            None => Vec::new(),
        };
        // 3. Apply the underline marks.
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_spell_marks(spans);
        }
    }

    /// Open the spell-suggestion popup (Ctrl+;) for the misspelled word at the
    /// cursor. Reports a status when spell-checking is off/unavailable, the cursor
    /// is not on a word, or that word is spelled correctly.
    fn open_spell_suggest(&mut self) {
        if self.spellcheck {
            self.ensure_speller();
        }
        if self.speller.is_none() {
            self.status = t!("status.spell_unavailable").into();
            return;
        }
        let found = self.editor.active_tab().and_then(|t| {
            if t.is_image() {
                return None;
            }
            t.editor.word_at(t.editor.get_cursor())
        });
        let Some((start, end, word)) = found else {
            self.status = t!("status.spell_no_word").into();
            return;
        };
        let sc = self.speller.as_ref().unwrap();
        if sc.check(&word) {
            self.status = t!("status.spell_ok").into();
            return;
        }
        let suggestions = sc.suggest(&word);
        self.spell_suggest = Some(SpellSuggest { word, span: (start, end), suggestions, selected: 0 });
    }

    fn spell_suggest_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.spell_suggest.as_mut() {
                    p.selected = p.selected.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.spell_suggest.as_mut() {
                    if p.selected + 1 < p.suggestions.len() {
                        p.selected += 1;
                    }
                }
            }
            KeyCode::Enter => self.spell_apply_selected(),
            KeyCode::Char('a' | 'A') => self.spell_add_word(),
            KeyCode::Char('i' | 'I') => self.spell_ignore_word(),
            KeyCode::Esc => self.spell_suggest = None,
            _ => {}
        }
    }

    fn spell_suggest_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.spell_suggest;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row = (mouse.row - r.y) as usize;
        if let Some(p) = self.spell_suggest.as_mut() {
            if row < p.suggestions.len() {
                p.selected = row;
                self.spell_apply_selected();
            }
        }
    }

    /// Replace the misspelled word with the highlighted suggestion.
    fn spell_apply_selected(&mut self) {
        let Some(p) = self.spell_suggest.take() else {
            return;
        };
        let Some(rep) = p.suggestions.get(p.selected).cloned() else {
            return;
        };
        if let Some(t) = self.editor.active_tab_mut() {
            replace_char_span(t, p.span, &rep);
            t.dirty = true;
            t.preview = false;
        }
        self.refresh_spellcheck();
    }

    /// Add the misspelled word to the session user dictionary.
    fn spell_add_word(&mut self) {
        let Some(p) = self.spell_suggest.take() else {
            return;
        };
        if let Some(sc) = self.speller.as_mut() {
            sc.add_word(&p.word);
        }
        self.status = t!("status.spell_added", word = p.word).to_string();
        self.refresh_spellcheck();
    }

    /// Ignore the misspelled word for the rest of the session.
    fn spell_ignore_word(&mut self) {
        let Some(p) = self.spell_suggest.take() else {
            return;
        };
        if let Some(sc) = self.speller.as_mut() {
            sc.ignore_word(&p.word);
        }
        self.status = t!("status.spell_ignored", word = p.word).to_string();
        self.refresh_spellcheck();
    }

    // ----- right-click context menu ---------------------------------------

    /// Open the editor context menu at screen position `(x, y)`.
    fn open_context_menu(&mut self, x: u16, y: u16) {
        let selected = CONTEXT_ITEMS.iter().position(|(_, a)| *a != SEP_ACTION).unwrap_or(0);
        self.context_menu = Some(ContextMenu { selected, x, y });
    }

    fn context_menu_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.context_move(false),
            KeyCode::Down => self.context_move(true),
            KeyCode::Enter => self.run_context_selected(),
            KeyCode::Esc => self.context_menu = None,
            _ => {}
        }
    }

    /// Move the context-menu selection to the next/previous non-separator row.
    fn context_move(&mut self, down: bool) {
        let Some(cm) = self.context_menu.as_mut() else { return };
        let n = CONTEXT_ITEMS.len();
        let mut i = cm.selected;
        for _ in 0..n {
            i = if down { (i + 1) % n } else { (i + n - 1) % n };
            if CONTEXT_ITEMS[i].1 != SEP_ACTION {
                break;
            }
        }
        cm.selected = i;
    }

    fn context_menu_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.context_menu;
        if !rect_contains(r, mouse.column, mouse.row) {
            self.context_menu = None; // click outside dismisses
            return;
        }
        let row = (mouse.row.saturating_sub(r.y + 1)) as usize; // +1 for the top border
        if row < CONTEXT_ITEMS.len() && CONTEXT_ITEMS[row].1 != SEP_ACTION {
            if let Some(cm) = self.context_menu.as_mut() {
                cm.selected = row;
            }
            self.run_context_selected();
        }
    }

    /// Run the highlighted context-menu action and close the menu.
    fn run_context_selected(&mut self) {
        let action = self.context_menu.as_ref().map(|cm| CONTEXT_ITEMS[cm.selected].1);
        self.context_menu = None;
        if let Some(action) = action {
            if action != SEP_ACTION {
                self.run_action(action);
            }
        }
    }

    // ----- git changes panel ----------------------------------------------

    /// Open the git changes panel (refreshing status first). Reports a status when
    /// the workspace root is not a git repository.
    fn open_git_panel(&mut self) {
        self.refresh_git();
        if !self.git_repo {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        self.git_panel = Some(GitPanel { selected: 0 });
    }

    fn git_panel_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.git_panel.as_mut() {
                    p.selected = p.selected.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.git_panel.as_mut() {
                    if p.selected + 1 < self.git_status.len() {
                        p.selected += 1;
                    }
                }
            }
            // Space / s / u toggle or set the staged state of the selected file.
            KeyCode::Char(' ') => self.git_toggle_stage(),
            KeyCode::Char('s' | 'S') => self.git_stage_selected(true),
            KeyCode::Char('u' | 'U') => self.git_stage_selected(false),
            KeyCode::Char('c' | 'C') => self.git_begin_commit(),
            KeyCode::Char('r' | 'R') => {
                self.refresh_git();
                self.clamp_git_selection();
            }
            KeyCode::Esc => self.git_panel = None,
            _ => {}
        }
    }

    fn git_panel_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.git_panel;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row = (mouse.row - r.y) as usize;
        if row < self.git_status.len() {
            if let Some(p) = self.git_panel.as_mut() {
                p.selected = row;
            }
            self.git_toggle_stage();
        }
    }

    /// The repo-relative path of the selected changed file.
    fn git_selected_path(&self) -> Option<String> {
        let idx = self.git_panel.as_ref()?.selected;
        self.git_status.get(idx).map(|s| s.path.clone())
    }

    fn clamp_git_selection(&mut self) {
        if let Some(p) = self.git_panel.as_mut() {
            p.selected = p.selected.min(self.git_status.len().saturating_sub(1));
        }
    }

    /// Stage (or unstage) the selected file, then refresh.
    fn git_stage_selected(&mut self, stage: bool) {
        let Some(path) = self.git_selected_path() else {
            return;
        };
        let ok = if stage {
            vix_git::stage(&self.root, &path)
        } else {
            vix_git::unstage(&self.root, &path)
        };
        if !ok {
            self.messages.error(t!("msg.git_failed").to_string());
        }
        self.refresh_git();
        self.clamp_git_selection();
    }

    /// Toggle the selected file between staged and unstaged based on its current
    /// state (staged → unstage; otherwise stage).
    fn git_toggle_stage(&mut self) {
        let staged = self
            .git_panel
            .as_ref()
            .and_then(|p| self.git_status.get(p.selected))
            .is_some_and(vix_git::FileStatus::is_staged);
        self.git_stage_selected(!staged);
    }

    /// Begin a commit: prompt for a message (only when something is staged).
    fn git_begin_commit(&mut self) {
        let any_staged = self.git_status.iter().any(vix_git::FileStatus::is_staged);
        if !any_staged {
            self.status = t!("status.git_nothing_staged").into();
            return;
        }
        self.git_panel = None;
        self.prompt = Some(Prompt::new(PromptKind::GitCommit, t!("prompt.git_commit").to_string()));
    }

    /// Run `git commit -m <message>` and report the outcome.
    fn git_commit(&mut self, message: &str) {
        let message = message.trim();
        if message.is_empty() {
            self.status = t!("status.git_empty_message").into();
            return;
        }
        match vix_git::commit(&self.root, message) {
            Ok(()) => self.status = t!("status.git_committed").into(),
            Err(e) => self.messages.error(t!("msg.git_commit_failed", error = e).to_string()),
        }
        self.refresh_git();
    }

    /// Begin creating a topic branch: prompt for its name (only in a repo).
    fn git_begin_new_branch(&mut self) {
        if !vix_git::is_repo(&self.root) {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        self.git_panel = None;
        self.prompt = Some(Prompt::new(
            PromptKind::GitNewBranch,
            t!("prompt.git_new_branch").to_string(),
        ));
    }

    /// Create a new topic branch named `name` and switch to it.
    fn git_create_branch(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            self.status = t!("status.git_empty_branch").into();
            return;
        }
        match vix_git::create_branch(&self.root, name) {
            Ok(()) => {
                self.status = t!("status.git_switched", branch = name).to_string();
                self.refresh_git();
                // Files on disk may now differ; refresh the explorer tree.
                self.explorer.rebuild();
            }
            Err(e) => self.messages.error(t!("msg.git_branch_failed", error = e).to_string()),
        }
    }

    /// Show the commit history, streaming `git log` into the bottom dock.
    fn git_log(&mut self) {
        if !vix_git::is_repo(&self.root) {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        self.git_panel = None;
        self.run_command("git --no-pager log --oneline --graph --decorate -n 200");
    }

    /// Show the working-tree status, streaming `git status` into the bottom dock.
    fn git_status_to_dock(&mut self) {
        if !vix_git::is_repo(&self.root) {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        self.git_panel = None;
        self.run_command("git --no-pager status");
    }

    /// Initialize a git repository in the workspace, refusing (for safety) if one
    /// already exists (a `.git` directory or a detected repo).
    fn git_init(&mut self) {
        if self.root.join(".git").exists() || vix_git::is_repo(&self.root) {
            self.status = t!("status.git_already_init").to_string();
            return;
        }
        self.run_command("git init");
    }

    /// Begin cloning a repository: prompt for its URL (works outside a repo too).
    fn git_begin_clone(&mut self) {
        self.git_panel = None;
        self.prompt = Some(Prompt::new(PromptKind::GitClone, t!("prompt.git_clone").to_string()));
    }

    /// Clone `url` into the workspace, streaming `git clone` into the bottom dock.
    fn git_clone(&mut self, url: &str) {
        let url = url.trim();
        if url.is_empty() {
            self.status = t!("status.git_empty_url").into();
            return;
        }
        self.run_command(&format!("git clone {url}"));
    }

    /// Run a remote git command (push/pull/fetch) asynchronously, streaming its
    /// output to the bottom dock. Git state refreshes when it completes.
    fn git_remote_command(&mut self, cmd: &str) {
        if !vix_git::is_repo(&self.root) {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        self.git_panel = None;
        self.run_command(cmd);
    }

    // ----- git branch switcher --------------------------------------------

    fn open_branch_chooser(&mut self) {
        self.open_branch_chooser_mode(false);
    }

    /// Open the branch chooser; `merge` picks merge-into-current rather than
    /// checkout.
    fn open_branch_chooser_mode(&mut self, merge: bool) {
        if !vix_git::is_repo(&self.root) {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        let branches = vix_git::local_branches(&self.root);
        if branches.is_empty() {
            self.status = t!("status.git_no_branches").into();
            return;
        }
        self.branch_chooser = Some(BranchChooser { branches, selected: 0, merge });
    }

    fn branch_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(c) = self.branch_chooser.as_mut() {
                    let n = c.branches.len();
                    c.selected = (c.selected + n - 1) % n;
                }
            }
            KeyCode::Down => {
                if let Some(c) = self.branch_chooser.as_mut() {
                    c.selected = (c.selected + 1) % c.branches.len();
                }
            }
            KeyCode::Enter => self.checkout_selected_branch(),
            KeyCode::Esc => self.branch_chooser = None,
            _ => {}
        }
    }

    fn branch_mouse(&mut self, mouse: MouseEvent) {
        if let Some(idx) = self.chooser_row(mouse) {
            if let Some(c) = self.branch_chooser.as_mut() {
                if idx < c.branches.len() {
                    c.selected = idx;
                    self.checkout_selected_branch();
                }
            }
        }
    }

    /// Apply the highlighted branch and close the chooser: merge it into the
    /// current branch (merge mode) or check it out.
    fn checkout_selected_branch(&mut self) {
        let Some(c) = self.branch_chooser.take() else {
            return;
        };
        let Some(branch) = c.branches.get(c.selected).cloned() else {
            return;
        };
        if c.merge {
            // Merge streams its output (and any conflicts) to the bottom dock;
            // git state and the tree refresh when it finishes.
            self.run_command(&format!("git merge {branch}"));
            return;
        }
        match vix_git::checkout(&self.root, &branch) {
            Ok(()) => {
                self.status = t!("status.git_switched", branch = branch).to_string();
                self.refresh_git();
                // Files on disk may now differ; refresh the explorer tree.
                self.explorer.rebuild();
            }
            Err(e) => self.messages.error(t!("msg.git_checkout_failed", error = e).to_string()),
        }
    }

    /// Toggle the editor's line-number gutter, driven by the editor panel's own
    /// `toggle_line_numbers`, then mirrored across every tab and persisted.
    fn toggle_editor_line_numbers(&mut self) {
        let fallback = !self.editor.line_numbers;
        let on = self
            .editor
            .active_tab_mut()
            .map_or(fallback, |t| t.editor.toggle_line_numbers());
        self.editor.line_numbers = on;
        self.editor.refresh_line_numbers();
        self.settings.line_numbers = on;
        self.status = if on {
            t!("status.line_numbers_on")
        } else {
            t!("status.line_numbers_off")
        }
        .to_string();
    }

    /// Toggle the editor's visible-whitespace glyphs, mirrored across every tab
    /// and persisted.
    fn toggle_editor_whitespace(&mut self) {
        let fallback = !self.editor.show_whitespace;
        let on = self
            .editor
            .active_tab_mut()
            .map_or(fallback, |t| t.editor.toggle_whitespace());
        self.editor.show_whitespace = on;
        self.editor.refresh_whitespace();
        self.settings.show_whitespace = on;
        self.status = if on {
            t!("status.whitespace_on")
        } else {
            t!("status.whitespace_off")
        }
        .to_string();
    }

    /// Toggle soft wrap (long lines wrap vs. scroll), mirrored across every tab
    /// and persisted.
    fn toggle_editor_soft_wrap(&mut self) {
        let fallback = !self.editor.soft_wrap;
        let on = self
            .editor
            .active_tab_mut()
            .map_or(fallback, |t| t.editor.toggle_soft_wrap());
        self.editor.soft_wrap = on;
        self.editor.refresh_soft_wrap();
        self.settings.soft_wrap = on;
        self.status = if on {
            t!("status.soft_wrap_on")
        } else {
            t!("status.soft_wrap_off")
        }
        .to_string();
    }

    // ----- focus handlers -------------------------------------------------

    /// The editor rectangle, clamped to a width the code editor can safely
    /// scroll within (see [`MIN_EDITOR_WIDTH`]).
    fn editor_view(&self) -> Rect {
        let r = self.layout.editor;
        Rect {
            width: r.width.max(MIN_EDITOR_WIDTH),
            height: r.height.max(1),
            ..r
        }
    }

    fn editor_key(&mut self, key: KeyEvent) {
        // Image tabs are view-only.
        if self.editor.active_tab().is_some_and(Tab::is_image) {
            return;
        }
        let area = self.editor_view();
        match key.code {
            KeyCode::Home => return self.editor.cursor_line_home(),
            KeyCode::End => return self.editor.cursor_line_end(),
            KeyCode::Delete => {
                self.editor.delete_forward();
                self.mark_active_dirty();
                return;
            }
            KeyCode::PageUp => {
                self.editor.page_up(area.height.saturating_sub(2).max(1) as usize);
                return;
            }
            KeyCode::PageDown => {
                self.editor.page_down(area.height.saturating_sub(2).max(1) as usize);
                return;
            }
            _ => {}
        }

        let editing = Self::is_edit_key(&key);
        if let Some(t) = self.editor.active_tab_mut() {
            let _ = t.editor.input(key, &area);
            if editing {
                t.dirty = true;
                t.preview = false;
            }
        }
    }

    fn is_edit_key(key: &KeyEvent) -> bool {
        if Self::alt(key) {
            return false;
        }
        if Self::ctrl(key) {
            return matches!(
                key.code,
                KeyCode::Char('v' | 'x' | 'z' | 'Z' | 'k' | 'd')
            );
        }
        matches!(
            key.code,
            KeyCode::Char(_) | KeyCode::Enter | KeyCode::Backspace | KeyCode::Tab | KeyCode::BackTab
        )
    }

    fn mark_active_dirty(&mut self) {
        if let Some(t) = self.editor.active_tab_mut() {
            t.dirty = true;
            t.preview = false;
        }
    }

    // ----- LSP ------------------------------------------------------------

    /// The active tab's file path, if it has one.
    fn active_path(&self) -> Option<PathBuf> {
        self.editor.active_tab().and_then(|t| t.path.clone())
    }

    /// The cursor's LSP `(line, character)` for `path`'s server encoding.
    fn cursor_lsp_position(&self, path: &Path) -> (u32, u32) {
        let enc = self.lsp.encoding_for(path);
        let Some(t) = self.editor.active_tab() else { return (0, 0) };
        let code = t.editor.code_ref();
        let cur = t.editor.get_cursor();
        let line = code.char_to_line(cur);
        let line_start = code.line_to_char(line);
        let line_text = code.slice(line_start, line_start + code.line_len(line));
        let character = vix_lsp::position::char_to_col(&line_text, cur - line_start, enc);
        (u32::try_from(line).unwrap_or(0), character)
    }

    /// Push the active document to its language server when it has changed since
    /// the last sync (sending `didOpen` the first time, then `didChange`).
    fn lsp_sync_active(&mut self) {
        let Some(path) = self.active_path() else { return };
        if !self.lsp.handles(&path) {
            return;
        }
        let Some(rev) = self.editor.active_tab().map(|t| t.editor.revision()) else { return };
        let last = self.lsp_synced.get(&path).copied();
        if last == Some(rev) {
            return;
        }
        let Some(text) = self.editor.active_tab().map(|t| t.editor.get_content()) else { return };
        if last.is_none() {
            self.lsp.did_open(&path, &text);
        } else {
            self.lsp.did_change(&path, &text);
        }
        self.lsp_synced.insert(path, rev);
    }

    /// Tell the language server a file closed and forget its sync state.
    fn lsp_close(&mut self, path: &Path) {
        if self.lsp_synced.remove(path).is_some() {
            self.lsp.did_close(path);
        }
    }

    /// Drain language-server messages and act on them (diagnostics, hover,
    /// definition jumps, completion). Called once per event-loop iteration.
    pub fn poll_lsp(&mut self) {
        if !self.lsp.is_active() {
            return;
        }
        self.lsp_sync_active();
        for event in self.lsp.poll() {
            match event {
                crate::lsp::LspEvent::Diagnostics(_) => {}
                crate::lsp::LspEvent::Hover(text) => self.hover = Some(HoverPopup { text }),
                crate::lsp::LspEvent::Definition { path, line, character } => {
                    self.lsp_jump(&path, line, character);
                }
                crate::lsp::LspEvent::Completion(items) => {
                    if !items.is_empty() {
                        self.completion = Some(CompletionPopup { items, selected: 0 });
                    }
                }
            }
        }
        // Rebuild the active editor's diagnostic underlines every tick so they
        // stay correct across new publishes and tab switches alike (cheap — it
        // just maps the stored diagnostics for the active file).
        self.refresh_diagnostic_marks();
    }

    /// Whether a language-server request is in flight or starting up (the event
    /// loop ticks faster then, so responses feel prompt).
    #[must_use]
    pub fn lsp_busy(&self) -> bool {
        self.lsp.busy()
    }

    /// Rebuild the active editor's diagnostic underline marks from the latest
    /// diagnostics for its file.
    fn refresh_diagnostic_marks(&mut self) {
        let Some(path) = self.active_path() else { return };
        if !self.lsp.handles(&path) {
            if let Some(t) = self.editor.active_tab_mut() {
                t.editor.clear_diagnostic_marks();
            }
            return;
        }
        let enc = self.lsp.encoding_for(&path);
        let ranges: Vec<(vix_lsp::Range, vix_lsp::Severity)> = self
            .lsp
            .diagnostics_for(&path)
            .iter()
            .map(|d| (d.range, d.severity))
            .collect();
        let Some(t) = self.editor.active_tab_mut() else { return };
        let marks = {
            let code = t.editor.code_ref();
            ranges
                .iter()
                .map(|(range, sev)| {
                    let start = lsp_pos_to_char(code, range.start.line, range.start.character, enc);
                    let mut end = lsp_pos_to_char(code, range.end.line, range.end.character, enc);
                    if end <= start {
                        end = start + 1; // make a zero-width diagnostic visible
                    }
                    (start, end, severity_color(*sev))
                })
                .collect::<Vec<_>>()
        };
        t.editor.set_diagnostic_marks(marks);
    }

    /// Open `path` and move the cursor to LSP `(line, character)` (0-based).
    fn lsp_jump(&mut self, path: &Path, line: u32, character: u32) {
        let path = path.to_path_buf();
        self.with_jump(|s| {
            s.open_path(&path, false);
            s.focus = Focus::Editor;
            let enc = s.lsp.encoding_for(&path);
            let (target_line, target_col) = {
                let Some(t) = s.editor.active_tab() else { return };
                let code = t.editor.code_ref();
                let ln = (line as usize).min(code.len_lines().saturating_sub(1));
                let line_start = code.line_to_char(ln);
                let line_text = code.slice(line_start, line_start + code.line_len(ln));
                let col = vix_lsp::position::col_to_char(&line_text, character, enc);
                (ln + 1, col + 1)
            };
            let area = s.editor_view();
            s.editor.goto(target_line, Some(target_col), area);
        });
    }

    /// Request hover info for the symbol under the cursor.
    fn lsp_hover(&mut self) {
        let Some(path) = self.active_path() else { return };
        if !self.lsp.handles(&path) {
            self.status = t!("status.lsp_inactive").to_string();
            return;
        }
        let (line, character) = self.cursor_lsp_position(&path);
        self.lsp.request_hover(&path, line, character);
    }

    /// Request completion candidates at the cursor.
    fn lsp_complete(&mut self) {
        let Some(path) = self.active_path() else { return };
        if !self.lsp.handles(&path) {
            self.status = t!("status.lsp_inactive").to_string();
            return;
        }
        let (line, character) = self.cursor_lsp_position(&path);
        self.lsp.request_completion(&path, line, character);
    }

    /// The identifier characters immediately before the cursor (the typed prefix
    /// a completion should extend).
    fn word_prefix_before_cursor(&self) -> String {
        let Some(t) = self.editor.active_tab() else { return String::new() };
        let code = t.editor.code_ref();
        let cur = t.editor.get_cursor();
        let line = code.char_to_line(cur);
        let line_start = code.line_to_char(line);
        let before: String = code.slice(line_start, cur);
        before
            .chars()
            .rev()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect::<String>()
            .chars()
            .rev()
            .collect()
    }

    /// Insert the highlighted completion, extending the already-typed prefix.
    fn accept_completion(&mut self) {
        let Some(popup) = self.completion.take() else { return };
        let Some(item) = popup.items.get(popup.selected) else { return };
        let prefix = self.word_prefix_before_cursor();
        // If the candidate begins with what's typed, insert only the remainder so
        // the prefix is not duplicated; otherwise insert it whole.
        let insert = if !prefix.is_empty() && item.insert_text.starts_with(&prefix) {
            item.insert_text[prefix.len()..].to_string()
        } else {
            item.insert_text.clone()
        };
        let area = self.layout.editor;
        if self.editor.insert_str(&insert, area) {
            self.mark_active_dirty();
        }
    }

    /// Handle a key while the completion popup is open. Returns true if consumed.
    fn completion_key(&mut self, key: KeyEvent) -> bool {
        let Some(popup) = self.completion.as_mut() else { return false };
        match key.code {
            KeyCode::Up => {
                popup.selected = popup.selected.saturating_sub(1);
                true
            }
            KeyCode::Down => {
                if popup.selected + 1 < popup.items.len() {
                    popup.selected += 1;
                }
                true
            }
            KeyCode::Enter | KeyCode::Tab => {
                self.accept_completion();
                true
            }
            KeyCode::Esc => {
                self.completion = None;
                true
            }
            _ => {
                // Any other key dismisses the popup and is handled normally.
                self.completion = None;
                false
            }
        }
    }

    fn explorer_key(&mut self, key: KeyEvent) {
        if Self::ctrl(&key) {
            match key.code {
                KeyCode::Char('c') => return self.explorer_copy(false),
                KeyCode::Char('x') => return self.explorer_copy(true),
                KeyCode::Char('v') => return self.explorer_paste(),
                _ => {}
            }
        }
        match key.code {
            KeyCode::Up if Self::shift(&key) => self.explorer.extend(false),
            KeyCode::Down if Self::shift(&key) => self.explorer.extend(true),
            KeyCode::Up => {
                self.explorer.clear_marks();
                self.explorer.up();
                self.preview_selected();
            }
            KeyCode::Down => {
                self.explorer.clear_marks();
                self.explorer.down();
                self.preview_selected();
            }
            KeyCode::PageUp => self.explorer.page_up(10),
            KeyCode::PageDown => self.explorer.page_down(10),
            KeyCode::Home => self.explorer.first(),
            KeyCode::End => self.explorer.last(),
            KeyCode::Enter | KeyCode::Right => self.open_or_expand_selected(),
            KeyCode::Left => {
                self.explorer.collapse_or_parent();
            }
            KeyCode::Delete => self.explorer_delete_request(),
            KeyCode::Esc => {
                if !self.clip.is_empty() && self.clip_cut {
                    self.clip.clear();
                    self.clip_cut = false;
                    self.status = t!("status.cut_cancelled").into();
                } else if !self.explorer.marked.is_empty() {
                    self.explorer.clear_marks();
                } else {
                    self.focus = Focus::Editor;
                }
            }
            _ => {}
        }
    }

    // ----- explorer clipboard --------------------------------------------

    fn explorer_copy(&mut self, cut: bool) {
        let paths = self.explorer.selected_paths();
        if paths.is_empty() {
            return;
        }
        let n = paths.len();
        self.clip = paths;
        self.clip_cut = cut;
        self.status = (if cut { t!("status.cut_n", n = n) } else { t!("status.copied_n", n = n) }).to_string();
    }

    fn explorer_paste(&mut self) {
        if self.clip.is_empty() {
            return;
        }
        let target = match self.explorer.selected_node() {
            Some(n) if n.is_dir => n.path.clone(),
            Some(n) => n
                .path
                .parent().map_or_else(|| self.root.clone(), Path::to_path_buf),
            None => self.root.clone(),
        };
        self.paste = Some(PasteOp {
            target,
            cut: self.clip_cut,
            queue: self.clip.clone().into(),
            overwrite_all: false,
            skip_all: false,
            conflict: None,
        });
        self.process_paste();
    }

    /// Advance the paste, performing each source until the queue drains or a
    /// conflict needs a decision.
    fn process_paste(&mut self) {
        loop {
            let front = self.paste.as_ref().and_then(|op| op.queue.front().cloned());
            let Some(src) = front else {
                if let Some(op) = self.paste.take() {
                    if op.cut {
                        self.clip.clear();
                        self.clip_cut = false;
                    }
                }
                self.explorer.clear_marks();
                self.explorer.rebuild();
                self.status = t!("status.paste_complete").into();
                return;
            };
            let (target, cut, overwrite_all, skip_all) = {
                let op = self.paste.as_ref().unwrap();
                (op.target.clone(), op.cut, op.overwrite_all, op.skip_all)
            };
            let same_dir = src.parent() == Some(target.as_path());
            // Cutting a file into its own directory would move it onto itself: a
            // no-op, so just drop it from the queue.
            if cut && same_dir {
                self.paste.as_mut().unwrap().queue.pop_front();
                continue;
            }
            let mut dest = target.join(src.file_name().unwrap_or_default());
            // Copying into the same directory can't overwrite the source, so it
            // gets an auto-incremented "name copy" instead of a conflict prompt.
            if !cut && same_dir {
                dest = crate::fileops::unique_copy_name(&target, &src);
            } else if dest.exists() {
                if overwrite_all {
                    // fall through and overwrite
                } else if skip_all {
                    self.paste.as_mut().unwrap().queue.pop_front();
                    continue;
                } else {
                    self.paste.as_mut().unwrap().conflict = Some(src.clone());
                    return;
                }
            }
            self.perform_paste_one(&src, &dest, cut);
            self.paste.as_mut().unwrap().queue.pop_front();
        }
    }

    fn perform_paste_one(&mut self, src: &Path, dest: &Path, cut: bool) {
        // Capture the canonical source before moving (it won't exist after).
        let src_canon = src.canonicalize().unwrap_or_else(|_| src.to_path_buf());
        let res = if cut {
            crate::fileops::move_path(src, dest)
        } else {
            crate::fileops::copy_recursive(src, dest)
        };
        match res {
            Ok(()) => {
                if cut {
                    let dest_canon = dest.canonicalize().unwrap_or_else(|_| dest.to_path_buf());
                    self.relocate_buffers(&src_canon, &dest_canon);
                }
            }
            Err(e) => self.messages.error(format!("Paste failed: {e}")),
        }
    }

    fn paste_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('o') => {
                // Overwrite just this one, then continue.
                let info = self.paste.as_ref().and_then(|op| {
                    op.queue.front().map(|s| (s.clone(), op.target.clone(), op.cut))
                });
                if let Some((src, target, cut)) = info {
                    let dest = target.join(src.file_name().unwrap_or_default());
                    self.perform_paste_one(&src, &dest, cut);
                    if let Some(op) = self.paste.as_mut() {
                        op.conflict = None;
                        op.queue.pop_front();
                    }
                }
                self.process_paste();
            }
            KeyCode::Char('O') => {
                if let Some(op) = self.paste.as_mut() {
                    op.overwrite_all = true;
                    op.conflict = None;
                }
                self.process_paste();
            }
            KeyCode::Char('s') => {
                if let Some(op) = self.paste.as_mut() {
                    op.conflict = None;
                    op.queue.pop_front();
                }
                self.process_paste();
            }
            KeyCode::Char('S') => {
                if let Some(op) = self.paste.as_mut() {
                    op.skip_all = true;
                    op.conflict = None;
                    op.queue.pop_front();
                }
                self.process_paste();
            }
            KeyCode::Char('c') | KeyCode::Esc => {
                self.paste = None;
                self.status = t!("status.paste_cancelled").into();
            }
            _ => {}
        }
    }

    // ----- unsaved-changes prompt (close tab / quit) ---------------------

    /// Display name of the active buffer, for the unsaved-changes prompt.
    fn active_tab_name(&self) -> String {
        self.editor
            .active_tab()
            .and_then(|t| t.path.as_ref())
            .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| t!("ui.untitled").to_string())
    }

    /// Index of the first tab with unsaved changes (skipping read-only images).
    fn first_dirty_tab(&self) -> Option<usize> {
        self.editor.tabs.iter().position(|t| t.dirty && !t.is_image())
    }

    /// Close the active tab, prompting first if it has unsaved changes.
    fn request_close_active(&mut self) {
        if self.editor.active_tab().is_some_and(|t| t.dirty && !t.is_image()) {
            self.unsaved = Some(UnsavedPrompt {
                mode: UnsavedMode::CloseTab,
                name: self.active_tab_name(),
            });
        } else {
            self.do_close_active();
        }
    }

    /// Close the active tab unconditionally.
    fn do_close_active(&mut self) {
        if let Some(p) = self.editor.close_active() {
            self.lsp_close(&p);
            self.push_closed_tab(p);
        }
        self.status = t!("status.closed_buffer").into();
    }

    /// Quit, prompting for each tab that has unsaved changes first.
    fn request_quit(&mut self) {
        if let Some(idx) = self.first_dirty_tab() {
            self.editor.active = idx;
            self.unsaved = Some(UnsavedPrompt {
                mode: UnsavedMode::Quit,
                name: self.active_tab_name(),
            });
        } else {
            self.should_quit = true;
        }
    }

    /// After a dirty tab is resolved during quit, move on to the next one (or
    /// actually quit when none remain).
    fn advance_quit(&mut self) {
        if let Some(idx) = self.first_dirty_tab() {
            self.editor.active = idx;
            let name = self.active_tab_name();
            if let Some(u) = self.unsaved.as_mut() {
                u.name = name;
            }
        } else {
            self.unsaved = None;
            self.should_quit = true;
        }
    }

    fn unsaved_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('s' | 'S') => self.unsaved_save(),
            KeyCode::Char('d' | 'D') => self.unsaved_discard(),
            KeyCode::Char('c' | 'C') | KeyCode::Esc => self.unsaved = None,
            _ => {}
        }
    }

    /// Save the active buffer, then continue the close/quit it was guarding. If
    /// the buffer is untitled, [`save`](Self::save) opens a Save As prompt and the
    /// chain stops there (the user can re-trigger close/quit afterward).
    fn unsaved_save(&mut self) {
        let mode = self.unsaved.as_ref().map(|u| u.mode);
        self.save();
        // Untitled buffers route to Save As (still dirty here) or a save failed;
        // either way, drop the prompt and let that flow take over.
        if self.editor.active_tab().is_some_and(|t| t.dirty) {
            self.unsaved = None;
            return;
        }
        match mode {
            Some(UnsavedMode::CloseTab) => {
                self.unsaved = None;
                self.do_close_active();
            }
            Some(UnsavedMode::Quit) => self.advance_quit(),
            None => self.unsaved = None,
        }
    }

    /// Discard unsaved changes and continue the close/quit being guarded.
    fn unsaved_discard(&mut self) {
        match self.unsaved.as_ref().map(|u| u.mode) {
            Some(UnsavedMode::CloseTab) => {
                self.unsaved = None;
                self.do_close_active();
            }
            Some(UnsavedMode::Quit) => {
                if let Some(t) = self.editor.active_tab_mut() {
                    t.dirty = false;
                }
                self.advance_quit();
            }
            None => self.unsaved = None,
        }
    }

    // ----- explorer delete (with confirm) --------------------------------

    fn explorer_delete_request(&mut self) {
        let paths = self.explorer.selected_paths();
        if paths.is_empty() {
            return;
        }
        self.confirm = Some(Confirm {
            message: t!("confirm.delete", n = paths.len()).to_string(),
            paths,
        });
    }

    fn confirm_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y' | 'Y') => {
                if let Some(c) = self.confirm.take() {
                    let mut removed = 0;
                    for path in &c.paths {
                        // Canonicalize before removing so buffer paths still match.
                        let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
                        match crate::fileops::remove_path(path) {
                            Ok(()) => {
                                self.close_buffers_under(&canon);
                                removed += 1;
                            }
                            Err(e) => self.messages.error(t!("msg.delete_failed", error = e).to_string()),
                        }
                    }
                    self.explorer.clear_marks();
                    self.explorer.rebuild();
                    self.status = t!("status.deleted_n", n = removed).to_string();
                }
            }
            KeyCode::Char('n' | 'N') | KeyCode::Esc => {
                self.confirm = None;
                self.status = t!("status.delete_cancelled").into();
            }
            _ => {}
        }
    }

    /// Move open buffers when their file (or containing directory) is moved.
    fn relocate_buffers(&mut self, src: &Path, dest: &Path) {
        for tab in &mut self.editor.tabs {
            let Some(p) = tab.path.clone() else { continue };
            if p == src {
                tab.path = Some(dest.to_path_buf());
            } else if let Ok(rel) = p.strip_prefix(src) {
                tab.path = Some(dest.join(rel));
            }
        }
    }

    /// Close buffers whose file (or containing directory) was deleted.
    fn close_buffers_under(&mut self, path: &Path) {
        let mut i = 0;
        while i < self.editor.tabs.len() {
            let under = self.editor.tabs[i]
                .path
                .as_ref()
                .is_some_and(|p| p == path || p.starts_with(path));
            if under {
                self.editor.tabs.remove(i);
            } else {
                i += 1;
            }
        }
        if self.editor.tabs.is_empty() {
            self.editor.new_tab();
        } else if self.editor.active >= self.editor.tabs.len() {
            self.editor.active = self.editor.tabs.len() - 1;
        }
    }

    fn open_or_expand_selected(&mut self) {
        if let Some(node) = self.explorer.selected_node() {
            if node.is_dir {
                self.explorer.toggle_selected();
            } else {
                let path = node.path.clone();
                self.with_jump(|s| {
                    s.open_path(&path, false);
                    s.focus = Focus::Editor;
                });
            }
        }
    }

    /// Arrow-scan preview: opening the highlighted file in an ephemeral tab.
    fn preview_selected(&mut self) {
        if !self.settings.preview_tabs {
            return;
        }
        if let Some(node) = self.explorer.selected_node() {
            if !node.is_dir && !is_image_path(&node.path) {
                let path = node.path.clone();
                let _ = self.editor.open(&path, true);
            }
        }
    }

    fn messages_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.messages.up(),
            KeyCode::Down => self.messages.down(),
            KeyCode::Char('x') | KeyCode::Delete | KeyCode::Enter => self.messages.close_selected(),
            KeyCode::Esc => self.focus = Focus::Editor,
            _ => {}
        }
    }

    fn bottomdock_key(&mut self, key: KeyEvent) {
        // A page is the dock's visible height (minus its top border).
        let page = (self.layout.bottom_dock.height.saturating_sub(1) as usize).max(1);
        match key.code {
            KeyCode::Up => self.bottom_dock.scroll_up(1),
            KeyCode::Down => self.bottom_dock.scroll_down(1, page),
            KeyCode::PageUp => self.bottom_dock.scroll_up(page),
            KeyCode::PageDown => self.bottom_dock.scroll_down(page, page),
            KeyCode::Home => self.bottom_dock.scroll_to_top(),
            KeyCode::End => self.bottom_dock.scroll_to_bottom(),
            KeyCode::Esc => self.focus = Focus::Editor,
            _ => {}
        }
    }

    fn open_path(&mut self, path: &Path, preview: bool) {
        if is_image_path(path) {
            if !preview {
                self.open_image(path);
                self.record_recent(path);
            }
            return;
        }
        match self.editor.open(path, preview) {
            Ok(()) => {
                if !preview {
                    self.editor.promote_active();
                    self.record_recent(path);
                }
                self.status = t!("status.opened", path = path.display()).to_string();
            }
            Err(e) => self.messages.error(t!("msg.open_failed", error = e).to_string()),
        }
    }

    /// Push a just-closed file path onto the reopen stack (most-recent last),
    /// de-duplicated and capped.
    fn push_closed_tab(&mut self, path: PathBuf) {
        self.closed_tabs.retain(|p| p != &path);
        self.closed_tabs.push(path);
        let cap = 20;
        if self.closed_tabs.len() > cap {
            let drop = self.closed_tabs.len() - cap;
            self.closed_tabs.drain(0..drop);
        }
    }

    /// Reopen the most recently closed tab whose file still exists.
    fn reopen_closed_tab(&mut self) {
        while let Some(path) = self.closed_tabs.pop() {
            if path.is_file() {
                self.with_jump(|s| {
                    s.open_path(&path, false);
                    s.focus = Focus::Editor;
                });
                return;
            }
        }
        self.status = t!("status.no_closed_tab").to_string();
    }

    /// Record a real (non-preview) file open at the front of the recent list,
    /// de-duplicated and capped. Stored canonicalized so reopening is reliable.
    fn record_recent(&mut self, path: &Path) {
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let entry = canon.to_string_lossy().into_owned();
        let max = self.settings.recent_files_max;
        let recent = &mut self.settings.recent_files;
        recent.retain(|p| p != &entry);
        recent.insert(0, entry);
        recent.truncate(max);
    }

    // ----- position history (Alt+Left / Alt+Right) -----------------------

    fn current_location(&self) -> Option<Location> {
        let tab = self.editor.active_tab()?;
        let path = tab.path.clone()?;
        let (line, col) = tab.cursor_1based();
        Some(Location { path, line, col })
    }

    /// Run a cursor-moving jump `f`, recording the origin and destination in the
    /// position history so Alt+Left/Right can revisit them.
    fn with_jump<F: FnOnce(&mut Self)>(&mut self, f: F) {
        let origin = self.current_location();
        f(self);
        let dest = self.current_location();
        // Drop any forward history, then append origin then destination.
        if !self.nav_history.is_empty() {
            self.nav_history.truncate(self.nav_idx + 1);
        }
        if let Some(o) = origin {
            if self.nav_history.last() != Some(&o) {
                self.nav_history.push(o);
            }
        }
        if let Some(d) = dest {
            if self.nav_history.last() != Some(&d) {
                self.nav_history.push(d);
            }
        }
        self.nav_idx = self.nav_history.len().saturating_sub(1);
    }

    fn nav_back(&mut self) {
        if self.nav_history.is_empty() || self.nav_idx == 0 {
            self.status = t!("status.no_earlier").into();
            return;
        }
        self.nav_idx -= 1;
        self.navigate_to(self.nav_history[self.nav_idx].clone());
    }

    fn nav_forward(&mut self) {
        if self.nav_idx + 1 >= self.nav_history.len() {
            self.status = t!("status.no_later").into();
            return;
        }
        self.nav_idx += 1;
        self.navigate_to(self.nav_history[self.nav_idx].clone());
    }

    /// Go to a recorded location without itself recording a new jump.
    fn navigate_to(&mut self, loc: Location) {
        self.open_path(&loc.path, false);
        self.editor.goto(loc.line, Some(loc.col), self.editor_view());
        self.focus = Focus::Editor;
        self.status = format!("{}:{}", loc.path.display(), loc.line);
    }

    fn open_image(&mut self, path: &Path) {
        let Some(picker) = self.picker.as_ref() else {
            self.messages
                .warn(t!("msg.image_needs_terminal"));
            return;
        };
        match decode_image(path) {
            Ok(img) => {
                let proto = picker.new_resize_protocol(img);
                self.editor.open_image(path, proto);
                self.focus = Focus::Editor;
                self.status = t!("status.opened_image", path = path.display()).to_string();
            }
            Err(e) => self.messages.error(t!("msg.image_open_failed", error = e).to_string()),
        }
    }

    // ----- mouse ----------------------------------------------------------

    /// Handle a mouse event, dispatching to whichever pane it lands in.
    pub fn on_mouse(&mut self, mouse: MouseEvent) {
        // The welcome overlay is modal: the wheel scrolls it, nothing else.
        if self.welcome.is_some() {
            self.welcome_mouse(mouse);
            return;
        }
        // The right-click context menu takes all clicks while open (a click on a
        // row runs it; a click elsewhere dismisses it).
        if self.context_menu.is_some() {
            self.context_menu_mouse(mouse);
            return;
        }
        // A right-click in the editor opens the context menu.
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Right))
            && rect_contains(self.layout.editor, mouse.column, mouse.row)
        {
            self.open_context_menu(mouse.column, mouse.row);
            return;
        }
        // Overlays swallow mouse input rather than acting on panes underneath.
        // The info dialog is modal. Within a text-field dialog, clicks/drags in
        // the field select text (for copying); a left click anywhere else acts as
        // the Ok button and closes.
        if self.dialog.is_some() {
            let (col, row) = (mouse.column, mouse.row);
            let in_field = self.dialog.as_ref().is_some_and(|d| d.editor.is_some())
                && rect_contains(self.layout.dialog_body, col, row);
            if in_field {
                let area = self.dialog_field_area();
                if let Some(ed) = self.dialog.as_mut().and_then(|d| d.editor.as_mut()) {
                    let _ = ed.mouse(mouse, &area);
                }
            } else if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                self.dialog = None;
            }
            return;
        }
        // Choosers are list overlays: a left click on a row highlights it (and,
        // for the theme chooser, previews live), mirroring keyboard Up/Down.
        if self.locale_chooser.is_some() {
            self.locale_mouse(mouse);
            return;
        }
        if self.time_zone_chooser.is_some() {
            self.time_zone_mouse(mouse);
            return;
        }
        if self.recent_chooser.is_some() {
            self.recent_mouse(mouse);
            return;
        }
        if self.nerd_palette.is_some() {
            self.nerd_mouse(mouse);
            return;
        }
        if self.ascii_panel.is_some() {
            self.ascii_mouse(mouse);
            return;
        }
        if self.x11_panel.is_some() {
            self.x11_mouse(mouse);
            return;
        }
        if self.html_panel.is_some() {
            self.html_mouse(mouse);
            return;
        }
        if self.system_info.is_some() {
            self.system_info_mouse(mouse);
            return;
        }
        if self.file_info.is_some() {
            self.file_info_mouse(mouse);
            return;
        }
        if self.vcard.is_some() {
            self.vcard_mouse(mouse);
            return;
        }
        if self.contacts.is_some() {
            self.contacts_mouse(mouse);
            return;
        }
        if self.spell_suggest.is_some() {
            self.spell_suggest_mouse(mouse);
            return;
        }
        if self.git_panel.is_some() {
            self.git_panel_mouse(mouse);
            return;
        }
        if self.branch_chooser.is_some() {
            self.branch_mouse(mouse);
            return;
        }
        if self.outline.is_some() {
            self.outline_mouse(mouse);
            return;
        }
        // The find / replace box: a left click focuses the Find or Replace field.
        if self.search.is_some() {
            self.search_mouse(mouse);
            return;
        }
        // The calendar box: a left click inserts a date-time line or a day.
        if self.show_calendar {
            self.calendar_mouse(mouse);
            return;
        }
        // The clock box: a left click inserts the picked time row.
        if self.show_clock {
            self.clock_mouse(mouse);
            return;
        }
        // Keyboard-only modal overlays swallow all mouse input rather than
        // letting a click fall through to the editor/explorer underneath.
        if self.show_help
            || self.palette.is_some()
            || self.prompt.is_some()
            || self.query_replace.is_some()
            || self.workspace_search.is_some()
            || self.confirm.is_some()
            || self.unsaved.is_some()
            || self.spell_suggest.is_some()
            || self.git_panel.is_some()
            || self.branch_chooser.is_some()
            || self.dashboard.is_some()
            || self.outline.is_some()
            || self.paste.as_ref().is_some_and(|p| p.conflict.is_some())
        {
            return;
        }
        let (col, row) = (mouse.column, mouse.row);

        // Clicking the status-bar git/branch indicator opens the Git panel.
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
            && rect_contains(self.layout.git_status_bar, col, row)
        {
            self.run_action("git.changes");
            return;
        }

        // Editor scrollbar: press the thumb/track to jump there, then drag to
        // scroll. The drag continues even if the pointer leaves the 1-column
        // track (tracked by `scrollbar_active`), and ends on button release.
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left)
                if rect_contains(self.layout.scrollbar, col, row) =>
            {
                self.scrollbar_active = true;
                self.scrollbar_drag(row);
                return;
            }
            MouseEventKind::Drag(MouseButton::Left) if self.scrollbar_active => {
                self.scrollbar_drag(row);
                return;
            }
            MouseEventKind::Up(MouseButton::Left) => self.scrollbar_active = false,
            _ => {}
        }

        // Dock resizing: press a dock's inner edge (the explorer's right border
        // or the messages drawer's left border) and drag to resize it. The drag
        // continues even if the pointer drifts off that column.
        let left_edge = self
            .show_explorer
            .then(|| self.layout.explorer.right().saturating_sub(1));
        let right_edge = self.show_messages.then_some(self.layout.messages.x);
        // The bottom dock's top edge (its top border row), draggable to resize.
        let bottom_edge = self.show_bottom_dock.then_some(self.layout.bottom_dock.y);
        match mouse.kind {
            // The bottom edge is a row, so check it first (a column edge could
            // otherwise win on that row).
            MouseEventKind::Down(MouseButton::Left) if Some(row) == bottom_edge => {
                self.dock_resize = Some(DockResize::Bottom);
                return;
            }
            MouseEventKind::Down(MouseButton::Left) if Some(col) == left_edge => {
                self.dock_resize = Some(DockResize::Left);
                return;
            }
            MouseEventKind::Down(MouseButton::Left) if Some(col) == right_edge => {
                self.dock_resize = Some(DockResize::Right);
                return;
            }
            MouseEventKind::Drag(MouseButton::Left) if self.dock_resize.is_some() => {
                if matches!(self.dock_resize, Some(DockResize::Bottom)) {
                    self.resize_bottom_dock(row);
                } else {
                    self.resize_dock(col);
                }
                return;
            }
            MouseEventKind::Up(MouseButton::Left) => self.dock_resize = None,
            _ => {}
        }

        // While a menu is open, a left click runs the dropdown item under the
        // pointer, switches menus when on the bar, or closes the menu when
        // clicked away. Moving the pointer (hover, or drag from the bar) follows
        // the selection without committing.
        if self.menu.is_open() {
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => self.menu_mouse(col, row),
                MouseEventKind::Moved | MouseEventKind::Drag(MouseButton::Left) => {
                    self.menu_hover(col, row);
                }
                _ => {}
            }
            return;
        }

        // Plain pointer motion only drives the open menu above. Ignore it
        // elsewhere so hovering a pane never steals focus or moves the cursor.
        if matches!(mouse.kind, MouseEventKind::Moved) {
            return;
        }

        if rect_contains(self.layout.menu, col, row) {
            if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                self.menu_click(col);
            }
            return;
        }
        if self.show_explorer && rect_contains(self.layout.explorer, col, row) {
            self.explorer_mouse(mouse);
            return;
        }
        if rect_contains(self.layout.tabs, col, row) {
            if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                self.tab_click(col);
            }
            return;
        }
        if rect_contains(self.layout.editor, col, row) {
            self.editor_mouse(mouse);
            return;
        }
        if self.show_messages && rect_contains(self.layout.messages, col, row) {
            self.messages_mouse(mouse);
            return;
        }
        if self.show_bottom_dock && rect_contains(self.layout.bottom_dock, col, row) {
            self.bottomdock_mouse(mouse);
        }
    }

    /// A left click focuses the bottom dock (and jumps to a `path:line` location
    /// on the clicked line, if any); the wheel scrolls it.
    fn bottomdock_mouse(&mut self, mouse: MouseEvent) {
        let a = self.layout.bottom_dock;
        let total = self.bottom_dock.lines.len();
        let viewport = a.height.saturating_sub(1) as usize;
        let sb_shown = self.settings.show_scrollbar && total > viewport && a.width > 1;
        let sb_col = a.x + a.width.saturating_sub(1);
        if sb_shown
            && mouse.column == sb_col
            && matches!(
                mouse.kind,
                MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left)
            )
        {
            self.focus = Focus::BottomDock;
            let sb_rect = Rect { x: sb_col, y: a.y + 1, width: 1, height: a.height - 1 };
            self.bottom_dock.scroll =
                crate::ui::scrollbar_pos_from_row(sb_rect, mouse.row, total.saturating_sub(viewport));
            return;
        }
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.focus = Focus::BottomDock;
                self.bottomdock_open_at(mouse.row);
            }
            MouseEventKind::ScrollUp => self.bottom_dock.scroll_up(3),
            MouseEventKind::ScrollDown => {
                let page = (self.layout.bottom_dock.height.saturating_sub(1) as usize).max(1);
                self.bottom_dock.scroll_down(3, page);
            }
            _ => {}
        }
    }

    /// If the clicked dock line names a `path:line[:col]` location (e.g. a build
    /// error or grep hit), open that file there.
    fn bottomdock_open_at(&mut self, row: u16) {
        let area = self.layout.bottom_dock;
        if row <= area.y {
            return; // the top border row
        }
        let inner_h = area.height.saturating_sub(1) as usize;
        let idx = (row - area.y - 1) as usize;
        let Some(line) = self.bottom_dock.visible(inner_h).get(idx).cloned() else {
            return;
        };
        let (path, target) = palette::parse_path_target(line.trim());
        let (Some((line_no, col)), false) = (target, path.is_empty()) else {
            return;
        };
        let path = self.resolve(&path);
        if path.is_file() {
            self.with_jump(|s| {
                s.open_path(&path, false);
                let area = s.editor_view();
                s.editor.goto(line_no, Some(col), area);
                s.focus = Focus::Editor;
            });
        }
    }

    /// Scroll the editor to the line corresponding to a scrollbar row `row`.
    ///
    /// The scrollbar thumb tracks the cursor line, so dragging maps the pointer's
    /// position along the track to a target line and moves the cursor there,
    /// which scrolls the view (and the thumb) to match.
    fn scrollbar_drag(&mut self, row: u16) {
        let sb = self.layout.scrollbar;
        if sb.height == 0 {
            return;
        }
        let total = self.editor.active_line_count().max(1);
        // Fraction of the track the pointer is at, mapped to a 1-based line.
        let rel = row.saturating_sub(sb.y).min(sb.height.saturating_sub(1)) as usize;
        let denom = (sb.height.saturating_sub(1)).max(1) as usize;
        let line = 1 + rel * (total - 1) / denom;
        let area = self.editor_view();
        self.editor.goto(line, None, area);
        self.focus = Focus::Editor;
    }

    /// Resize the dock currently being dragged so its edge follows column `col`,
    /// keeping at least a minimum dock width and leaving room for the editor.
    fn resize_dock(&mut self, col: u16) {
        const MIN_DOCK: u16 = 12;
        const MIN_EDITOR: u16 = 20;
        let full = self.layout.menu.width; // the menu bar spans the full width
        match self.dock_resize {
            Some(DockResize::Left) => {
                let other = if self.show_messages { self.settings.messages_width } else { 0 };
                let max = full.saturating_sub(MIN_EDITOR + other).max(MIN_DOCK);
                let w = (col.saturating_sub(self.layout.explorer.x) + 1).clamp(MIN_DOCK, max);
                self.settings.explorer_width = w;
            }
            Some(DockResize::Right) => {
                let other = if self.show_explorer { self.settings.explorer_width } else { 0 };
                let max = full.saturating_sub(MIN_EDITOR + other).max(MIN_DOCK);
                let w = self.layout.messages.right().saturating_sub(col).clamp(MIN_DOCK, max);
                self.settings.messages_width = w;
            }
            Some(DockResize::Bottom) | None => {}
        }
    }

    /// Resize the bottom dock so its top edge follows `row`, keeping at least a
    /// minimum dock height and a minimum body above it.
    fn resize_bottom_dock(&mut self, row: u16) {
        const MIN_DOCK: u16 = 3;
        const MIN_BODY: u16 = 3;
        let bottom = self.layout.bottom_dock.bottom(); // boundary above the status bar
        let body_top = self.layout.menu.bottom(); // first body row (below the menu)
        let max = bottom
            .saturating_sub(body_top)
            .saturating_sub(MIN_BODY)
            .max(MIN_DOCK);
        let h = bottom.saturating_sub(row).clamp(MIN_DOCK, max);
        self.settings.bottom_dock_height = h;
    }

    fn editor_mouse(&mut self, mouse: MouseEvent) {
        self.focus = Focus::Editor;
        let area = self.layout.editor;
        if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
            if let Some(t) = self.editor.active_tab_mut() {
                t.preview = false;
            }
        }
        if let Some(t) = self.editor.active_tab_mut() {
            let _ = t.editor.mouse(mouse, &area);
        }
    }

    fn explorer_mouse(&mut self, mouse: MouseEvent) {
        let a = self.layout.explorer;
        let inner_top = a.y + 1; // inside the border
        // Pressing or dragging the scrollbar (rightmost inner column) scrolls the
        // tree instead of selecting a row.
        let total = self.explorer.nodes.len();
        let viewport = a.height.saturating_sub(1) as usize;
        let sb_shown = self.settings.show_scrollbar && total > viewport && a.width > 1;
        let sb_col = a.x + a.width.saturating_sub(2);
        if sb_shown
            && mouse.column == sb_col
            && matches!(
                mouse.kind,
                MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left)
            )
        {
            self.focus = Focus::Explorer;
            let sb_rect = Rect { x: sb_col, y: inner_top, width: 1, height: a.height - 1 };
            // The thumb tracks the selection, so map the drag to a selected row;
            // `ensure_visible` (in draw) scrolls the view to follow it.
            self.explorer.selected =
                crate::ui::scrollbar_pos_from_row(sb_rect, mouse.row, total.saturating_sub(1));
            return;
        }
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.explorer.up();
            }
            MouseEventKind::ScrollDown => {
                self.explorer.down();
            }
            MouseEventKind::Down(MouseButton::Left) => {
                self.focus = Focus::Explorer;
                if mouse.row < inner_top {
                    return;
                }
                let idx = self.explorer.top + (mouse.row - inner_top) as usize;
                if idx < self.explorer.nodes.len() {
                    let was_selected = self.explorer.selected == idx;
                    self.explorer.selected = idx;
                    let is_dir = self.explorer.nodes[idx].is_dir;
                    if was_selected && !is_dir {
                        // Second click on the same file promotes to a real tab.
                        self.open_or_expand_selected();
                    } else if is_dir {
                        self.explorer.toggle_selected();
                    } else {
                        self.preview_selected();
                    }
                }
            }
            _ => {}
        }
    }

    fn messages_mouse(&mut self, mouse: MouseEvent) {
        let area = self.layout.messages;
        let inner_top = area.y + 1;
        // Scrollbar (rightmost column) press/drag scrolls the message list.
        let total = self.messages.items.len();
        let viewport = area.height.saturating_sub(1) as usize;
        let sb_shown = self.settings.show_scrollbar && total > viewport && area.width > 1;
        let sb_col = area.x + area.width.saturating_sub(1);
        if sb_shown
            && mouse.column == sb_col
            && matches!(
                mouse.kind,
                MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left)
            )
        {
            self.focus = Focus::Messages;
            let sb_rect = Rect { x: sb_col, y: inner_top, width: 1, height: area.height - 1 };
            self.messages.selected =
                crate::ui::scrollbar_pos_from_row(sb_rect, mouse.row, total.saturating_sub(1));
            return;
        }
        match mouse.kind {
            MouseEventKind::ScrollUp => self.messages.up(),
            MouseEventKind::ScrollDown => self.messages.down(),
            MouseEventKind::Down(MouseButton::Left) => {
                self.focus = Focus::Messages;
                if mouse.row >= inner_top {
                    let idx = (mouse.row - inner_top) as usize;
                    if idx < self.messages.items.len() {
                        self.messages.selected = idx;
                        // Clicking near the right edge hits the close "x".
                        if mouse.column >= area.x + area.width.saturating_sub(3) {
                            self.messages.close_selected();
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn tab_click(&mut self, col: u16) {
        let mut x = self.layout.tabs.x + 1;
        for (i, tab) in self.editor.tabs.iter().enumerate() {
            let w = tab.title().chars().count() as u16;
            if col >= x && col < x + w {
                self.editor.active = i;
                self.editor.promote_active();
                self.focus = Focus::Editor;
                return;
            }
            x += w + 3; // title + " │ " divider
        }
    }

    /// Handle a left click while a menu dropdown is open: run the item under the
    /// pointer, switch menus when the bar is clicked, or close on a click away.
    fn menu_mouse(&mut self, col: u16, row: u16) {
        if rect_contains(self.layout.menu, col, row) {
            self.menu_click(col);
            return;
        }
        // The open submenu (drawn to the right of its parent) takes priority.
        if self.menu.sub.is_some() {
            let sd = self.layout.submenu_dropdown;
            if rect_contains(sd, col, row) {
                let top = sd.y + 1;
                if let Some(items) = self.menu.submenu_items() {
                    let idx = row.saturating_sub(top) as usize;
                    if row >= top && idx < items.len() && !items[idx].is_separator() {
                        let action = items[idx].action;
                        self.menu.close();
                        self.run_action(action);
                    }
                }
                return;
            }
        }
        let dd = self.layout.menu_dropdown;
        if rect_contains(dd, col, row) {
            // Items start one row below the dropdown's top border.
            let top = dd.y + 1;
            if let Some(mi) = self.menu.open {
                let items = menus()[mi].items;
                let idx = row.saturating_sub(top) as usize;
                if row >= top && idx < items.len() && !items[idx].is_separator() {
                    if items[idx].has_submenu() {
                        self.menu.item = Some(idx);
                        self.menu.sub = None;
                        self.menu.right(); // opens the submenu
                    } else {
                        let action = items[idx].action;
                        self.menu.close();
                        self.run_action(action);
                    }
                }
            }
            return;
        }
        // Clicked outside the bar and every dropdown: dismiss the menu.
        self.menu.close();
    }

    fn menu_click(&mut self, col: u16) {
        // Right-aligned dock toggles take priority over menu hit-testing.
        let (left_dock, right_dock) = crate::ui::dock_toggle_cols(self.layout.menu);
        if col == left_dock {
            self.run_action("view.explorer");
            return;
        }
        if col == right_dock {
            self.run_action("view.messages");
            return;
        }
        if let Some(i) = self.top_menu_index_at(col) {
            self.menu.open_index(i);
        }
    }

    /// Index of the top-level menu whose title spans column `col`, if any.
    fn top_menu_index_at(&self, col: u16) -> Option<usize> {
        let mut x = self.layout.menu.x + 1;
        for (i, m) in menus().iter().enumerate() {
            let w = m.title().chars().count() as u16 + 2;
            if col >= x && col < x + w {
                return Some(i);
            }
            x += w;
        }
        None
    }

    /// Move the open-menu highlight to follow the pointer: hovering a different
    /// top-level name switches to that menu; hovering a dropdown row highlights
    /// that item. Never commits an action or closes the menu.
    fn menu_hover(&mut self, col: u16, row: u16) {
        if rect_contains(self.layout.menu, col, row) {
            if let Some(i) = self.top_menu_index_at(col) {
                if self.menu.open != Some(i) {
                    self.menu.open_index(i);
                }
            }
            return;
        }
        if self.menu.sub.is_some() {
            let sd = self.layout.submenu_dropdown;
            if rect_contains(sd, col, row) {
                let top = sd.y + 1;
                if let Some(items) = self.menu.submenu_items() {
                    let idx = row.saturating_sub(top) as usize;
                    if row >= top && idx < items.len() && !items[idx].is_separator() {
                        self.menu.sub = Some(idx);
                    }
                }
                return;
            }
        }
        let dd = self.layout.menu_dropdown;
        if rect_contains(dd, col, row) {
            // Items start one row below the dropdown's top border.
            let top = dd.y + 1;
            if let Some(mi) = self.menu.open {
                let items = menus()[mi].items;
                let idx = row.saturating_sub(top) as usize;
                if row >= top && idx < items.len() && !items[idx].is_separator() {
                    self.menu.item = Some(idx);
                    self.menu.sub = None;
                    if items[idx].has_submenu() {
                        self.menu.right(); // reveal the submenu on hover
                    }
                }
            }
        }
    }

    // ----- menu -----------------------------------------------------------

    fn menu_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Left => self.menu.left(),
            KeyCode::Right => self.menu.right(),
            KeyCode::Up => self.menu.up(),
            KeyCode::Down => self.menu.down(),
            KeyCode::Enter => {
                if let Some(action) = self.menu.enter() {
                    self.menu.close();
                    self.run_action(action);
                }
            }
            KeyCode::Esc => {
                if self.menu.sub.is_some() {
                    self.menu.sub = None;
                } else {
                    self.menu.close();
                }
            }
            KeyCode::F(10) => self.menu.close(),
            // Type-ahead: a plain letter jumps to the next matching item.
            KeyCode::Char(c) if !Self::ctrl(&key) && !Self::alt(&key) => {
                self.menu.type_ahead(c);
            }
            _ => {}
        }
    }

    // ----- theme chooser --------------------------------------------------

    /// Custom themes available to choose from: those installed in the user's
    /// themes directory first (so they win on a name clash), then the themes
    /// bundled into the binary.
    fn available_custom_themes() -> Vec<crate::theme::CustomTheme> {
        let mut themes = Settings::themes_dir()
            .map(|d| vix_theme_model::load_custom_themes(&d))
            .unwrap_or_default();
        themes.extend(bundled_themes());
        themes
    }

    /// Apply a persisted theme value by name (case-insensitive, so the default
    /// `"dark"` matches the bundled `Dark`). Falls back to `Dark`, then to the
    /// first available theme.
    fn apply_saved_theme(value: &str) {
        let themes = Self::available_custom_themes();
        let chosen = themes
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case(value))
            .or_else(|| themes.iter().find(|t| t.name.eq_ignore_ascii_case("dark")))
            .or_else(|| themes.first())
            .cloned();
        crate::theme::set_custom(chosen);
    }

    /// Apply the theme with display `name` (chosen from the View → Theme submenu),
    /// persist it, and restyle the editor. Unknown names are ignored.
    fn set_theme_by_name(&mut self, name: &str) {
        let Some(theme) =
            Self::available_custom_themes().into_iter().find(|t| t.name == name)
        else {
            return;
        };
        vix_theme_model::apply(&theme);
        self.editor.refresh_theme();
        self.settings.theme.clone_from(&theme.name);
        self.status = t!("status.theme", theme = theme.name).to_string();
    }

    // ----- info dialog ----------------------------------------------------

    /// Open a dialog whose body is a selectable/copyable text field.
    fn open_text_dialog(&mut self, title: String, text: &str) {
        self.dialog = Some(Dialog {
            title,
            body: text.to_string(),
            editor: Some(crate::editor::text_field(text)),
        });
    }

    /// The dialog's text-field rectangle, clamped to a width the editor can
    /// safely scroll within (mirrors [`App::editor_view`]).
    fn dialog_field_area(&self) -> Rect {
        let r = self.layout.dialog_body;
        Rect {
            width: r.width.max(MIN_EDITOR_WIDTH),
            height: r.height.max(1),
            ..r
        }
    }

    // ----- locale chooser -------------------------------------------------

    fn open_locale_chooser(&mut self) {
        self.locale_chooser = Some(LocaleChooser::open(&rust_i18n::locale()));
    }

    fn locale_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Down => {
                if let Some(lc) = self.locale_chooser.as_mut() {
                    if key.code == KeyCode::Up {
                        lc.up();
                    } else {
                        lc.down();
                    }
                    // Preview the highlighted language live.
                    rust_i18n::set_locale(lc.selected_code());
                }
            }
            KeyCode::Enter => {
                if let Some(lc) = self.locale_chooser.take() {
                    let code = lc.selected_code();
                    rust_i18n::set_locale(code);
                    self.settings.locale = code.to_string();
                    self.status = t!("status.locale", locale = code).to_string();
                }
            }
            KeyCode::Esc => {
                if let Some(lc) = self.locale_chooser.take() {
                    rust_i18n::set_locale(lc.original_code());
                    self.status = t!("status.locale_unchanged").to_string();
                }
            }
            _ => {}
        }
    }

    // ----- keymap ---------------------------------------------------------

    /// Apply the keymap with the given `id` (from the View → Keymap submenu),
    /// persist it, and reset per-keymap session state. Unknown ids are ignored.
    fn set_keymap(&mut self, id: &str) {
        let Some(km) = vix_keymap_model::by_id(id) else {
            return;
        };
        self.settings.keymap = km.id.to_string();
        self.reset_keymap_modes();
        self.status = t!("status.keymap", keymap = km.id).to_string();
    }

    // ----- time zone chooser ----------------------------------------------

    fn open_time_zone_chooser(&mut self) {
        self.time_zone_chooser = Some(TimeZoneChooser::open(vix_time_zone_model::active_name()));
    }

    fn time_zone_key(&mut self, key: KeyEvent) {
        let page = (self.layout.tz_chooser.height as usize).max(1);
        match key.code {
            KeyCode::Up => {
                if let Some(c) = self.time_zone_chooser.as_mut() {
                    c.up();
                }
            }
            KeyCode::Down => {
                if let Some(c) = self.time_zone_chooser.as_mut() {
                    c.down();
                }
            }
            KeyCode::PageUp => {
                if let Some(c) = self.time_zone_chooser.as_mut() {
                    c.page_up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(c) = self.time_zone_chooser.as_mut() {
                    c.page_down(page);
                }
            }
            KeyCode::Char(ch) => {
                if let Some(c) = self.time_zone_chooser.as_mut() {
                    c.push(ch);
                }
            }
            KeyCode::Backspace => {
                if let Some(c) = self.time_zone_chooser.as_mut() {
                    c.backspace();
                }
            }
            KeyCode::Enter => {
                if let Some(zone) =
                    self.time_zone_chooser.as_ref().and_then(TimeZoneChooser::selected_zone)
                {
                    vix_time_zone_model::set_active(zone.name);
                    self.settings.time_zone = zone.name.to_string();
                    self.status = t!("status.time_zone", zone = zone.name).to_string();
                }
                self.time_zone_chooser = None;
            }
            KeyCode::Esc => {
                self.time_zone_chooser = None;
                self.status = t!("status.time_zone_unchanged").to_string();
            }
            _ => {}
        }
    }

    /// Reset per-keymap session state (Emacs chord prefix, Vim mode/command line)
    /// so a freshly chosen keymap starts clean — Vim begins in Normal mode.
    fn reset_keymap_modes(&mut self) {
        self.emacs_prefix = false;
        self.vim_insert = false;
        self.vim_cmd = None;
    }

    // ----- recent-files chooser -------------------------------------------

    /// Open the recent-files chooser, listing the saved recent paths that still
    /// exist. Does nothing (just a status note) when there are none.
    fn open_recent_chooser(&mut self) {
        let entries: Vec<PathBuf> = self
            .settings
            .recent_files
            .iter()
            .map(PathBuf::from)
            .filter(|p| p.is_file())
            .collect();
        if entries.is_empty() {
            self.status = t!("status.no_recent").to_string();
            return;
        }
        self.recent_chooser = Some(RecentChooser { entries, selected: 0 });
    }

    fn recent_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(rc) = self.recent_chooser.as_mut() {
                    let n = rc.entries.len();
                    rc.selected = (rc.selected + n - 1) % n;
                }
            }
            KeyCode::Down => {
                if let Some(rc) = self.recent_chooser.as_mut() {
                    rc.selected = (rc.selected + 1) % rc.entries.len();
                }
            }
            KeyCode::Enter => self.open_selected_recent(),
            KeyCode::Esc => {
                self.recent_chooser = None;
            }
            _ => {}
        }
    }

    /// Open the highlighted recent file and close the chooser.
    fn open_selected_recent(&mut self) {
        if let Some(rc) = self.recent_chooser.take() {
            if let Some(path) = rc.entries.get(rc.selected).cloned() {
                self.with_jump(|s| {
                    s.open_path(&path, false);
                    s.focus = Focus::Editor;
                });
            }
        }
    }

    fn recent_mouse(&mut self, mouse: MouseEvent) {
        if let Some(idx) = self.chooser_row(mouse) {
            if let Some(rc) = self.recent_chooser.as_mut() {
                if idx < rc.entries.len() {
                    // A click selects the row and opens it (no live preview to
                    // justify a two-step interaction).
                    rc.selected = idx;
                    self.open_selected_recent();
                }
            }
        }
    }

    // ----- chooser mouse --------------------------------------------------

    /// The row index a mouse event lands on within the open chooser's list
    /// rectangle, or `None` if it is outside the list.
    fn chooser_row(&self, mouse: MouseEvent) -> Option<usize> {
        let r = self.layout.chooser;
        (matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
            && rect_contains(r, mouse.column, mouse.row))
        .then(|| (mouse.row - r.y) as usize)
    }

    fn locale_mouse(&mut self, mouse: MouseEvent) {
        if let Some(idx) = self.chooser_row(mouse) {
            if let Some(lc) = self.locale_chooser.as_mut() {
                if idx < vix_locale_chooser::LOCALES.len() {
                    lc.selected = idx;
                    rust_i18n::set_locale(lc.selected_code());
                }
            }
        }
    }

    fn time_zone_mouse(&mut self, mouse: MouseEvent) {
        let list = self.layout.tz_chooser;
        let sb = self.layout.tz_scrollbar;
        let viewport = (list.height as usize).max(1);
        match mouse.kind {
            // Press or drag on the scrollbar gutter moves the highlight.
            MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left)
                if rect_contains(sb, mouse.column, mouse.row) =>
            {
                if let Some(c) = self.time_zone_chooser.as_mut() {
                    let max = c.len().saturating_sub(1);
                    let pos = crate::ui::scrollbar_pos_from_row(sb, mouse.row, max);
                    c.select(pos);
                    c.ensure_visible(viewport);
                }
            }
            // A click on a row accepts that zone.
            MouseEventKind::Down(MouseButton::Left) if rect_contains(list, mouse.column, mouse.row) => {
                let row_in_view = (mouse.row - list.y) as usize;
                let pick = self.time_zone_chooser.as_mut().and_then(|c| {
                    let idx = c.scroll + row_in_view;
                    (idx < c.len()).then(|| {
                        c.select(idx);
                        c.selected_zone()
                    })?
                });
                if let Some(zone) = pick {
                    vix_time_zone_model::set_active(zone.name);
                    self.settings.time_zone = zone.name.to_string();
                    self.status = t!("status.time_zone", zone = zone.name).to_string();
                    self.time_zone_chooser = None;
                }
            }
            MouseEventKind::ScrollUp => {
                if let Some(c) = self.time_zone_chooser.as_mut() {
                    c.scroll = c.scroll.saturating_sub(3);
                }
            }
            MouseEventKind::ScrollDown => {
                if let Some(c) = self.time_zone_chooser.as_mut() {
                    let max = c.len().saturating_sub(viewport);
                    c.scroll = (c.scroll + 3).min(max);
                }
            }
            _ => {}
        }
    }

    // ----- Nerd Font palette ----------------------------------------------

    fn open_nerd_palette(&mut self) {
        self.nerd_palette = Some(NerdPalette::open());
    }

    fn nerd_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.nerd_palette.as_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.nerd_palette.as_mut() {
                    p.down();
                }
            }
            KeyCode::Left => {
                if let Some(p) = self.nerd_palette.as_mut() {
                    p.left();
                }
            }
            KeyCode::Right => {
                if let Some(p) = self.nerd_palette.as_mut() {
                    p.right();
                }
            }
            // Enter inserts and keeps the palette open so several glyphs can be
            // picked in a row; Esc closes it.
            KeyCode::Enter => self.insert_selected_glyph(),
            KeyCode::Esc => self.nerd_palette = None,
            _ => {}
        }
    }

    fn nerd_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.nerd_palette;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let col = ((mouse.column - r.x) / crate::ui::NERD_CELL_W) as usize;
        let row = (mouse.row - r.y) as usize;
        if let Some(p) = self.nerd_palette.as_mut() {
            if p.select_at(row, col) {
                self.insert_selected_glyph();
            }
        }
    }

    /// Insert the highlighted glyph into the active editor (leaving the palette
    /// open). No-op when there is no editable buffer (e.g. an image tab).
    fn insert_selected_glyph(&mut self) {
        let Some(p) = self.nerd_palette.as_ref() else {
            return;
        };
        let glyph = p.selected_glyph();
        let name = p.selected_name();
        let area = self.layout.editor;
        if self.editor.insert_str(&glyph.to_string(), area) {
            self.status = t!("status.glyph_inserted", name = name).to_string();
        }
    }

    // ----- ASCII panel ----------------------------------------------------

    fn open_ascii_panel(&mut self) {
        self.ascii_panel = Some(AsciiPanel::open());
    }

    fn ascii_key(&mut self, key: KeyEvent) {
        let page = (self.layout.ascii_panel.height as usize).max(1);
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.ascii_panel.as_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.ascii_panel.as_mut() {
                    p.down();
                }
            }
            KeyCode::PageUp => {
                if let Some(p) = self.ascii_panel.as_mut() {
                    p.page_up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(p) = self.ascii_panel.as_mut() {
                    p.page_down(page);
                }
            }
            KeyCode::Home => {
                if let Some(p) = self.ascii_panel.as_mut() {
                    p.page_up(p.len());
                }
            }
            KeyCode::End => {
                if let Some(p) = self.ascii_panel.as_mut() {
                    p.page_down(p.len());
                }
            }
            // Enter inserts and keeps the panel open so several characters can be
            // picked in a row; Esc closes it.
            KeyCode::Enter => self.insert_selected_ascii(),
            KeyCode::Esc => self.ascii_panel = None,
            _ => {}
        }
    }

    fn ascii_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.ascii_panel;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row_in_view = (mouse.row - r.y) as usize;
        if let Some(p) = self.ascii_panel.as_mut() {
            let idx = p.scroll + row_in_view;
            if p.select_index(idx) {
                self.insert_selected_ascii();
            }
        }
    }

    /// Insert the highlighted character into the active editor (leaving the panel
    /// open). No-op when there is no editable buffer (e.g. an image tab).
    fn insert_selected_ascii(&mut self) {
        let Some(p) = self.ascii_panel.as_ref() else {
            return;
        };
        let ch = p.selected_char();
        let name = p.selected_label();
        let area = self.layout.editor;
        if self.editor.insert_str(&ch.to_string(), area) {
            self.status = t!("status.ascii_inserted", name = name).to_string();
        }
    }

    // ----- X11 color palette ----------------------------------------------

    fn open_x11_panel(&mut self) {
        self.x11_panel = Some(X11Panel::open());
    }

    fn x11_key(&mut self, key: KeyEvent) {
        let page = (self.layout.x11_panel.height as usize).max(1);
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.x11_panel.as_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.x11_panel.as_mut() {
                    p.down();
                }
            }
            KeyCode::PageUp => {
                if let Some(p) = self.x11_panel.as_mut() {
                    p.page_up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(p) = self.x11_panel.as_mut() {
                    p.page_down(page);
                }
            }
            KeyCode::Home => {
                if let Some(p) = self.x11_panel.as_mut() {
                    p.page_up(p.len());
                }
            }
            KeyCode::End => {
                if let Some(p) = self.x11_panel.as_mut() {
                    p.page_down(p.len());
                }
            }
            // Enter inserts and keeps the panel open so several colors can be
            // picked in a row; Esc closes it.
            KeyCode::Enter => self.insert_selected_x11(),
            KeyCode::Esc => self.x11_panel = None,
            _ => {}
        }
    }

    fn x11_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.x11_panel;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row_in_view = (mouse.row - r.y) as usize;
        if let Some(p) = self.x11_panel.as_mut() {
            let idx = p.scroll + row_in_view;
            if p.select_index(idx) {
                self.insert_selected_x11();
            }
        }
    }

    /// Insert the highlighted color's hex (e.g. `#F0F8FF`) into the active editor
    /// (leaving the panel open). No-op when there is no editable buffer.
    fn insert_selected_x11(&mut self) {
        let Some(p) = self.x11_panel.as_ref() else {
            return;
        };
        let hex = p.selected_hex().to_string();
        let name = p.selected_name().to_string();
        let area = self.layout.editor;
        if self.editor.insert_str(&hex, area) {
            self.status = t!("status.x11_inserted", name = name).to_string();
        }
    }

    // ----- HTML character palette -----------------------------------------

    fn open_html_panel(&mut self) {
        self.html_panel = Some(HtmlPanel::open());
    }

    fn html_key(&mut self, key: KeyEvent) {
        let page = (self.layout.html_panel.height as usize).max(1);
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.html_panel.as_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.html_panel.as_mut() {
                    p.down();
                }
            }
            KeyCode::PageUp => {
                if let Some(p) = self.html_panel.as_mut() {
                    p.page_up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(p) = self.html_panel.as_mut() {
                    p.page_down(page);
                }
            }
            KeyCode::Home => {
                if let Some(p) = self.html_panel.as_mut() {
                    p.page_up(p.len());
                }
            }
            KeyCode::End => {
                if let Some(p) = self.html_panel.as_mut() {
                    p.page_down(p.len());
                }
            }
            // Enter inserts and keeps the panel open so several entities can be
            // picked in a row; Esc closes it.
            KeyCode::Enter => self.insert_selected_html(),
            KeyCode::Esc => self.html_panel = None,
            _ => {}
        }
    }

    fn html_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.html_panel;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row_in_view = (mouse.row - r.y) as usize;
        let rel_col = mouse.column.saturating_sub(r.x) as usize;
        // A click picks the individual cell under the pointer — glyph, name, or
        // code — and inserts just that cell's text.
        let cell = self.html_panel.as_mut().and_then(|p| {
            let idx = p.scroll + row_in_view;
            p.select_index(idx).then(|| html_cell_at(p.selected_entity(), rel_col))
        });
        if let Some(text) = cell {
            let area = self.layout.editor;
            if self.editor.insert_str(&text, area) {
                self.status = t!("status.html_inserted", name = text).to_string();
            }
        }
    }

    /// Insert the highlighted entity's glyph into the active editor (leaving the
    /// panel open) — the keyboard equivalent of clicking the glyph cell. No-op
    /// when there is no editable buffer.
    fn insert_selected_html(&mut self) {
        let Some(p) = self.html_panel.as_ref() else {
            return;
        };
        let glyph = p.selected_entity().glyph.to_string();
        let area = self.layout.editor;
        if self.editor.insert_str(&glyph, area) {
            self.status = t!("status.html_inserted", name = glyph).to_string();
        }
    }

    // ----- Welcome panel --------------------------------------------------

    fn open_welcome(&mut self) {
        self.welcome = Some(WelcomePanel::open(Self::welcome_lines()));
    }

    /// The welcome text from the i18n catalog, split into lines.
    fn welcome_lines() -> Vec<String> {
        t!("welcome.body").lines().map(str::to_string).collect()
    }

    /// Open the user's settings file in the editor. Saves the current settings
    /// first so the file exists (and reflects in-app changes) before opening.
    fn open_settings_file(&mut self) {
        let Some(path) = Settings::config_path() else {
            self.status = t!("status.settings_no_path").to_string();
            return;
        };
        let _ = self.settings.save();
        self.with_jump(|s| {
            s.open_path(&path, false);
            s.focus = Focus::Editor;
        });
    }

    fn welcome_key(&mut self, key: KeyEvent) {
        let page = (self.layout.welcome.height as usize).max(1);
        match key.code {
            KeyCode::Up => {
                if let Some(w) = self.welcome.as_mut() {
                    w.up();
                }
            }
            KeyCode::Down => {
                if let Some(w) = self.welcome.as_mut() {
                    w.down(page);
                }
            }
            KeyCode::PageUp => {
                if let Some(w) = self.welcome.as_mut() {
                    w.page_up(page);
                }
            }
            KeyCode::PageDown | KeyCode::Char(' ') => {
                if let Some(w) = self.welcome.as_mut() {
                    w.page_down(page);
                }
            }
            KeyCode::Home => {
                if let Some(w) = self.welcome.as_mut() {
                    w.page_up(w.len());
                }
            }
            KeyCode::End => {
                if let Some(w) = self.welcome.as_mut() {
                    w.page_down(w.len());
                }
            }
            KeyCode::Esc | KeyCode::Enter | KeyCode::F(1) | KeyCode::Char('q') => {
                self.welcome = None;
            }
            _ => {}
        }
    }

    fn welcome_mouse(&mut self, mouse: MouseEvent) {
        let page = (self.layout.welcome.height as usize).max(1);
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                if let Some(w) = self.welcome.as_mut() {
                    w.up();
                }
            }
            MouseEventKind::ScrollDown => {
                if let Some(w) = self.welcome.as_mut() {
                    w.down(page);
                }
            }
            _ => {}
        }
    }

    // ----- AI -------------------------------------------------------------

    /// Run the `claude` CLI with `prompt`, feeding it `text` on stdin and
    /// streaming the response into the bottom dock.
    fn ai_run_on_text(&mut self, prompt: &str, text: &str) {
        if text.trim().is_empty() {
            self.status = t!("status.ai_no_input").to_string();
            return;
        }
        let tmp = std::env::temp_dir().join(format!("vix-ai-{}.txt", std::process::id()));
        if std::fs::write(&tmp, text).is_err() {
            self.status = t!("status.ai_no_input").to_string();
            return;
        }
        let path = tmp.display();
        self.run_command(&format!("claude -p \"{prompt}\" < \"{path}\""));
    }

    /// The selected text, or the whole active buffer when nothing is selected.
    fn selected_or_all_text(&mut self) -> String {
        let selection = self.editor.active_tab_mut().and_then(|t| t.editor.get_selection_text());
        match selection {
            Some(s) if !s.trim().is_empty() => s,
            _ => self.editor.active_tab().map(|t| t.editor.get_content()).unwrap_or_default(),
        }
    }

    /// Summarize the selection (or the whole file when nothing is selected) with
    /// `claude` (output to the dock).
    fn ai_summarize(&mut self) {
        let text = self.selected_or_all_text();
        self.ai_run_on_text("Summarize this text.", &text);
    }

    /// Explain the selection (or the whole file when nothing is selected) with
    /// `claude` (output to the dock).
    fn ai_explain(&mut self) {
        let text = self.selected_or_all_text();
        self.ai_run_on_text("Explain this text.", &text);
    }

    /// Define a word with `claude` (output to the dock). The input is the
    /// selection if there is one; otherwise the word under the cursor, or the
    /// next word when the cursor sits between words. Never the whole buffer.
    fn ai_define(&mut self) {
        let text = self.selected_or_word_text();
        self.ai_run_on_text("Define this text.", &text);
    }

    /// The selection, else the word at the cursor, else the next word after it.
    /// Returns an empty string when there is no editable buffer or no word ahead.
    fn selected_or_word_text(&mut self) -> String {
        let Some(tab) = self.editor.active_tab_mut() else {
            return String::new();
        };
        if let Some(sel) = tab.editor.get_selection_text() {
            if !sel.trim().is_empty() {
                return sel;
            }
        }
        let cursor = tab.editor.get_cursor();
        if let Some((_, _, word)) = tab.editor.word_at(cursor) {
            return word;
        }
        // Cursor is between words: scan forward to the next word.
        let len = tab.editor.get_content().chars().count();
        for pos in cursor..=len {
            if let Some((_, _, word)) = tab.editor.word_at(pos) {
                return word;
            }
        }
        String::new()
    }

    /// Annotate the selection (or the whole file when nothing is selected) with
    /// `claude`, replacing it with the result.
    fn ai_annotate(&mut self) {
        self.ai_replace_text("Annotate this text.", &t!("menu.item.ai.annotate"));
    }

    /// Improve the selection (or the whole file when nothing is selected) with
    /// `claude`, replacing it with the result.
    fn ai_improve(&mut self) {
        self.ai_replace_text("Improve this text.", &t!("menu.item.ai.improve"));
    }

    /// Launch `claude -p <prompt>` over the selection (or the whole buffer when
    /// nothing is selected), capturing its full output to replace that text when
    /// it finishes. The transform runs in the background; [`Self::poll_ai_replace`]
    /// applies the result.
    fn ai_replace_text(&mut self, prompt: &str, label: &str) {
        if self.ai_replace.is_some() {
            self.status = t!("status.ai_busy").to_string();
            return;
        }
        let tab_idx = self.editor.active;
        let (text, target) = {
            let Some(tab) = self.editor.active_tab_mut() else { return };
            if tab.is_image() {
                self.status = t!("status.ai_no_input").to_string();
                return;
            }
            match tab.editor.get_selection() {
                Some(sel) if !sel.is_empty() => {
                    (tab.editor.get_content_slice(sel.start, sel.end), AiTarget::Range(sel.start, sel.end))
                }
                _ => (tab.editor.get_content(), AiTarget::Whole),
            }
        };
        if text.trim().is_empty() {
            self.status = t!("status.ai_no_input").to_string();
            return;
        }

        let tmp = std::env::temp_dir().join(format!("vix-ai-{}.txt", std::process::id()));
        if std::fs::write(&tmp, &text).is_err() {
            self.status = t!("status.ai_no_input").to_string();
            return;
        }
        let path = tmp.display();
        let cmd = format!("claude -p \"{prompt}\" < \"{path}\"");
        let mut child = match std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .current_dir(&self.root)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                self.messages.error(t!("msg.command_failed", error = e).to_string());
                return;
            }
        };
        let stdout = child.stdout.take().expect("piped stdout");
        let child = std::sync::Arc::new(std::sync::Mutex::new(child));
        let (tx, rx) = std::sync::mpsc::channel();
        let reader_child = child.clone();
        std::thread::spawn(move || {
            use std::io::Read;
            let mut out = String::new();
            let ok = std::io::BufReader::new(stdout).read_to_string(&mut out).is_ok();
            let status = reader_child.lock().expect("ai lock").wait().ok();
            let success = ok && status.and_then(|s| s.code()) == Some(0);
            let _ = tx.send(if success { AiMsg::Done(out) } else { AiMsg::Failed });
        });
        self.ai_replace = Some(AiReplace { rx, tab: tab_idx, target, label: label.to_string() });
        self.status = t!("status.ai_running", action = label).to_string();
    }

    /// Drain a finished AI transform and apply its result. Called once per
    /// event-loop iteration; cheap when none is running.
    pub fn poll_ai_replace(&mut self) {
        let msg = {
            let Some(ar) = self.ai_replace.as_ref() else {
                return;
            };
            match ar.rx.try_recv() {
                Ok(m) => m,
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => AiMsg::Failed,
            }
        };
        let Some(ar) = self.ai_replace.take() else {
            return;
        };
        match msg {
            AiMsg::Done(out) => {
                let text = out.trim_end_matches('\n');
                if text.is_empty() {
                    self.status = t!("status.ai_failed", action = ar.label).to_string();
                    return;
                }
                self.apply_ai_replace(ar.tab, ar.target, text);
                self.status = t!("status.ai_done", action = ar.label).to_string();
            }
            AiMsg::Failed => {
                self.status = t!("status.ai_failed", action = ar.label).to_string();
            }
        }
    }

    /// Write an AI transform's `text` back into tab `tab_idx`, replacing either
    /// the whole buffer or the recorded character range.
    fn apply_ai_replace(&mut self, tab_idx: usize, target: AiTarget, text: &str) {
        let Some(tab) = self.editor.tabs.get_mut(tab_idx) else {
            return;
        };
        let new = match target {
            AiTarget::Whole => text.to_string(),
            AiTarget::Range(start, end) => {
                let chars: Vec<char> = tab.editor.get_content().chars().collect();
                let n = chars.len();
                let start = start.min(n);
                let end = end.min(n).max(start);
                let mut out: String = chars[..start].iter().collect();
                out.push_str(text);
                out.extend(&chars[end..]);
                out
            }
        };
        tab.editor.set_content(&new);
        tab.editor.set_selection(None);
        tab.dirty = true;
    }

    /// Whether a background AI transform is in progress.
    #[must_use]
    pub fn ai_replace_running(&self) -> bool {
        self.ai_replace.is_some()
    }

    // ----- Contacts (vCard browser) ---------------------------------------

    /// Open the contact browser over the configured vCard directory (or the
    /// workspace root), parsing each `.vcf`'s display name.
    fn open_contacts(&mut self) {
        let dir = if self.settings.contacts_dir.trim().is_empty() {
            self.root.clone()
        } else {
            PathBuf::from(self.settings.contacts_dir.trim())
        };
        let mut contacts = Vec::new();
        if let Ok(read) = std::fs::read_dir(&dir) {
            for entry in read.flatten() {
                let path = entry.path();
                let is_vcf = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("vcf"));
                if !is_vcf {
                    continue;
                }
                let name = std::fs::read_to_string(&path)
                    .ok()
                    .map(|t| vix_vcard_parser::parse(&t).display_name())
                    .filter(|n| n != "(unnamed)")
                    .unwrap_or_else(|| {
                        path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
                    });
                contacts.push(vix_contact_panel::Contact { name, path });
            }
        }
        contacts.sort_by_key(|c| c.name.to_lowercase());
        if contacts.is_empty() {
            self.status = t!("status.no_contacts").to_string();
        }
        self.contacts = Some(ContactPanel::open(contacts));
    }

    /// Open the highlighted contact's vCard in the single-vCard view.
    fn open_selected_vcard(&mut self) {
        let Some(path) = self.contacts.as_ref().and_then(ContactPanel::selected_path) else {
            return;
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => self.vcard = Some(VcardPanel::open(vix_vcard_parser::parse(&text))),
            Err(e) => self.messages.error(t!("msg.open_failed", error = e).to_string()),
        }
    }

    fn contacts_key(&mut self, key: KeyEvent) {
        let page = (self.layout.contacts.height as usize).max(1);
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.contacts.as_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.contacts.as_mut() {
                    p.down();
                }
            }
            KeyCode::PageUp => {
                if let Some(p) = self.contacts.as_mut() {
                    p.page_up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(p) = self.contacts.as_mut() {
                    p.page_down(page);
                }
            }
            KeyCode::Home => {
                if let Some(p) = self.contacts.as_mut() {
                    p.page_up(p.len());
                }
            }
            KeyCode::End => {
                if let Some(p) = self.contacts.as_mut() {
                    p.page_down(p.len());
                }
            }
            KeyCode::Enter => self.open_selected_vcard(),
            KeyCode::Esc => self.contacts = None,
            _ => {}
        }
    }

    fn contacts_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.contacts;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row_in_view = (mouse.row - r.y) as usize;
        let hit = self.contacts.as_mut().is_some_and(|p| p.select_index(p.scroll + row_in_view));
        if hit {
            self.open_selected_vcard();
        }
    }

    fn vcard_key(&mut self, key: KeyEvent) {
        let page = (self.layout.vcard.height as usize).max(1);
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.vcard.as_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.vcard.as_mut() {
                    p.down();
                }
            }
            KeyCode::PageUp => {
                if let Some(p) = self.vcard.as_mut() {
                    p.page_up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(p) = self.vcard.as_mut() {
                    p.page_down(page);
                }
            }
            KeyCode::Home => {
                if let Some(p) = self.vcard.as_mut() {
                    p.page_up(p.len());
                }
            }
            KeyCode::End => {
                if let Some(p) = self.vcard.as_mut() {
                    p.page_down(p.len());
                }
            }
            KeyCode::Enter => self.insert_selected_vcard_value(),
            // Esc returns to the contact browser (or closes if opened directly).
            KeyCode::Esc => self.vcard = None,
            _ => {}
        }
    }

    fn vcard_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.vcard;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row_in_view = (mouse.row - r.y) as usize;
        let hit = self.vcard.as_mut().is_some_and(|p| p.select_index(p.scroll + row_in_view));
        if hit {
            self.insert_selected_vcard_value();
        }
    }

    /// Insert the highlighted vCard field's value into the active editor.
    fn insert_selected_vcard_value(&mut self) {
        let Some(p) = self.vcard.as_ref() else { return };
        let value = p.selected_value();
        if value.is_empty() {
            return;
        }
        let area = self.layout.editor;
        if self.editor.insert_str(&value, area) {
            self.status = t!("status.ascii_inserted", name = value).to_string();
        }
    }

    // ----- File Information panel -----------------------------------------

    fn open_file_info(&mut self) {
        let info = self.gather_file_info();
        self.file_info = Some(FileInfoPanel::open(&info));
    }

    /// Collect facts about the active file: counts from the buffer, and size /
    /// permissions / modified-time from the filesystem when it is saved.
    fn gather_file_info(&self) -> vix_file_information_panel::FileInfo {
        use vix_file_information_panel::FileInfo;
        let mut info = FileInfo::default();
        let Some(t) = self.editor.active_tab() else { return info };
        let content = t.editor.get_content();
        info.language = t.editor.language().to_string();
        info.chars = content.chars().count();
        info.words = content.split_whitespace().count();
        info.lines = t.editor.code_ref().len_lines();
        info.dirty = t.dirty;
        if let Some(path) = &t.path {
            info.name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            info.path = path.display().to_string();
            if let Ok(meta) = std::fs::metadata(path) {
                info.bytes = Some(meta.len());
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    info.mode = Some(meta.permissions().mode());
                }
                if let Ok(modified) = meta.modified() {
                    if let Ok(d) = modified.duration_since(std::time::UNIX_EPOCH) {
                        info.modified_secs = i64::try_from(d.as_secs()).ok();
                    }
                }
            }
        }
        info
    }

    fn file_info_key(&mut self, key: KeyEvent) {
        let page = (self.layout.file_info.height as usize).max(1);
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.file_info.as_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.file_info.as_mut() {
                    p.down();
                }
            }
            KeyCode::PageUp => {
                if let Some(p) = self.file_info.as_mut() {
                    p.page_up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(p) = self.file_info.as_mut() {
                    p.page_down(page);
                }
            }
            KeyCode::Home => {
                if let Some(p) = self.file_info.as_mut() {
                    p.page_up(p.len());
                }
            }
            KeyCode::End => {
                if let Some(p) = self.file_info.as_mut() {
                    p.page_down(p.len());
                }
            }
            KeyCode::Enter => self.insert_selected_file_info(),
            KeyCode::Esc => self.file_info = None,
            _ => {}
        }
    }

    fn file_info_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.file_info;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row_in_view = (mouse.row - r.y) as usize;
        if let Some(p) = self.file_info.as_mut() {
            let idx = p.scroll + row_in_view;
            if p.select_index(idx) {
                self.insert_selected_file_info();
            }
        }
    }

    /// Insert the highlighted value into the active editor (leaving the panel open).
    fn insert_selected_file_info(&mut self) {
        let Some(p) = self.file_info.as_ref() else {
            return;
        };
        let value = p.selected_value();
        if value.is_empty() {
            return;
        }
        let area = self.layout.editor;
        if self.editor.insert_str(&value, area) {
            self.status = t!("status.ascii_inserted", name = value).to_string();
        }
    }

    // ----- System Information panel ---------------------------------------

    fn open_system_info(&mut self) {
        self.system_info = Some(SystemInfoPanel::open());
    }

    fn system_info_key(&mut self, key: KeyEvent) {
        let page = (self.layout.system_info.height as usize).max(1);
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.system_info.as_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.system_info.as_mut() {
                    p.down();
                }
            }
            KeyCode::PageUp => {
                if let Some(p) = self.system_info.as_mut() {
                    p.page_up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(p) = self.system_info.as_mut() {
                    p.page_down(page);
                }
            }
            KeyCode::Home => {
                if let Some(p) = self.system_info.as_mut() {
                    p.page_up(p.len());
                }
            }
            KeyCode::End => {
                if let Some(p) = self.system_info.as_mut() {
                    p.page_down(p.len());
                }
            }
            // Enter inserts the highlighted value and keeps the panel open; Esc closes.
            KeyCode::Enter => self.insert_selected_system_info(),
            KeyCode::Esc => self.system_info = None,
            _ => {}
        }
    }

    fn system_info_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.system_info;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row_in_view = (mouse.row - r.y) as usize;
        if let Some(p) = self.system_info.as_mut() {
            let idx = p.scroll + row_in_view;
            if p.select_index(idx) {
                self.insert_selected_system_info();
            }
        }
    }

    /// Insert the highlighted value into the active editor (leaving the panel
    /// open). Section-heading rows have no value, so they insert nothing.
    fn insert_selected_system_info(&mut self) {
        let Some(p) = self.system_info.as_ref() else {
            return;
        };
        let value = p.selected_value();
        if value.is_empty() {
            return;
        }
        let area = self.layout.editor;
        if self.editor.insert_str(&value, area) {
            self.status = t!("status.ascii_inserted", name = value).to_string();
        }
    }

    // ----- workspace dashboard ----------------------------------------------

    /// Open the Workspace Dashboard and kick off the background metric computations
    /// (disk usage via `du`, a recursive file count, and the git commit count).
    fn open_dashboard(&mut self) {
        let folder = self
            .root
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.root.display().to_string());
        self.dashboard = Some(Dashboard::new(folder));

        let (tx, rx) = std::sync::mpsc::channel();
        let root = self.root.clone();

        let dtx = tx.clone();
        let droot = root.clone();
        std::thread::spawn(move || {
            if let Ok(out) = std::process::Command::new("du").arg("-sh").arg(&droot).output() {
                if out.status.success() {
                    let text = String::from_utf8_lossy(&out.stdout);
                    if let Some(size) = text.split_whitespace().next() {
                        let _ = dtx.send(DashMsg::Disk(size.to_string()));
                    }
                }
            }
        });

        let ftx = tx.clone();
        let froot = root.clone();
        std::thread::spawn(move || {
            let _ = ftx.send(DashMsg::Files(count_files(&froot)));
        });

        std::thread::spawn(move || {
            let _ = tx.send(DashMsg::Commits(vix_git::commit_count(&root).unwrap_or(0)));
        });

        self.dashboard_rx = Some(rx);
    }

    fn close_dashboard(&mut self) {
        self.dashboard = None;
        self.dashboard_rx = None;
    }

    /// Whether the dashboard is open with metrics still computing (the run loop
    /// ticks faster then, so values appear promptly).
    #[must_use]
    pub fn dashboard_loading(&self) -> bool {
        self.dashboard.as_ref().is_some_and(|d| !d.is_complete())
    }

    /// Drain any finished dashboard metrics into the open panel.
    pub fn poll_dashboard(&mut self) {
        if self.dashboard.is_none() {
            self.dashboard_rx = None;
            return;
        }
        let Some(rx) = self.dashboard_rx.as_ref() else {
            return;
        };
        let mut msgs = Vec::new();
        while let Ok(m) = rx.try_recv() {
            msgs.push(m);
        }
        if let Some(d) = self.dashboard.as_mut() {
            for m in msgs {
                match m {
                    DashMsg::Disk(s) => d.disk_usage = Some(s),
                    DashMsg::Files(n) => d.file_count = Some(n),
                    DashMsg::Commits(n) => d.commit_count = Some(n),
                }
            }
        }
    }

    // ----- code outline ---------------------------------------------------

    /// Open the outline panel for the active buffer, selecting the symbol the
    /// cursor is currently inside. Reports a status when there are no symbols.
    fn open_outline(&mut self) {
        let cursor_line = self.editor.cursor_1based().0;
        let entries: Vec<vix_outline_panel::Entry> = self
            .editor
            .active_tab()
            .filter(|t| !t.is_image())
            .map(|t| {
                crate::palette::symbols(&t.text())
                    .into_iter()
                    .map(|s| vix_outline_panel::Entry { kind: s.kind, name: s.name, line: s.line })
                    .collect()
            })
            .unwrap_or_default();
        if entries.is_empty() {
            self.status = t!("status.outline_empty").into();
            return;
        }
        let mut outline = Outline::new(entries);
        outline.select_nearest(cursor_line);
        self.outline = Some(outline);
    }

    fn outline_key(&mut self, key: KeyEvent) {
        let page = (self.layout.outline.height as usize).max(1);
        match key.code {
            KeyCode::Up => {
                if let Some(o) = self.outline.as_mut() {
                    o.up();
                }
            }
            KeyCode::Down => {
                if let Some(o) = self.outline.as_mut() {
                    o.down();
                }
            }
            KeyCode::PageUp => {
                if let Some(o) = self.outline.as_mut() {
                    o.page_up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(o) = self.outline.as_mut() {
                    o.page_down(page);
                }
            }
            KeyCode::Home => {
                if let Some(o) = self.outline.as_mut() {
                    o.page_up(o.len());
                }
            }
            KeyCode::End => {
                if let Some(o) = self.outline.as_mut() {
                    o.page_down(o.len());
                }
            }
            KeyCode::Enter => self.jump_to_outline(),
            KeyCode::Esc => self.outline = None,
            _ => {}
        }
    }

    fn outline_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.outline;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row = (mouse.row - r.y) as usize;
        if let Some(o) = self.outline.as_mut() {
            let idx = o.scroll + row;
            if o.select_index(idx) {
                self.jump_to_outline();
            }
        }
    }

    /// Jump the cursor to the highlighted outline symbol and close the panel.
    fn jump_to_outline(&mut self) {
        let Some(line) = self.outline.as_ref().and_then(vix_outline_panel::Outline::selected_line)
        else {
            return;
        };
        self.outline = None;
        self.with_jump(|s| {
            let area = s.editor_view();
            s.editor.goto(line, None, area);
            s.focus = Focus::Editor;
        });
    }

    // ----- command palette ------------------------------------------------

    fn open_palette(&mut self) {
        self.open_palette_seeded("");
    }

    /// Open the palette with `seed` already typed (e.g. `"@"` to land in
    /// go-to-symbol mode).
    fn open_palette_seeded(&mut self, seed: &str) {
        self.build_file_index();
        self.palette_origin = self.editor.active_tab().map(|t| t.editor.get_cursor());
        let mut p = Palette::new();
        p.input = seed.to_string();
        self.palette = Some(p);
        self.recompute_palette();
    }

    /// Live `:` go-to-line preview: move the cursor to the number being typed.
    /// Always reverts to the captured origin first, so editing the number (or
    /// leaving go-to-line mode) tracks the latest input.
    fn preview_goto_line(&mut self) {
        self.restore_palette_origin();
        let Some(p) = self.palette.as_ref() else {
            return;
        };
        if p.mode() != PMode::GotoLine {
            return;
        }
        let Ok(n) = p.query().trim().parse::<usize>() else {
            return;
        };
        if n >= 1 {
            let area = self.editor_view();
            self.editor.goto(n, None, area);
        }
    }

    /// Return the cursor to where it was when the palette opened (used to revert a
    /// go-to-line preview).
    fn restore_palette_origin(&mut self) {
        let Some(off) = self.palette_origin else {
            return;
        };
        let area = self.editor_view();
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_cursor(off);
            t.editor.focus(&area);
        }
    }

    fn build_file_index(&mut self) {
        let mut out = Vec::new();
        Self::walk_files(&self.root, &mut out, 0);
        self.file_index = out;
    }

    fn walk_files(dir: &std::path::Path, out: &mut Vec<PathBuf>, depth: usize) {
        if depth > 12 || out.len() > 5000 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
            if path.is_dir() {
                Self::walk_files(&path, out, depth + 1);
            } else {
                out.push(path);
            }
        }
    }

    fn recompute_palette(&mut self) {
        let Some(p) = self.palette.as_ref() else {
            return;
        };
        let mode = p.mode();
        let query = p.query().to_string();
        let mut entries: Vec<Entry> = Vec::new();
        match mode {
            PMode::Files => {
                let (qpath, target) = palette::parse_path_target(&query);
                for path in &self.file_index {
                    let rel = path
                        .strip_prefix(&self.root)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .into_owned();
                    if qpath.is_empty() || palette::fuzzy_match(&rel, &qpath) {
                        entries.push(Entry {
                            label: rel,
                            action: PAction::OpenFile(path.clone(), target),
                        });
                    }
                    if entries.len() >= 200 {
                        break;
                    }
                }
            }
            PMode::Commands => {
                for (label_key, action) in palette::COMMANDS {
                    let label = t!(*label_key).to_string();
                    if query.is_empty() || palette::fuzzy_match(&label, &query) {
                        entries.push(Entry {
                            label: format!("> {label}"),
                            action: PAction::RunCommand((*action).to_string()),
                        });
                    }
                }
            }
            PMode::Buffers => {
                for (i, tab) in self.editor.tabs.iter().enumerate() {
                    let label = tab.display_path();
                    if query.is_empty() || palette::fuzzy_match(&label, &query) {
                        entries.push(Entry {
                            label: format!("# {label}"),
                            action: PAction::SwitchBuffer(i),
                        });
                    }
                }
            }
            PMode::GotoLine => {
                if let Ok(n) = query.trim().parse::<usize>() {
                    entries.push(Entry {
                        label: format!(": go to line {n}"),
                        action: PAction::GotoLine(n),
                    });
                }
            }
            PMode::Symbols => {
                if let Some(tab) = self.editor.active_tab() {
                    if !tab.is_image() {
                        let text = tab.text();
                        for sym in palette::symbols(&text) {
                            if query.is_empty() || palette::fuzzy_match(&sym.name, &query) {
                                entries.push(Entry {
                                    label: format!("@ {}", sym.text),
                                    action: PAction::GotoLine(sym.line),
                                });
                            }
                        }
                    }
                }
            }
        }
        if let Some(p) = self.palette.as_mut() {
            if p.selected >= entries.len() {
                p.selected = entries.len().saturating_sub(1);
            }
            p.entries = entries;
        }
    }

    fn palette_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                // Cancel: revert any go-to-line preview to where the cursor was.
                self.restore_palette_origin();
                self.palette = None;
                self.palette_origin = None;
            }
            KeyCode::Up => {
                if let Some(p) = self.palette.as_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.palette.as_mut() {
                    p.down();
                }
            }
            KeyCode::Tab => {
                if let Some(p) = self.palette.as_mut() {
                    p.selected = 0;
                }
                self.accept_palette();
            }
            KeyCode::Enter => self.accept_palette(),
            KeyCode::Backspace => {
                if let Some(p) = self.palette.as_mut() {
                    p.backspace();
                }
                self.recompute_palette();
                self.preview_goto_line();
            }
            KeyCode::Char(c) => {
                if let Some(p) = self.palette.as_mut() {
                    p.insert(c);
                }
                self.recompute_palette();
                self.preview_goto_line();
            }
            _ => {}
        }
    }

    fn accept_palette(&mut self) {
        let Some(p) = self.palette.as_ref() else {
            return;
        };
        let Some(entry) = p.selected_entry().cloned() else {
            return;
        };
        self.palette = None;
        // Undo any live go-to-line preview so the action below jumps from (and
        // records in the history) the cursor's real pre-palette position.
        self.restore_palette_origin();
        self.palette_origin = None;
        match entry.action {
            PAction::OpenFile(path, target) => {
                self.with_jump(|s| {
                    s.open_path(&path, false);
                    if let Some((line, col)) = target {
                        let area = s.editor_view();
                        s.editor.goto(line, Some(col), area);
                    }
                    s.focus = Focus::Editor;
                });
            }
            PAction::RunCommand(action) => self.run_action(&action),
            PAction::SwitchBuffer(i) => {
                self.with_jump(|s| {
                    if i < s.editor.tabs.len() {
                        s.editor.active = i;
                    }
                    s.focus = Focus::Editor;
                });
            }
            PAction::GotoLine(n) => {
                self.with_jump(|s| {
                    let area = s.editor_view();
                    s.editor.goto(n, None, area);
                    s.focus = Focus::Editor;
                });
            }
        }
    }

    // ----- search / replace ----------------------------------------------

    fn start_search(&mut self, replacing: bool) {
        self.search = Some(SearchBar::new(replacing));
    }

    /// Whether typing in the search box should live-preview the next match.
    /// Interactive (query-replace) mode keeps the cursor put until it begins.
    fn search_should_preview(&self) -> bool {
        self.search
            .as_ref()
            .is_some_and(|s| !s.interactive && s.field == Field::Query)
    }

    /// Recompute and apply search-highlight marks for the active buffer, then
    /// move the cursor to the next/previous match.
    fn find_step(&mut self, forward: bool) {
        // While the find box is open, use its (possibly empty) query; once closed,
        // repeat the last completed search so Find Next / Previous keep working.
        let pat = if self.search.is_some() {
            self.search.as_ref().and_then(super::search::SearchBar::pattern)
        } else {
            self.last_search.clone()
        };
        let Some(pat) = pat else {
            if self.search.is_some() {
                self.clear_marks();
            } else {
                // Nothing remembered yet: fall back to the selection / word.
                self.find_selection(forward);
            }
            return;
        };
        let re = match Regex::new(&pat) {
            Ok(r) => r,
            Err(e) => {
                if let Some(s) = self.search.as_mut() {
                    s.status = t!("msg.bad_regex", error = e).to_string();
                }
                return;
            }
        };
        self.last_search = Some(pat);
        let count = self.find_with(&re, forward);
        let msg = if count == 0 {
            t!("status.no_matches").to_string()
        } else {
            t!("status.matches", count = count).to_string()
        };
        if let Some(s) = self.search.as_mut() {
            s.status = msg;
        } else {
            self.status = msg;
        }
    }

    /// Find next/previous occurrence of the current selection (or, with no
    /// selection, the word under the cursor) — independent of the search bar.
    fn find_selection(&mut self, forward: bool) {
        let selected = self
            .editor
            .active_tab_mut()
            .and_then(|t| t.editor.get_selection_text());
        let query = selected
            .filter(|s| !s.trim().is_empty())
            .or_else(|| self.symbol_under_cursor());
        let Some(query) = query else {
            self.status = t!("status.no_selection").to_string();
            return;
        };
        let pat = regex::escape(&query);
        let Ok(re) = Regex::new(&pat) else {
            return;
        };
        self.last_search = Some(pat);
        let count = self.find_with(&re, forward);
        self.status = if count == 0 {
            t!("status.no_matches").into()
        } else {
            t!("status.matches", count = count).to_string()
        };
    }

    /// Mark every match of `re` in the active buffer and move the cursor to the
    /// next/previous one (wrapping at the ends). Returns the match count; zero
    /// matches clears the marks.
    fn find_with(&mut self, re: &Regex, forward: bool) -> usize {
        let area = self.editor_view();
        let Some(t) = self.editor.active_tab_mut() else {
            return 0;
        };
        let content = t.text();
        let matches = vix_find_panel::matches(&content, re);
        if matches.is_empty() {
            t.editor.remove_marks();
            return 0;
        }
        let marks: Vec<(usize, usize, &str)> =
            matches.iter().map(|(s, e)| (*s, *e, SEARCH_MARK)).collect();
        t.editor.set_marks(marks);

        // Pick the next/previous match relative to the cursor, wrapping around
        // the ends (first match after the last, last match before the first).
        let cur = t.editor.get_cursor();
        let target = if forward {
            matches
                .iter()
                .find(|(s, _)| *s > cur)
                .copied()
                .unwrap_or(matches[0]) // past the last match: wrap to the first
        } else {
            matches
                .iter()
                .rev()
                .find(|(s, _)| *s < cur)
                .copied()
                .unwrap_or(*matches.last().unwrap()) // before the first: wrap to the last
        };
        t.editor.set_cursor(target.0);
        t.editor.set_selection(Some(Selection::new(target.0, target.1)));
        t.editor.focus(&area);
        matches.len()
    }

    fn clear_marks(&mut self) {
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.remove_marks();
        }
    }

    fn end_search(&mut self) {
        self.clear_marks();
        self.search = None;
    }

    fn search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.end_search(),
            KeyCode::F(3) if Self::shift(&key) => self.find_step(false),
            KeyCode::F(3) => self.find_step(true),
            KeyCode::Tab => {
                if let Some(s) = self.search.as_mut() {
                    s.toggle_field();
                }
            }
            KeyCode::Enter => {
                let interactive = self.search.as_ref().is_some_and(|s| s.interactive);
                let replacing = self.search.as_ref().is_some_and(|s| s.replacing);
                let on_replace_field =
                    self.search.as_ref().map(|s| s.field) == Some(Field::Replace);
                if interactive {
                    self.begin_query_replace();
                } else if replacing && (Self::alt(&key) || on_replace_field) {
                    self.replace_all();
                } else {
                    self.find_step(true);
                }
            }
            KeyCode::Backspace => {
                if let Some(s) = self.search.as_mut() {
                    s.active_field_mut().pop();
                }
                if self.search_should_preview() {
                    self.find_step(true);
                }
            }
            KeyCode::Char(c) if Self::alt(&key) => {
                if let Some(s) = self.search.as_mut() {
                    match c.to_ascii_lowercase() {
                        'c' => s.case_sensitive = !s.case_sensitive,
                        'w' => s.whole_word = !s.whole_word,
                        'r' => s.regex = !s.regex,
                        _ => {}
                    }
                }
                // Toggles never move the cursor while in interactive mode.
                if self.search.as_ref().is_some_and(|s| !s.interactive) {
                    self.find_step(true);
                }
            }
            KeyCode::Char(c) => {
                if let Some(s) = self.search.as_mut() {
                    s.active_field_mut().push(c);
                }
                if self.search_should_preview() {
                    self.find_step(true);
                }
            }
            _ => {}
        }
    }

    /// A left click inside the find / replace box focuses the field whose row was
    /// clicked (the second row is the Replace field in replace mode). Clicks
    /// elsewhere are ignored so the box stays open.
    fn search_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.search;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let rel = mouse.row - r.y;
        if let Some(s) = self.search.as_mut() {
            if rel == 0 {
                s.field = Field::Query;
            } else if s.replacing && rel == 1 {
                s.field = Field::Replace;
            }
        }
    }

    /// `strftime` pattern for inserting a clicked calendar day, by active locale.
    fn locale_date_pattern() -> &'static str {
        match &*rust_i18n::locale() {
            "en" => "%m/%d/%Y",
            "de" => "%d.%m.%Y",
            "fr" | "es" | "cy" => "%d/%m/%Y",
            _ => "%Y-%m-%d",
        }
    }

    /// A left click in the clock box inserts the clicked time row into the active
    /// editor; the box stays open so several values can be picked. A click
    /// outside the box closes it.
    fn clock_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.clock;
        if !rect_contains(r, mouse.column, mouse.row) {
            self.show_clock = false;
            return;
        }
        let row = (mouse.row - r.y) as usize;
        if row < self.clock.row_count() {
            self.clock.select(row);
            let now = crate::clock::now_local();
            if let Some(text) = self.clock.selected_value(&now) {
                let area = self.editor_view();
                self.editor.insert_str(&text, area);
            }
        }
    }

    /// A left click in the calendar box inserts text into the active editor: one
    /// of the three date-time info lines, or a clicked day formatted per locale.
    /// A click outside the box closes it. The box stays open after an insert so
    /// several values can be picked.
    fn calendar_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.calendar;
        if !rect_contains(r, mouse.column, mouse.row) {
            self.show_calendar = false;
            return;
        }
        let rel_y = mouse.row - r.y;
        let rel_x = mouse.column - r.x;
        // The calendar is the month header on top and the day grid beneath. Row 0
        // is the month header carrying the `◀`/`▶` nav arrows (`◀` at column 0,
        // `▶` at column 20).
        if rel_y == 0 {
            if rel_x == 0 {
                self.calendar.prev_month();
            } else if rel_x == 20 {
                self.calendar.next_month();
            }
            return;
        }
        // The weekday header is row 1 and the week rows start at row 2, each day
        // cell three columns wide. Clicking a day inserts it.
        let text = if rel_y >= 2 {
            let week = (rel_y - 2) as usize;
            let col = (rel_x / 3) as usize;
            self.calendar
                .grid()
                .weeks
                .get(week)
                .and_then(|w| w.get(col).copied())
                .flatten()
                .and_then(|d| self.calendar.format_day(d, Self::locale_date_pattern()))
        } else {
            None
        };
        if let Some(text) = text {
            let area = self.editor_view();
            self.editor.insert_str(&text, area);
        }
    }

    fn replace_all(&mut self) {
        let Some(sb) = self.search.as_ref() else {
            return;
        };
        let Some(pat) = sb.pattern() else {
            return;
        };
        let re = match Regex::new(&pat) {
            Ok(r) => r,
            Err(e) => {
                if let Some(s) = self.search.as_mut() {
                    s.status = t!("msg.bad_regex", error = e).to_string();
                }
                return;
            }
        };
        let use_regex = sb.regex;
        let replacement = sb.replace.clone();
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let text = tab.text();
        let (new_text, count) = vix_find_panel::replace_all(&text, &re, use_regex, &replacement);
        tab.editor.set_content(&new_text);
        tab.editor.remove_marks();
        tab.dirty = true;
        tab.preview = false;
        if let Some(s) = self.search.as_mut() {
            s.status = t!("status.replaced", count = count).to_string();
        }
    }

    // ----- workspace-wide search / replace ---------------------------------

    /// Go to the definition of the symbol under the cursor. When a language
    /// server handles the active file, ask it (`textDocument/definition`, the
    /// result arrives asynchronously and jumps via [`App::poll_lsp`]). Otherwise
    /// fall back to the heuristic, keyword-prefixed cross-workspace search below.
    fn goto_definition(&mut self) {
        if let Some(path) = self.active_path() {
            if self.lsp.handles(&path) {
                let (line, character) = self.cursor_lsp_position(&path);
                self.lsp.request_definition(&path, line, character);
                return;
            }
        }
        let Some(symbol) = self.symbol_under_cursor() else {
            self.messages.warn(t!("msg.no_symbol"));
            return;
        };
        self.build_file_index();
        let hits = self.find_definitions(&symbol);
        match hits.len() {
            0 => self
                .messages
                .warn(t!("msg.no_definition", symbol = symbol).to_string()),
            1 => {
                let (path, line, col) = (hits[0].path.clone(), hits[0].line, hits[0].col);
                self.with_jump(|s| {
                    s.open_path(&path, false);
                    let area = s.editor_view();
                    s.editor.goto(line, Some(col), area);
                    s.focus = Focus::Editor;
                });
                self.status = t!("status.definition_of", symbol = symbol).to_string();
            }
            n => {
                let mut ps = WorkspaceSearch::new(false);
                ps.query.clone_from(&symbol);
                ps.static_results = true;
                ps.hits = hits;
                ps.status = t!("status.definitions_n", n = n, symbol = symbol).to_string();
                self.workspace_search = Some(ps);
            }
        }
    }

    /// The identifier (alphanumeric/underscore word) under the cursor.
    fn symbol_under_cursor(&self) -> Option<String> {
        let tab = self.editor.active_tab()?;
        if tab.is_image() {
            return None;
        }
        let text = tab.text();
        let cursor = tab.editor.get_cursor();
        let chars: Vec<char> = text.chars().collect();
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        let mut start = cursor.min(chars.len());
        while start > 0 && is_word(chars[start - 1]) {
            start -= 1;
        }
        let mut end = cursor.min(chars.len());
        while end < chars.len() && is_word(chars[end]) {
            end += 1;
        }
        if start == end {
            return None;
        }
        let sym: String = chars[start..end].iter().collect();
        let ok = sym.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_');
        ok.then_some(sym)
    }

    fn find_definitions(&self, symbol: &str) -> Vec<Hit> {
        let esc = regex::escape(symbol);
        // Definition-introducing keywords across common languages.
        let kw = "fn|func|function|def|class|struct|enum|trait|interface|type|\
                  const|let|var|val|static|mod|namespace|package|macro_rules!";
        let pat = format!(r"(?:\b(?:{kw})\s+{esc}\b|#define\s+{esc}\b)");
        let Ok(re) = Regex::new(&pat) else {
            return Vec::new();
        };
        let mut hits = Vec::new();
        for path in &self.file_index {
            let Some(content) = self.current_text(path) else {
                continue;
            };
            let rel = path
                .strip_prefix(&self.root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned();
            for (i, line) in content.lines().enumerate() {
                if re.is_match(line) {
                    let byte = line.find(symbol).unwrap_or(0);
                    let col = line[..byte].chars().count() + 1;
                    let clipped: String = line.trim_start().chars().take(120).collect();
                    hits.push(Hit {
                        path: path.clone(),
                        line: i + 1,
                        col,
                        display: format!("{rel}:{}: {clipped}", i + 1),
                    });
                    if hits.len() >= 200 {
                        return hits;
                    }
                }
            }
        }
        hits
    }

    fn open_workspace_search(&mut self, replacing: bool) {
        self.build_file_index();
        self.workspace_search = Some(WorkspaceSearch::new(replacing));
        self.run_workspace_search();
    }

    /// The current text for a path: the open buffer if one points at it (so
    /// unsaved edits are searched), otherwise the file on disk.
    fn current_text(&self, path: &Path) -> Option<String> {
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(tab) = self
            .editor
            .tabs
            .iter()
            .find(|t| t.path.as_deref() == Some(canon.as_path()))
        {
            return Some(tab.text());
        }
        let meta = std::fs::metadata(path).ok()?;
        if meta.len() > 2_000_000 {
            return None; // skip very large files
        }
        std::fs::read_to_string(path).ok()
    }

    fn run_workspace_search(&mut self) {
        // Static result lists (e.g. go-to-definition) are not re-searched.
        if self.workspace_search.as_ref().is_some_and(|p| p.static_results) {
            return;
        }
        let Some(ps) = self.workspace_search.as_ref() else {
            return;
        };
        let Some(pat) = ps.pattern() else {
            if let Some(p) = self.workspace_search.as_mut() {
                p.hits.clear();
                p.selected = 0;
                p.status = t!("status.workspace_search_prompt").into();
            }
            return;
        };
        let re = match Regex::new(&pat) {
            Ok(r) => r,
            Err(e) => {
                if let Some(p) = self.workspace_search.as_mut() {
                    p.status = t!("msg.bad_regex", error = e).to_string();
                }
                return;
            }
        };

        let filter = ps.path_filter();
        let mut hits: Vec<Hit> = Vec::new();
        let mut files = 0usize;
        'outer: for path in &self.file_index {
            let rel = path
                .strip_prefix(&self.root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned();
            if !filter.allows(&rel.replace('\\', "/")) {
                continue;
            }
            let Some(content) = self.current_text(path) else {
                continue;
            };
            let mut file_had_hit = false;
            for (i, line) in content.lines().enumerate() {
                if let Some(m) = re.find(line) {
                    file_had_hit = true;
                    let clipped: String = line.trim_start().chars().take(120).collect();
                    hits.push(Hit {
                        path: path.clone(),
                        line: i + 1,
                        col: m.start() + 1,
                        display: format!("{rel}:{}: {clipped}", i + 1),
                    });
                    if hits.len() >= 5000 {
                        break 'outer;
                    }
                }
            }
            if file_had_hit {
                files += 1;
            }
        }

        if let Some(p) = self.workspace_search.as_mut() {
            p.status = if hits.is_empty() {
                t!("status.no_matches_cap").into()
            } else {
                t!("status.matches_in_files", count = hits.len(), files = files).to_string()
            };
            if p.selected >= hits.len() {
                p.selected = hits.len().saturating_sub(1);
            }
            p.hits = hits;
        }
    }

    fn workspace_replace_all(&mut self) {
        let Some(ps) = self.workspace_search.as_ref() else {
            return;
        };
        if !ps.replacing {
            return;
        }
        let Some(pat) = ps.pattern() else {
            return;
        };
        let re = match Regex::new(&pat) {
            Ok(r) => r,
            Err(e) => {
                if let Some(p) = self.workspace_search.as_mut() {
                    p.status = t!("msg.bad_regex", error = e).to_string();
                }
                return;
            }
        };
        let use_regex = ps.regex;
        let replacement = ps.replace.clone();

        // Unique set of files that currently have hits.
        let mut paths: Vec<PathBuf> = ps.hits.iter().map(|h| h.path.clone()).collect();
        paths.sort();
        paths.dedup();

        let mut replaced = 0usize;
        let mut files = 0usize;
        for path in &paths {
            let Some(content) = self.current_text(path) else {
                continue;
            };
            let (new, count) = vix_find_panel::replace_all(&content, &re, use_regex, &replacement);
            if count == 0 {
                continue;
            }
            if new == content {
                continue;
            }
            if let Err(e) = std::fs::write(path, &new) {
                self.messages.error(t!("msg.write_failed", path = path.display(), error = e).to_string());
                continue;
            }
            // Keep any open buffer in sync (and clean, since we just saved).
            let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
            for tab in &mut self.editor.tabs {
                if tab.path.as_deref() == Some(canon.as_path()) {
                    tab.editor.set_content(&new);
                    tab.dirty = false;
                }
            }
            replaced += count;
            files += 1;
        }

        self.run_workspace_search();
        if let Some(p) = self.workspace_search.as_mut() {
            p.status = t!("status.replaced_in_files", replaced = replaced, files = files).to_string();
        }
    }

    fn ps_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.workspace_search = None,
            KeyCode::Up => {
                if let Some(p) = self.workspace_search.as_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.workspace_search.as_mut() {
                    p.down();
                }
            }
            KeyCode::Tab => {
                if let Some(p) = self.workspace_search.as_mut() {
                    p.toggle_field();
                }
            }
            KeyCode::Enter => {
                let replacing = self.workspace_search.as_ref().is_some_and(|p| p.replacing);
                let on_replace = self.workspace_search.as_ref().map(|p| p.field) == Some(Field::Replace);
                if replacing && (Self::alt(&key) || on_replace) {
                    self.workspace_replace_all();
                } else {
                    self.open_selected_hit();
                }
            }
            KeyCode::Char(c) if Self::alt(&key) => {
                if let Some(p) = self.workspace_search.as_mut() {
                    match c.to_ascii_lowercase() {
                        'c' => p.case_sensitive = !p.case_sensitive,
                        'r' => p.regex = !p.regex,
                        _ => {}
                    }
                }
                self.run_workspace_search();
            }
            KeyCode::Backspace => {
                // Editing any field except Replace (query or the path filters)
                // changes the result set, so re-run the search.
                let affects = self.workspace_search.as_ref().map(|p| p.field) != Some(Field::Replace);
                if let Some(p) = self.workspace_search.as_mut() {
                    p.active_field_mut().pop();
                }
                if affects {
                    self.run_workspace_search();
                }
            }
            KeyCode::Char(c) => {
                let affects = self.workspace_search.as_ref().map(|p| p.field) != Some(Field::Replace);
                if let Some(p) = self.workspace_search.as_mut() {
                    p.active_field_mut().push(c);
                }
                if affects {
                    self.run_workspace_search();
                }
            }
            _ => {}
        }
    }

    fn open_selected_hit(&mut self) {
        let target = self
            .workspace_search
            .as_ref()
            .and_then(|p| p.selected_hit())
            .map(|h| (h.path.clone(), h.line, h.col));
        if let Some((path, line, col)) = target {
            self.workspace_search = None;
            self.with_jump(|s| {
                s.open_path(&path, false);
                let area = s.editor_view();
                s.editor.goto(line, Some(col), area);
                s.focus = Focus::Editor;
            });
        }
    }

    // ----- interactive query-replace -------------------------------------

    fn begin_query_replace(&mut self) {
        let Some(sb) = self.search.as_ref() else {
            return;
        };
        let Some(pat) = sb.pattern() else {
            if let Some(s) = self.search.as_mut() {
                s.status = t!("status.type_to_find").into();
            }
            return;
        };
        let re = match Regex::new(&pat) {
            Ok(r) => r,
            Err(e) => {
                if let Some(s) = self.search.as_mut() {
                    s.status = t!("msg.bad_regex", error = e).to_string();
                }
                return;
            }
        };
        let template = if sb.regex {
            vix_find_panel::unescape(&sb.replace)
        } else {
            sb.replace.clone()
        };
        let regex = sb.regex;
        let label = sb.query.clone();
        self.search = None;

        let area = self.editor_view();
        let found = {
            let Some(t) = self.editor.active_tab_mut() else {
                return;
            };
            let from = t.editor.get_cursor();
            match next_match_from(t, &re, from) {
                Some((cs, ce)) => {
                    highlight_match(t, cs, ce, area);
                    Some((cs, ce))
                }
                None => None,
            }
        };
        match found {
            Some(current) => {
                self.query_replace = Some(QueryReplace {
                    re,
                    template,
                    regex,
                    current,
                    replaced: 0,
                    label,
                });
                self.status = t!("status.qr_keys").into();
            }
            None => self.status = t!("status.qr_no_matches").into(),
        }
    }

    fn qr_key(&mut self, key: KeyEvent) {
        let decision = match key.code {
            KeyCode::Char('y' | 'Y' | ' ') => Decision::Replace,
            KeyCode::Char('n' | 'N') | KeyCode::Delete => Decision::Skip,
            KeyCode::Char('!') => Decision::ReplaceRest,
            KeyCode::Char('q' | 'Q') | KeyCode::Esc | KeyCode::Enter => {
                Decision::Quit
            }
            _ => return,
        };
        self.qr_apply(decision);
    }

    fn qr_apply(&mut self, decision: Decision) {
        let area = self.editor_view();
        let Some(qr) = self.query_replace.as_ref() else {
            return;
        };
        let re = qr.re.clone();
        let template = qr.template.clone();
        let regex = qr.regex;
        let (cs, ce) = qr.current;
        let mut replaced = qr.replaced;

        let next = {
            let Some(t) = self.editor.active_tab_mut() else {
                return;
            };
            let result = match decision {
                Decision::Quit => {
                    t.editor.remove_marks();
                    t.editor.set_selection(None);
                    None
                }
                Decision::Skip => {
                    t.editor.remove_marks();
                    next_match_from(t, &re, ce)
                }
                Decision::Replace => {
                    let resume = do_replace(t, &re, regex, &template, (cs, ce));
                    replaced += 1;
                    t.dirty = true;
                    t.preview = false;
                    t.editor.remove_marks();
                    next_match_from(t, &re, resume)
                }
                Decision::ReplaceRest => {
                    let mut cur = (cs, ce);
                    let mut guard = 0usize;
                    loop {
                        let resume = do_replace(t, &re, regex, &template, cur);
                        replaced += 1;
                        guard += 1;
                        if guard > 1_000_000 {
                            break;
                        }
                        match next_match_from(t, &re, resume) {
                            Some(m) if m.0 >= resume => cur = m,
                            _ => break,
                        }
                    }
                    t.dirty = true;
                    t.preview = false;
                    t.editor.remove_marks();
                    t.editor.set_selection(None);
                    None
                }
            };
            if let Some((ns, ne)) = result {
                highlight_match(t, ns, ne, area);
            }
            result
        };

        if let Some(current) = next {
            if let Some(q) = self.query_replace.as_mut() {
                q.current = current;
                q.replaced = replaced;
            }
        } else {
            self.query_replace = None;
            self.status = t!("status.qr_replaced", count = replaced).to_string();
        }
    }

    // ----- prompt (Open / Save As) ---------------------------------------

    fn prompt_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.prompt = None,
            KeyCode::Enter => self.accept_prompt(),
            KeyCode::Backspace => {
                if let Some(p) = self.prompt.as_mut() {
                    p.input.pop();
                }
            }
            // Alt+C / Alt+R toggle case / regex for the workspace→dock search.
            KeyCode::Char(c) if Self::alt(&key) => {
                if let Some(p) = self.prompt.as_mut() {
                    if matches!(p.kind, PromptKind::SearchToDock) {
                        match c.to_ascii_lowercase() {
                            'c' => p.case_sensitive = !p.case_sensitive,
                            'r' => p.regex = !p.regex,
                            _ => {}
                        }
                    }
                }
            }
            KeyCode::Char(c) => {
                if let Some(p) = self.prompt.as_mut() {
                    p.input.push(c);
                }
            }
            _ => {}
        }
    }

    fn accept_prompt(&mut self) {
        let Some(prompt) = self.prompt.take() else {
            return;
        };
        match prompt.kind {
            PromptKind::Open => {
                let (path, target) = palette::parse_path_target(prompt.input.trim());
                if path.is_empty() {
                    return;
                }
                let path = self.resolve(&path);
                self.with_jump(|s| {
                    s.open_path(&path, false);
                    if let Some((line, col)) = target {
                        let area = s.editor_view();
                        s.editor.goto(line, Some(col), area);
                    }
                    s.focus = Focus::Editor;
                });
            }
            PromptKind::SaveAs => {
                let raw = prompt.input.trim();
                if raw.is_empty() {
                    return;
                }
                let path = self.resolve(raw);
                let opts = self.save_options();
                match self.editor.save_active_as(path, opts) {
                    Ok(p) => {
                        self.status = t!("status.saved", path = p.display()).to_string();
                        self.explorer.rebuild();
                    }
                    Err(e) => self.messages.error(t!("msg.save_failed", error = e).to_string()),
                }
            }
            PromptKind::Rename => self.rename_file(&prompt.input),
            PromptKind::RunCommand => self.run_command(&prompt.input),
            PromptKind::SearchToDock => {
                self.search_workspace_to_dock(&prompt.input, prompt.case_sensitive, prompt.regex);
            }
            PromptKind::GitCommit => self.git_commit(&prompt.input),
            PromptKind::GitNewBranch => self.git_create_branch(&prompt.input),
            PromptKind::GitClone => self.git_clone(&prompt.input),
            PromptKind::ExplorerInclude => {
                let exclude = self.explorer.exclude_filter.clone();
                self.explorer.set_filter(prompt.input.trim(), &exclude);
            }
            PromptKind::ExplorerExclude => {
                let include = self.explorer.include_filter.clone();
                self.explorer.set_filter(&include, prompt.input.trim());
            }
        }
    }

    /// Search every workspace file for `query` and list the hits in the bottom dock
    /// as `relpath:line:col: text` lines, which are click-to-jump. `regex` treats
    /// the query as a regular expression (else literal); `case_sensitive` matches
    /// case exactly. Shows the dock.
    fn search_workspace_to_dock(&mut self, query: &str, case_sensitive: bool, regex: bool) {
        let query = query.trim();
        if query.is_empty() {
            return;
        }
        self.build_file_index();
        let core = if regex {
            query.to_string()
        } else {
            regex::escape(query)
        };
        let pat = if case_sensitive { core } else { format!("(?i){core}") };
        let re = match Regex::new(&pat) {
            Ok(r) => r,
            Err(e) => {
                self.show_bottom_dock = true;
                self.settings.show_bottom_dock = true;
                self.bottom_dock.push(format!("[bad regex: {e}]"));
                self.status = t!("msg.bad_regex", error = e).to_string();
                return;
            }
        };
        self.show_bottom_dock = true;
        self.settings.show_bottom_dock = true;
        self.bottom_dock.push(format!("$ search \"{query}\""));
        let mut count = 0usize;
        let mut files = 0usize;
        for path in self.file_index.clone() {
            let Some(content) = self.current_text(&path) else {
                continue;
            };
            let rel = path
                .strip_prefix(&self.root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            let mut had_hit = false;
            for (i, line) in content.lines().enumerate() {
                if let Some(m) = re.find(line) {
                    had_hit = true;
                    count += 1;
                    let clipped: String = line.trim_start().chars().take(120).collect();
                    self.bottom_dock
                        .push(format!("{rel}:{}:{}: {clipped}", i + 1, m.start() + 1));
                    if count >= 5000 {
                        break;
                    }
                }
            }
            if had_hit {
                files += 1;
            }
            if count >= 5000 {
                break;
            }
        }
        self.bottom_dock
            .push(format!("[{count} matches in {files} files]"));
        self.status = t!("status.matches_in_files", count = count, files = files).to_string();
    }

    /// Run a shell command in the workspace root, streaming its output (stdout and
    /// stderr merged) into the bottom dock, which is shown. The command runs in a
    /// background thread; [`App::poll_command`] drains its output each frame and
    /// [`App::cancel_command`] kills it.
    /// Open the rename prompt for the active file, seeded with its current name.
    fn open_rename_prompt(&mut self) {
        let Some(cur) = self.editor.active_tab().and_then(|t| t.path.clone()) else {
            self.status = t!("status.rename_no_file").to_string();
            return;
        };
        let name = cur.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        self.prompt =
            Some(Prompt::new(PromptKind::Rename, t!("prompt.rename").to_string()).with_input(name));
    }

    /// Rename the active file on disk to `input`. A bare name stays in the same
    /// directory; a value containing `/` is resolved against the workspace root.
    /// Updates the active tab's path and refreshes the explorer and git state.
    fn rename_file(&mut self, input: &str) {
        let raw = input.trim();
        if raw.is_empty() {
            return;
        }
        let Some(cur) = self.editor.active_tab().and_then(|t| t.path.clone()) else {
            self.status = t!("status.rename_no_file").to_string();
            return;
        };
        let new_path = if raw.contains('/') {
            self.resolve(raw)
        } else {
            cur.parent().map_or_else(|| PathBuf::from(raw), |d| d.join(raw))
        };
        if new_path == cur {
            return;
        }
        if new_path.exists() {
            self.status = t!("status.rename_exists", name = new_path.display()).to_string();
            return;
        }
        match std::fs::rename(&cur, &new_path) {
            Ok(()) => {
                if let Some(t) = self.editor.active_tab_mut() {
                    t.path = Some(new_path.clone());
                }
                self.status = t!("status.renamed", path = new_path.display()).to_string();
                self.explorer.rebuild();
                self.refresh_git();
            }
            Err(e) => self.messages.error(t!("msg.rename_failed", error = e).to_string()),
        }
    }

    fn run_command(&mut self, cmd: &str) {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            return;
        }
        if self.running_command.is_some() {
            self.status = t!("status.command_busy").to_string();
            return;
        }
        self.show_bottom_dock = true;
        self.settings.show_bottom_dock = true;
        self.bottom_dock.push(format!("$ {cmd}"));

        // Merge the whole command's stderr into stdout so one pipe carries both.
        let mut child = match std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("{{ {cmd} ; }} 2>&1"))
            .current_dir(&self.root)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                self.bottom_dock.push(format!("[error: {e}]"));
                self.messages.error(t!("msg.command_failed", error = e).to_string());
                return;
            }
        };
        let stdout = child.stdout.take().expect("piped stdout");
        let child = std::sync::Arc::new(std::sync::Mutex::new(child));
        let (tx, rx) = std::sync::mpsc::channel();
        let reader_child = child.clone();
        std::thread::spawn(move || {
            use std::io::BufRead;
            let reader = std::io::BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if tx.send(CmdMsg::Line(line)).is_err() {
                    return; // the app dropped the receiver
                }
            }
            // Pipe closed: the process is finishing. Reap it for the exit code.
            let code = reader_child
                .lock()
                .expect("command lock")
                .wait()
                .ok()
                .and_then(|s| s.code());
            let _ = tx.send(CmdMsg::Done(code));
        });
        self.running_command = Some(RunningCommand { rx, child });
    }

    /// Drain any streamed command output into the bottom dock. Called once per
    /// event-loop iteration; cheap when no command is running.
    pub fn poll_command(&mut self) {
        let msgs: Vec<CmdMsg> = {
            let Some(rc) = self.running_command.as_ref() else {
                return;
            };
            let mut v = Vec::new();
            while let Ok(m) = rc.rx.try_recv() {
                v.push(m);
            }
            v
        };
        let mut done = false;
        for msg in msgs {
            match msg {
                CmdMsg::Line(l) => self.bottom_dock.push(l),
                CmdMsg::Done(code) => {
                    let code = code.unwrap_or(-1);
                    self.bottom_dock.push(format!("[exit {code}]"));
                    self.status = t!("status.command_done", code = code).to_string();
                    done = true;
                }
            }
        }
        if done {
            self.running_command = None;
            // A finished command may have changed the working tree or HEAD (e.g.
            // git push/pull/checkout); refresh the cached git state.
            self.refresh_git();
        }
    }

    /// Whether a command is currently running (the loop polls faster then).
    #[must_use]
    pub fn command_running(&self) -> bool {
        self.running_command.is_some()
    }

    /// Kill the running command, if any. Its `[exit N]` line still follows once
    /// the reader thread reaps it.
    fn cancel_command(&mut self) {
        if let Some(rc) = self.running_command.as_ref() {
            let _ = rc.child.lock().expect("command lock").kill();
            self.bottom_dock.push("[cancelled]".to_string());
        }
    }

    fn resolve(&self, input: &str) -> PathBuf {
        let p = PathBuf::from(input);
        if p.is_absolute() {
            p
        } else {
            self.root.join(p)
        }
    }

    /// Persist settings on exit; failures become a status message only.
    pub fn on_exit(&mut self) {
        self.lsp.shutdown();
        if let Err(e) = self.settings.save() {
            self.messages
                .push(Level::Warn, t!("msg.settings_save_failed", error = e).to_string());
        }
    }
}

fn rect_contains(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
}

/// The char offset within `code` of LSP position `(line, character)`, where
/// `character` is in the server's `enc` units. Out-of-range positions clamp.
fn lsp_pos_to_char(
    code: &vix_editor::code::Code,
    line: u32,
    character: u32,
    enc: vix_lsp::Encoding,
) -> usize {
    let line = line as usize;
    if line >= code.len_lines() {
        return code.len();
    }
    let line_start = code.line_to_char(line);
    let line_text = code.slice(line_start, line_start + code.line_len(line));
    line_start + vix_lsp::position::col_to_char(&line_text, character, enc)
}

/// The underline color for a diagnostic severity.
fn severity_color(sev: vix_lsp::Severity) -> ratatui::style::Color {
    use ratatui::style::Color;
    match sev {
        vix_lsp::Severity::Error => Color::Red,
        vix_lsp::Severity::Warning => Color::Yellow,
        vix_lsp::Severity::Information => Color::Cyan,
        vix_lsp::Severity::Hint => Color::Blue,
    }
}

/// The text of the HTML-palette row cell at `rel_col` (columns measured from the
/// row's left edge): the glyph, the entity name, or the code point. The column
/// bands track the row format rendered by `ui::draw_html_panel`
/// (`"  {glyph:2}  {name:26}  {code}"`).
fn html_cell_at(e: &vix_html_character_picker::Entity, rel_col: usize) -> String {
    if rel_col < 6 {
        e.glyph.to_string()
    } else if rel_col < 34 {
        e.name.to_string()
    } else {
        e.code.to_string()
    }
}

/// Decode an image file into a `DynamicImage` the picker can turn into a
/// terminal protocol.
fn decode_image(path: &Path) -> Result<image::DynamicImage, String> {
    image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())
}

/// First match whose (char) start is at or after `from_char`, as char offsets.
/// Editor adapter: the first match of `re` at/after char offset `from_char` in
/// `t`'s buffer. Pure matching lives in [`vix_find_panel::next_match`].
fn next_match_from(t: &Tab, re: &Regex, from_char: usize) -> Option<(usize, usize)> {
    vix_find_panel::next_match(&t.text(), re, from_char)
}

/// Editor adapter: replace the single match at char offset `current.0` in `t`,
/// returning the char offset just past the inserted text (where searching should
/// resume). Pure replacement lives in [`vix_find_panel::replace_one`].
fn do_replace(t: &mut Tab, re: &Regex, regex: bool, template: &str, current: (usize, usize)) -> usize {
    match vix_find_panel::replace_one(&t.text(), re, regex, template, current.0) {
        Some((new, resume)) => {
            t.editor.set_content(&new);
            resume
        }
        None => current.1,
    }
}

/// Recursively count regular files under `dir`, skipping `.git` and `target`
/// (the large generated trees). Best-effort: unreadable entries are skipped.
fn count_files(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut count = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name == ".git" || name == "target" {
            continue;
        }
        match entry.file_type() {
            Ok(ft) if ft.is_dir() => count += count_files(&entry.path()),
            Ok(ft) if ft.is_file() => count += 1,
            _ => {}
        }
    }
    count
}

/// Hex color for a diff-gutter line mark (green add, yellow modify, red delete).
fn gutter_hex(mark: vix_git::LineMark) -> &'static str {
    match mark {
        vix_git::LineMark::Added => "#3fb950",
        vix_git::LineMark::Modified => "#d29922",
        vix_git::LineMark::Deleted => "#f85149",
    }
}

/// Replace the char range `[span.0, span.1)` with `replacement` (whole-buffer
/// rebuild, mirroring [`do_replace`]), leaving the cursor after the new text.
fn replace_char_span(t: &mut Tab, span: (usize, usize), replacement: &str) {
    let content = t.text();
    let bs = t.editor.code_ref().char_to_byte(span.0);
    let be = t.editor.code_ref().char_to_byte(span.1);
    let mut new = String::with_capacity(content.len() + replacement.len());
    new.push_str(&content[..bs]);
    new.push_str(replacement);
    new.push_str(&content[be..]);
    t.editor.set_content(&new);
    t.editor.set_cursor(span.0 + replacement.chars().count());
}

/// Move the cursor to a match, select it, and add a search highlight mark.
fn highlight_match(t: &mut Tab, cs: usize, ce: usize, area: Rect) {
    t.editor.set_cursor(cs);
    t.editor.set_selection(Some(Selection::new(cs, ce)));
    t.editor.set_marks(vec![(cs, ce, SEARCH_MARK)]);
    t.editor.focus(&area);
}

/// Minimum editor width handed to the code editor. Its `focus()` computes
/// `visible_width - 10` and underflows (panics) when the area is narrower than
/// the line-number gutter plus that step, so we never pass it a smaller width.
const MIN_EDITOR_WIDTH: u16 = 20;

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect()
    }

    #[test]
    fn renders_workspace_search_panel_with_hits() {
        let dir = std::env::temp_dir().join(format!("vix-ps-unit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("note.txt"), "the needle is here\n").unwrap();

        let mut app = App::new(dir.clone(), Settings::default());
        app.run_action("search.workspace");
        for c in "needle".chars() {
            app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        assert_eq!(app.workspace_search.as_ref().unwrap().hits.len(), 1);

        let mut terminal = Terminal::new(TestBackend::new(100, 40)).unwrap();
        terminal.draw(|f| crate::ui::draw(&mut app, f)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("Search in Workspace"), "panel title rendered");
        assert!(text.contains("needle"), "the matching line is shown");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn renders_image_tab_without_panic() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        // Halfblocks renders into a plain cell buffer — no real terminal needed.
        let picker = Picker::halfblocks();
        let img = image::DynamicImage::new_rgb8(8, 8);
        let proto = picker.new_resize_protocol(img);
        app.editor.open_image(Path::new("/tmp/vix-test.png"), proto);
        assert!(app.editor.active_tab().unwrap().is_image());

        // A full draw of the image tab must not panic.
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| crate::ui::draw(&mut app, f)).unwrap();

        // Editing keys are ignored on an image tab.
        app.on_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(!app.editor.active_tab().unwrap().dirty);
    }
}
