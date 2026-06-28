//! Application state and event handling.

#![warn(clippy::pedantic)]

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use include_dir::{include_dir, Dir};
use ratatui::layout::Rect;
use crate::editor_core::actions::{
    Copy as CopyAction, Cut as CutAction, Paste as PasteAction, Redo as RedoAction,
    ToggleComment, Undo as UndoAction,
};
use crate::editor_core::selection::Selection;
use ratatui_image::picker::Picker;
use regex::Regex;

use crate::editor::{is_image_path, Editor, SplitDir, Tab, SEARCH_MARK};
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
        .filter_map(|f| f.contents_utf8().and_then(crate::theme_model::parse_theme))
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

/// Which view's horizontal scrollbar is being dragged.
#[derive(Clone, Copy, PartialEq, Eq)]
enum HBar {
    /// The center editor.
    Editor,
    /// The file explorer (left dock).
    Explorer,
    /// The message drawer (right dock).
    Messages,
    /// The bottom dock.
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
    /// Enter a description for the current branch (`git branch --edit-description`).
    GitEditDescription,
    /// Enter the name of a branch to delete (`git branch --delete`).
    GitDeleteBranch,
    /// Enter a regex to search the repository with `git grep`.
    GitGrep,
    /// Enter a query to search symbols across the project (LSP workspace/symbol).
    WorkspaceSymbol,
    /// Enter the new name for the symbol under the cursor (LSP rename).
    LspRename,
    /// Enter replacement text for the cursor's linked-editing ranges (LSP).
    LinkedEdit,
    /// Enter the file-explorer "include" path regex filter.
    ExplorerInclude,
    /// Enter the file-explorer "exclude" path regex filter.
    ExplorerExclude,
    /// Enter a file path to compare the active buffer against (diff overlay).
    CompareFile,
    /// Enter a name to save the just-recorded keyboard macro under.
    SaveMacro,
    /// Enter an expression to evaluate in the debugger (REPL).
    DebugRepl,
    /// Enter an expression to add as a debugger watch.
    DebugWatch,
    /// Enter an Org capture — a quick idea/task inserted as a `* TODO` headline.
    OrgCapture,
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

/// `PostgreSQL` `CREATE EXTENSION` snippet (Tools → Insert → SQL → Create Extension).
const SQL_CREATE_EXTENSION: &str = r#"-- Cryptographic functions: hashing, encryption, random bytes, UUIDs, password salts.
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- Tracks execution statistics for all queries to find slow/expensive ones.
CREATE EXTENSION IF NOT EXISTS "pg_stat_statements";

-- Large Object type plus a trigger to auto-clean orphaned large objects.
CREATE EXTENSION IF NOT EXISTS "lo";

-- Hierarchical "label tree" type for fast ancestor/descendant path queries.
CREATE EXTENSION IF NOT EXISTS "ltree";

-- Multidimensional cube type for N-dimensional points and boxes.
CREATE EXTENSION IF NOT EXISTS "cube";

-- Great-circle distance between lat/long points (depends on cube).
CREATE EXTENSION IF NOT EXISTS "earthdistance";

-- Strips accents from text for accent-insensitive search (Café -> Cafe).
CREATE EXTENSION IF NOT EXISTS "unaccent";

-- Trigram similarity and matching to speed up LIKE/ILIKE and fuzzy search.
CREATE EXTENSION IF NOT EXISTS "pg_trgm";

-- Fuzzy string matching by sound and edit distance (soundex, levenshtein).
CREATE EXTENSION IF NOT EXISTS "fuzzystrmatch";

-- Table-returning functions, notably crosstab() for pivot tables.
CREATE EXTENSION IF NOT EXISTS "tablefunc";

-- Trigger helper that auto-increments an integer field from a sequence.
CREATE EXTENSION IF NOT EXISTS "autoinc";

-- Trigger helper that stamps a column with the current DB username on write.
CREATE EXTENSION IF NOT EXISTS "insert_username";

-- Trigger helper that sets a timestamp column to now() on every UPDATE.
CREATE EXTENSION IF NOT EXISTS "moddatetime";

-- UUID generators (v1/v4/v5); v4 random UUIDs for primary keys.
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Triggered Change Notification: emits LISTEN/NOTIFY events on row changes.
CREATE EXTENSION IF NOT EXISTS "tcn";
"#;

/// `PostgreSQL` `CREATE TABLE` snippet (Tools → Insert → SQL → Create Table).
const SQL_CREATE_TABLE: &str = r"CREATE TABLE items (
    id INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    title VARCHAR(50) NOT NULL,
    subtitle VARCHAR(50) NOT NULL
);

CREATE TRIGGER updated_at
BEFORE UPDATE ON items
FOR EACH ROW
EXECUTE FUNCTION updated_at();

CREATE INDEX items_trigram
ON items
USING GIN ((
    title
    || ' ' ||
    subtitle
) gin_trgm_ops );
";

/// A command running in a background thread, streaming into the bottom dock.
struct RunningCommand {
    /// Receiver for the reader thread's output.
    rx: std::sync::mpsc::Receiver<CmdMsg>,
    /// The child process, shared so it can be reaped by the reader and killed by
    /// the app.
    child: std::sync::Arc<std::sync::Mutex<std::process::Child>>,
    /// The command line, for the completion notification.
    label: String,
}

/// Result of a background AI text transform whose output replaces editor text.
enum AiMsg {
    /// The CLI finished successfully, carrying its full stdout.
    Done(String),
    /// The CLI failed (non-zero exit, or it died before producing output).
    Failed,
}

/// Which part of a buffer an AI transform replaces.
#[derive(Clone, Copy)]
enum AiTarget {
    /// Replace the whole buffer.
    Whole,
    /// Replace this character range `[start, end)`.
    Range(usize, usize),
}

/// Where a finished AI task's captured output goes.
#[derive(Clone, Copy)]
enum AiDest {
    /// Replace text in tab `tab` (the whole buffer or a range).
    Replace { tab: usize, target: AiTarget },
    /// Open the result in a new editor tab.
    NewTab,
    /// Append the result to the AI chat panel transcript as an assistant turn.
    Panel,
    /// Open a reviewable accept/reject diff for tab `tab` over `target`, instead
    /// of replacing the text immediately (see [`crate::ai_diff`]).
    Diff { tab: usize, target: AiTarget },
}

/// An open AI diff review: the proposed change plus where it applies.
struct AiDiffState {
    /// The reviewable, hunk-toggleable diff.
    review: crate::ai_diff::Review,
    /// Tab the change applies to.
    tab: usize,
    /// Region of that tab the change replaces.
    target: AiTarget,
}

/// A background AI task whose captured output is applied when it finishes —
/// either replacing editor text (Annotate, Improve) or opening a new tab
/// (Summarize, Explain, Define).
struct AiReplace {
    /// Receiver for the captured result.
    rx: std::sync::mpsc::Receiver<AiMsg>,
    /// Where to put the result.
    dest: AiDest,
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

/// A previewed project-wide replace awaiting confirmation. Holds the computed
/// per-file results so applying writes exactly what was previewed.
pub struct ReplaceConfirm {
    /// Per-file `(path, new contents)` to write on confirm.
    pub plan: Vec<(PathBuf, String)>,
    /// Total matches that will be replaced.
    pub replaced: usize,
    /// Preview rows (`relpath (count)`) for the affected files.
    pub lines: Vec<String>,
    /// First visible preview row.
    pub scroll: usize,
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

/// The task chooser (Tools → Tasks…): the workspace's `tasks.toml` entries and
/// the highlighted row. Choosing one runs its command via the Run pipeline.
pub struct TaskChooser {
    /// Loaded tasks, in file order.
    pub tasks: Vec<crate::tasks::Task>,
    /// Index of the highlighted task.
    pub selected: usize,
}

/// A read-only diff overlay (Tools → Compare With File…): the active buffer
/// compared against another file, with scroll state.
pub struct DiffViewState {
    /// Overlay title (the compared file names).
    pub title: String,
    /// Rendered unified-diff lines.
    pub lines: Vec<crate::diff_view::Line>,
    /// First visible line.
    pub scroll: usize,
}

/// An active snippet expansion: the tabstop ranges still to visit (in
/// navigation order) and the index of the current one. Tab advances through them.
pub struct SnippetSession {
    /// Absolute `(start, end)` char ranges of the tabstops, in nav order.
    pub stops: Vec<(usize, usize)>,
    /// Index of the current tabstop within `stops`.
    pub index: usize,
}

/// The recent-projects chooser (File → Switch Project…): saved workspace roots
/// (most recent first, current excluded) and the highlighted row.
pub struct WorkspaceChooser {
    /// Absolute workspace root paths, most-recently-used first.
    pub roots: Vec<String>,
    /// Index of the highlighted root.
    pub selected: usize,
}

/// The saved-macro chooser (Edit → Play Saved Macro…): the persisted macros and
/// the highlighted row. Choosing one loads its keys and replays them.
pub struct MacroChooser {
    /// Saved macros, in file order.
    pub macros: Vec<crate::macros::Macro>,
    /// Index of the highlighted macro.
    pub selected: usize,
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

/// Nerd Font palette overlay state (Tools -> Nerd Font Palette), re-exported from
/// [`crate::nerd_font_picker`]. Arrow keys move within the glyph grid; Enter (or a
/// click) inserts the highlighted glyph into the active editor, Esc closes.
pub use crate::nerd_font_picker::Palette as NerdPalette;

/// ASCII panel overlay state (Tools -> ASCII), re-exported from
/// [`crate::ascii_character_picker`]. Arrow keys move within the table; Enter (or a click)
/// inserts the highlighted character into the active editor, Esc closes.
pub use crate::ascii_character_picker::Panel as AsciiPanel;

/// X11 color palette overlay state (Tools -> X11 Colors), re-exported from
/// [`crate::x11_color_picker`]. Arrow keys move within the table; Enter (or a click)
/// inserts the highlighted color's hex into the active editor, Esc closes.
pub use crate::x11_color_picker::Panel as X11Panel;

/// HTML character palette overlay state (Tools -> HTML Characters), re-exported
/// from [`crate::html_character_picker`]. Arrow keys move within the table; Enter
/// (or a click) inserts the highlighted entity reference into the editor, Esc
/// closes.
pub use crate::html_character_picker::Panel as HtmlPanel;

/// System Information panel overlay state (Tools -> System Information),
/// re-exported from [`crate::system_information_panel`]. Arrow keys move within the
/// table; Enter (or a click) inserts the highlighted value into the active
/// editor, Esc closes.
pub use crate::system_information_panel::Panel as SystemInfoPanel;

/// Workspace dashboard overlay state (Tools -> Workspace Dashboard), re-exported from
/// [`crate::workspace_dashboard_panel`]. Its metrics fill in asynchronously; Esc closes.
pub use crate::workspace_dashboard_panel::Dashboard;

/// First-run welcome overlay state, re-exported from [`crate::welcome_panel`].
/// Scrollable, informational; Esc closes.
pub use crate::welcome_panel::Panel as WelcomePanel;

/// Contact-browser overlay state (Tools -> Contacts), re-exported from
/// [`crate::contact_panel`]. Lists the vCard files in a directory; Enter/click opens
/// the highlighted contact's [`VcardPanel`].
pub use crate::contact_panel::Panel as ContactPanel;

/// Single-vCard view overlay state, re-exported from [`crate::vcard_panel`]. Shows
/// one contact's fields; Enter/click inserts a value, Esc returns to the browser.
pub use crate::vcard_panel::Panel as VcardPanel;

/// File Information overlay state (Tools -> File Information), re-exported from
/// [`crate::file_information_panel`]. Arrow keys move within the table; Enter (or a
/// click) inserts the highlighted value into the active editor, Esc closes.
pub use crate::file_information_panel::Panel as FileInfoPanel;

/// Text Information overlay state (Tools -> About -> Text), re-exported from
/// [`crate::text_information_panel`]. Shows character/word/line/sentence/paragraph
/// counts for the selection (or buffer); Enter/click inserts a value, Esc closes.
pub use crate::text_information_panel::Panel as TextInfoPanel;

/// Markdown preview overlay state (Tools → Markdown Preview), re-exported from
/// [`crate::markdown_preview`]. Read-only; arrows/PageUp/Down scroll, Esc closes.
pub use crate::markdown_preview::Panel as MarkdownPreview;

/// Code-outline overlay state (Ctrl+Shift+O), re-exported from
/// [`crate::outline_panel`]. Lists the active buffer's symbols; Enter/click jumps to
/// one, Esc closes.
pub use crate::outline_panel::Outline;

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
    /// Vim-style modal editing plus a `Space` leader for menu-like command
    /// sequences (e.g. `SPC f f` find file).
    Spacemacs,
    /// `IntelliJ` IDEA (macOS) shortcuts, with `Ctrl` standing in for `Cmd`.
    IntelliJMac,
    /// `IntelliJ` IDEA (Windows/Linux) shortcuts.
    IntelliJWin,
    /// Eclipse (Windows) shortcuts.
    Eclipse,
}

impl Keymap {
    /// Parse a persisted keymap id; anything unrecognized is [`Keymap::Apple`].
    fn from_id(id: &str) -> Self {
        match id {
            "vscode" => Keymap::Vscode,
            "emacs" => Keymap::Emacs,
            // `vi` is the current id; `vim` is accepted for older configs.
            "vi" | "vim" => Keymap::Vim,
            "spacemacs" => Keymap::Spacemacs,
            // `intellij-*` are the current ids; `jetbrains-*` load older configs.
            "intellij-mac" | "jetbrains-mac" => Keymap::IntelliJMac,
            "intellij-win" | "jetbrains-win" => Keymap::IntelliJWin,
            "eclipse" => Keymap::Eclipse,
            _ => Keymap::Apple,
        }
    }
}

/// Convert a saved [`crate::session::PaneNode`] into a live pane tree, clamping
/// leaf tabs to `tab_count`.
fn node_to_pane(node: &crate::session::PaneNode, tab_count: usize) -> crate::pane_tree::Pane {
    match node {
        crate::session::PaneNode::Leaf(i) => crate::pane_tree::Pane::Leaf((*i).min(tab_count.saturating_sub(1))),
        crate::session::PaneNode::Split { dir, ratio, first, second } => crate::pane_tree::Pane::Split {
            dir: if dir == "horizontal" { crate::editor::SplitDir::Horizontal } else { crate::editor::SplitDir::Vertical },
            ratio: (*ratio).clamp(10, 90),
            first: Box::new(node_to_pane(first, tab_count)),
            second: Box::new(node_to_pane(second, tab_count)),
        },
    }
}

/// Convert a live pane tree into a saved node, mapping each leaf's tab index to a
/// file index via `tab_to_file`. `None` if any leaf is an untitled/image tab.
fn pane_to_node(
    pane: &crate::pane_tree::Pane,
    tab_to_file: &[Option<usize>],
) -> Option<crate::session::PaneNode> {
    match pane {
        crate::pane_tree::Pane::Leaf(tab) => Some(crate::session::PaneNode::Leaf((*tab_to_file.get(*tab)?)?)),
        crate::pane_tree::Pane::Split { dir, ratio, first, second } => Some(crate::session::PaneNode::Split {
            dir: match dir {
                crate::editor::SplitDir::Horizontal => "horizontal".to_string(),
                crate::editor::SplitDir::Vertical => "vertical".to_string(),
            },
            ratio: *ratio,
            first: Box::new(pane_to_node(first, tab_to_file)?),
            second: Box::new(pane_to_node(second, tab_to_file)?),
        }),
    }
}

/// Result of resolving a Spacemacs `SPC` leader sequence.
enum LeaderHit {
    /// The sequence maps to this action id; run it.
    Action(&'static str),
    /// The sequence is a prefix of a known one; keep accumulating keys.
    Prefix,
    /// No known sequence starts with this; abort the leader.
    None,
}

/// An active buffer-word autocomplete cycle: the word start, the candidate list,
/// the current index, and the cursor position right after the inserted candidate
/// (used to detect that the cycle is still active on the next keystroke).
struct CompleteSession {
    anchor: usize,
    candidates: Vec<String>,
    index: usize,
    end: usize,
}

/// The LSP code-action chooser: offered actions and the highlighted row. Each
/// action carries its workspace edit (empty when the action is command-only).
pub struct CodeActionMenu {
    /// Offered actions: `(title, per-file edits)`.
    pub actions: Vec<crate::lsp::CodeAction>,
    /// Index of the highlighted action.
    pub selected: usize,
}

/// The LSP code-lens chooser: invokable lenses and the highlighted row. Each
/// lens carries its `(line, title, command, arguments)`.
pub struct CodeLensMenu {
    /// Offered lenses.
    pub lenses: Vec<crate::lsp_core::message::CodeLens>,
    /// Index of the highlighted lens.
    pub selected: usize,
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

/// Recent-locations (jump list) chooser overlay state (Go -> Recent Locations,
/// Alt+E). Lists the cursor positions recorded in the position history,
/// most-recent first; Enter (or a click) jumps to the highlighted one.
pub struct LocationChooser {
    /// Recorded locations, most-recent first, de-duplicated.
    pub entries: Vec<Location>,
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
    /// Open third-level submenu dropdown rectangle (valid while one is open).
    pub subsubmenu_dropdown: Rect,
    /// Color Converter dialog field rows (valid while it is open), for click-to-focus.
    pub color_converter_rows: [Rect; 3],
    /// Unit Converter dialog rows (value, from, to), for click-to-focus.
    pub unit_converter_rows: [Rect; 3],
    /// Calculator dialog hit rects: input field, Run button, Insert button.
    pub calculator_rects: [Rect; 3],
    /// Regex tester dialog field rows (pattern, subject), for click-to-focus.
    pub regex_tester_rows: [Rect; 2],
    /// Pomodoro dialog primary-button (Start/Stop/Cancel) hit rect.
    pub pomodoro_button: Rect,
    /// Info-dialog text-field rectangle (valid while a text dialog is open).
    pub dialog_body: Rect,
    /// Tab-strip rectangle.
    pub tabs: Rect,
    /// Editor viewport rectangle.
    pub editor: Rect,
    /// Editor vertical-scrollbar rectangle (the column right of the editor text).
    pub scrollbar: Rect,
    /// Editor horizontal-scrollbar rectangle (the row below the editor text),
    /// shown when not soft-wrapping and a line overflows.
    pub editor_hscrollbar: Rect,
    /// The whole editor region (inner of the editor block), used to lay out and
    /// hit-test the split panes and their dividers.
    pub editor_region: Rect,
    /// Explorer pane rectangle.
    pub explorer: Rect,
    /// Explorer horizontal-scrollbar rectangle (bottom row), on overflow.
    pub explorer_hscrollbar: Rect,
    /// Message-drawer rectangle.
    pub messages: Rect,
    /// Message-drawer horizontal-scrollbar rectangle (bottom row), on overflow.
    pub messages_hscrollbar: Rect,
    /// Bottom-dock rectangle (valid while the bottom dock is shown).
    pub bottom_dock: Rect,
    /// Bottom-dock horizontal-scrollbar rectangle (bottom row), on overflow.
    pub bottom_hscrollbar: Rect,
    /// Row list rectangle of the open chooser overlay (recent files), so a
    /// click can hit-test which row was picked.
    pub chooser: Rect,
    /// Glyph-grid rectangle of the open Nerd Font palette, so a click can
    /// hit-test which cell was picked.
    pub nerd_palette: Rect,
    /// Row-list rectangle of the open ASCII panel, so a click can hit-test which
    /// row was picked.
    pub ascii_panel: Rect,
    /// Body (data-rows) rectangle of the open table editor, used to size paging
    /// and the scroll window.
    pub edit_table: Rect,
    /// Body rectangle of the open outline editor, used to size paging and the
    /// scroll window.
    pub edit_outline: Rect,
    /// Body rectangle of the open structured-value (JSON/YAML) editor.
    pub edit_value: Rect,
    /// Body rectangle of the open byte (hex) editor.
    pub edit_bytes: Rect,
    /// Body rectangle of the open SQL statement editor.
    pub edit_sql: Rect,
    /// Row-list rectangle of the open X11 color palette, so a click can hit-test
    /// which row was picked.
    pub x11_panel: Rect,
    /// Row-list rectangle of the media-type picker, for click-to-select.
    pub media_type_panel: Rect,
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
    /// Text Information panel row list rect (for click-to-select).
    pub text_info: Rect,
    /// Snippets picker row list rect (for click-to-select).
    pub snippets: Rect,
    /// Transcript rectangle of the open AI chat panel, for mouse-wheel scrolling.
    pub ai_panel: Rect,
    /// Row-list rectangle of the test-results panel, for click-to-jump.
    pub test_panel: Rect,
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
    /// Row-list rectangle of the outline sidebar dock, for click-to-jump.
    pub outline_dock: Rect,
    /// Inner content rectangle of the open find / replace box, so a click can
    /// focus the Find or Replace field.
    pub search: Rect,
    /// Clickable button rectangles in the find box: the Case/Word/Regex toggles
    /// and, in replace mode, the Once/Ask/All replace buttons. `Rect::default()`
    /// when not shown.
    pub search_case: Rect,
    /// See [`Self::search_case`].
    pub search_word: Rect,
    /// See [`Self::search_case`].
    pub search_regex: Rect,
    /// See [`Self::search_case`].
    pub search_once: Rect,
    /// See [`Self::search_case`].
    pub search_ask: Rect,
    /// See [`Self::search_case`].
    pub search_all: Rect,
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
    pub items: Vec<crate::lsp_core::CompletionItem>,
    /// Index of the highlighted candidate.
    pub selected: usize,
}

/// The whole application state.
// Many independent UI/editor toggles; grouping them only relocates the lint
// (a single flags struct would itself exceed the bool limit) and adds noise at
// every call site.
#[allow(clippy::struct_excessive_bools)]
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
    /// Project-wide replace preview awaiting confirmation, when open.
    pub replace_confirm: Option<ReplaceConfirm>,
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
    /// Task chooser overlay (Tools → Tasks…), when open.
    pub task_chooser: Option<TaskChooser>,
    /// Saved-macro chooser overlay (Edit → Play Saved Macro…), when open.
    pub macro_chooser: Option<MacroChooser>,
    /// Recent-projects chooser overlay (File → Switch Project…), when open.
    pub workspace_chooser: Option<WorkspaceChooser>,
    /// Read-only diff overlay (Tools → Compare With File…), when open.
    pub diff_view: Option<DiffViewState>,
    /// Recent-files chooser overlay, when open.
    pub recent_chooser: Option<RecentChooser>,
    /// Recent-locations (jump list) chooser overlay, when open.
    pub location_chooser: Option<LocationChooser>,
    /// Nerd Font palette (character picker) overlay, when open.
    pub nerd_palette: Option<NerdPalette>,
    /// ASCII panel (reference table) overlay, when open.
    pub ascii_panel: Option<AsciiPanel>,
    /// Table editor (CSV/TSV spreadsheet) overlay, when open.
    pub edit_table: Option<crate::edit_table::Grid>,
    /// Outline editor (prose hierarchy) overlay, when open.
    pub edit_outline: Option<crate::edit_outline::Tree>,
    /// Structured-value editor (JSON/YAML tree) overlay, when open.
    pub edit_value: Option<crate::edit_value::Tree>,
    /// Byte editor (hex view) overlay, when open.
    pub edit_bytes: Option<crate::edit_bytes::Hex>,
    /// SQL statement editor overlay, when open.
    pub edit_sql: Option<crate::edit_sql::Editor>,
    /// X11 color palette overlay, when open.
    pub x11_panel: Option<X11Panel>,
    /// Media-type (MIME) picker overlay, when open.
    pub media_type_panel: Option<crate::media_type::Panel>,
    /// HTML character palette overlay, when open.
    pub html_panel: Option<HtmlPanel>,
    /// System Information panel overlay, when open.
    pub system_info: Option<SystemInfoPanel>,
    /// Workspace Dashboard overlay, when open.
    pub dashboard: Option<Dashboard>,
    /// QR code overlay (rendered Unicode art), when open.
    pub qrcode: Option<String>,
    /// AI chat panel overlay (conversation with the configured assistant), when open.
    pub ai_panel: Option<crate::ai_panel::Panel>,
    /// Integrated terminal overlay (a PTY shell), when open.
    pub terminal: Option<crate::terminal::Terminal>,
    /// AI diff-review overlay (accept/reject an Annotate/Improve transform), when open.
    ai_diff: Option<AiDiffState>,
    /// Receiver for the dashboard's background metric computations.
    dashboard_rx: Option<std::sync::mpsc::Receiver<DashMsg>>,
    /// Code outline overlay, when open.
    pub outline: Option<Outline>,
    /// Persistent code-outline sidebar (symbol list following the cursor), when
    /// the dock is shown. Rebuilt by `refresh_outline_dock`.
    pub outline_dock: Option<Outline>,
    /// Cache key for the outline sidebar: `(active tab index, buffer revision)`,
    /// so symbols are rescanned only when the buffer changes.
    outline_dock_key: Option<(usize, u64)>,
    /// First-run welcome overlay, when shown.
    pub welcome: Option<WelcomePanel>,
    /// File Information overlay, when open.
    pub file_info: Option<FileInfoPanel>,
    /// Text Information overlay, when open.
    pub text_info: Option<TextInfoPanel>,
    /// Markdown preview overlay, when open.
    pub markdown_preview: Option<MarkdownPreview>,
    /// Snippets picker overlay, when open.
    pub snippets: Option<crate::snippets::Picker>,
    /// The in-scope snippet library (bundled + global + media-type + project),
    /// rebuilt for the active buffer's media type.
    pub snippet_library: Vec<crate::snippets::Snippet>,
    /// The media-type key the `snippet_library` was last built for (cache guard).
    snippet_library_key: Option<String>,
    /// Active snippet tabstop session (Tab navigates the fields), when expanding.
    snippet_session: Option<SnippetSession>,
    /// Contact-browser overlay, when open.
    pub contacts: Option<ContactPanel>,
    /// Single-vCard view overlay, when open (above the contact browser).
    pub vcard: Option<VcardPanel>,
    /// LSP client: language-server process management and document sync.
    pub lsp: crate::lsp::Lsp,
    /// Last document revision pushed to a language server, keyed by file path, so
    /// edits sync once per change rather than once per frame.
    lsp_synced: std::collections::HashMap<PathBuf, u64>,
    /// Inline-blame cache: the `(path, 1-based line)` last blamed, so the blame is
    /// recomputed only when the cursor moves to a different line.
    blame_cache: Option<(PathBuf, usize)>,
    /// Debug Adapter Protocol client (one active session).
    pub dap: crate::dap::Dap,
    /// Breakpoints per file: absolute path → set of 1-based lines.
    breakpoints: std::collections::HashMap<PathBuf, std::collections::BTreeSet<usize>>,
    /// Where the debugger is currently stopped: `(path, 1-based line)`.
    dap_stopped: Option<(PathBuf, usize)>,
    /// Latest call stack from the debugger.
    pub dap_stack: Vec<crate::dap::Frame>,
    /// Latest variables (top frame, first scope) from the debugger.
    pub dap_variables: Vec<crate::dap::Variable>,
    /// Watch expressions and their last results: `(expr, result)`.
    pub dap_watches: Vec<(String, String)>,
    /// Whether the Debug panel (stack / variables / watch) is shown.
    pub show_debug_panel: bool,
    /// While true, command output lines are also captured for test parsing.
    test_capture: bool,
    /// Buffered output of the running test command, parsed on completion.
    test_buffer: Vec<String>,
    /// Parsed results of the last test run.
    pub test_results: Vec<crate::test_runner::TestResult>,
    /// Highlighted row in the test panel.
    pub test_selected: usize,
    /// Whether the test-results panel is shown.
    pub show_test_panel: bool,
    /// LSP hover tooltip overlay, when shown.
    pub hover: Option<HoverPopup>,
    /// LSP completion overlay, when shown.
    pub completion: Option<CompletionPopup>,
    /// Modal info dialog (Vix menu About / Website / Email), when open.
    pub dialog: Option<Dialog>,
    /// Color Converter dialog (Tools → Color Converter…), when open.
    pub color_converter: Option<crate::color_converter_tool::Converter>,
    /// Unit Converter dialog (Tools → Convert → Unit Converter…), when open.
    pub unit_converter: Option<crate::unit_converter_tool::Converter>,
    /// Calculator dialog (Tools → Calculator…), when open.
    pub calculator: Option<crate::calculator_tool::Calculator>,
    /// Regex tester dialog (Tools → Regex Tester…), when open.
    pub regex_tester: Option<crate::regex_tool::Tester>,
    /// Code-action chooser (LSP quick fixes / refactors), when open.
    pub code_actions: Option<CodeActionMenu>,
    /// Code-lens chooser (LSP), when open.
    pub code_lens: Option<CodeLensMenu>,
    /// Pomodoro timer state (Tools → Pomodoro…). Stays `Some` and keeps counting
    /// down even after the dialog is closed via Start; see [`Self::pomodoro_open`].
    pub pomodoro: Option<crate::pomodoro_tool::Timer>,
    /// Whether the Pomodoro dialog is currently visible. The timer keeps running
    /// in the background while this is `false`; the break alert re-opens it.
    pub pomodoro_open: bool,
    /// Wall-clock anchor for the running Pomodoro countdown; `None` while idle.
    pomodoro_last_tick: Option<std::time::Instant>,
    /// Explorer clipboard: paths plus whether this is a cut (move) or copy.
    pub clip: Vec<PathBuf>,
    /// Whether [`App::clip`] holds a cut (move) rather than a copy.
    pub clip_cut: bool,
    /// Position-history jump list (Alt+Left / Alt+Right).
    pub nav_history: Vec<Location>,
    /// User bookmarks (file + line), toggled per line and navigable as a set.
    pub bookmarks: Vec<Location>,
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
    /// Overwrite (type-over) mode: typed characters replace the one under the
    /// cursor instead of inserting. Session-only; toggled with `toggle_overwrite_mode`.
    pub overwrite: bool,
    /// Show a vertical guide at the [`crate::ui::RULER_COLUMN`] text column.
    /// Session-only; toggled with `toggle_ruler`.
    pub show_ruler: bool,
    /// Whether a keyboard macro is being recorded (capturing editor keys).
    pub macro_recording: bool,
    /// The recorded editor key sequence, replayed by `macro.play`.
    macro_keys: Vec<KeyEvent>,
    /// True while replaying, to suppress re-recording and recursion.
    macro_playing: bool,
    /// In-progress word-completion cycle (buffer-word autocomplete).
    complete_session: Option<CompleteSession>,
    /// Cursor position captured when an LSP rename prompt was opened, used to
    /// send the rename request on submit: `(file, 0-based line, character)`.
    rename_at: Option<(PathBuf, u32, u32)>,
    /// Direction of a pending `selectionRange` request (`true` = expand, `false`
    /// = shrink), applied when the response arrives.
    expand_selection_dir: Option<bool>,
    /// Whether LSP inlay hints are displayed (toggled via `view.inlay_hints`).
    show_inlay_hints: bool,
    /// Set by the `suspend` action; the main loop suspends the process
    /// (`SIGTSTP`) on Unix and clears it on resume.
    pub suspend_requested: bool,
    /// Linked-editing ranges (char offsets in the active buffer) captured when
    /// the linked-edit prompt was opened, replaced together on submit.
    linked_ranges: Option<Vec<(usize, usize)>>,
    /// Whether the workspace root is a git work tree (checked once at startup).
    pub git_repo: bool,
    /// Cached current git branch (or short hash when detached), when in a repo.
    pub git_branch: Option<String>,
    /// Cached `git status` rows (changed files), refreshed on save / git actions.
    pub git_status: Vec<crate::git::FileStatus>,
    /// Cached HEAD blob text per file path, for the editor diff gutter. Cleared
    /// on save / git actions so it refetches.
    git_head_cache: std::collections::HashMap<PathBuf, String>,
    /// Whether spell-checking (red underline in comments/strings) is enabled.
    pub spellcheck: bool,
    /// Loaded spell checker for the active locale, when spell-checking is on and
    /// a dictionary was found.
    pub speller: Option<crate::spellcheck::SpellChecker>,
    /// Locale the loaded (or last-attempted) [`speller`](Self::speller) is for, so
    /// it is reloaded only on a locale change.
    speller_locale: Option<String>,
    /// Whether the bottom dock (log/output/data panel) is shown.
    pub show_bottom_dock: bool,
    /// Whether the breadcrumb bar (file ▸ symbol) is shown above the editor.
    pub show_breadcrumbs: bool,
    /// Saved dock/status visibility while zen (focus) mode is active, restored on
    /// exit. `Some` iff zen mode is on. Holds (explorer, messages, bottom, status).
    pub zen_saved: Option<(bool, bool, bool, bool)>,
    /// Bottom-dock line buffer.
    pub bottom_dock: crate::bottom_dock::BottomDock,
    /// Horizontal scroll offset (chars) of the bottom dock.
    pub bottom_hscroll: usize,
    /// Horizontal scroll offset (chars) of the file explorer.
    pub explorer_hscroll: usize,
    /// Horizontal scroll offset (chars) of the message drawer.
    pub messages_hscroll: usize,
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
    /// Live filter text for the keyboard-help browser (matches keys + description).
    pub help_filter: String,
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
    /// Action ids of commands recently run from the palette, most-recent first
    /// (capped). Surfaced at the top of the `>` command list when the query is
    /// empty, and used as a tiebreak when ranking matches.
    command_recents: Vec<String>,
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
    /// The theme name currently applied as a menu hover/keyboard preview (reverted
    /// to the committed theme when the menu closes or the pointer leaves it).
    theme_preview: Option<String>,
    /// True while the editor scrollbar thumb is being dragged, so the drag keeps
    /// scrolling even if the pointer drifts off the one-column track.
    scrollbar_active: bool,
    /// Which view's horizontal scrollbar is being dragged, if any.
    hbar_active: Option<HBar>,
    /// True while the split divider is being dragged to resize the panes.
    split_resize: bool,
    /// Max horizontal scroll (`content_width − viewport`) recorded each render for
    /// the editor, explorer, message drawer, and bottom dock, for scrollbar drag.
    pub editor_hmax: usize,
    /// See [`Self::editor_hmax`].
    pub explorer_hmax: usize,
    /// See [`Self::editor_hmax`].
    pub messages_hmax: usize,
    /// See [`Self::editor_hmax`].
    pub bottom_hmax: usize,
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
    /// Spacemacs keymap: true in Insert mode, false in Normal mode. Meaningless in
    /// other keymaps.
    spacemacs_insert: bool,
    /// Spacemacs keymap: the in-progress `Space` leader key sequence (after `SPC`
    /// in Normal mode), or `None` when no leader is pending.
    spacemacs_leader: Option<String>,
}

impl App {
    /// Build the editor, welcome messages, and LSP client for a fresh app, and
    /// apply the saved theme, theme menu, and time zone. Extracted from
    /// [`App::new`] so the constructor stays within the line limit.
    fn build_core(root: &Path, settings: &Settings) -> (Editor, Messages, crate::lsp::Lsp) {
        // Apply the saved theme before building any editor so the first buffer is
        // styled correctly. A theme value that is not a built-in mode is treated
        // as the name of a custom JSON theme.
        Self::apply_saved_theme(&settings.theme);
        // Populate the View → Theme submenu with the available theme names before
        // the menu bar is first rendered.
        let theme_names = crate::theme_model::theme_names(&Self::available_custom_themes());
        crate::menu::set_theme_names(theme_names);
        // Apply the saved time zone so the clock panel and status bar use it.
        crate::time_zone_model::set_active(&settings.time_zone);
        let mut editor = Editor::new(
            settings.line_numbers,
            settings.show_whitespace,
            settings.soft_wrap,
            settings.indent_string(),
        );
        for tab in &mut editor.tabs {
            tab.editor.set_auto_pair(settings.auto_pair);
        }
        let mut messages = Messages::default();
        messages.advice(t!("msg.welcome").to_string());
        messages.info(t!("msg.welcome_hint").to_string());

        let lsp = crate::lsp::Lsp::new(
            settings.lsp_enabled,
            settings.lsp_servers.clone(),
            root,
        );
        (editor, messages, lsp)
    }

    /// Build an app rooted at `root` using the given `settings`.
    ///
    /// The active locale and theme should already be applied by the caller
    /// (see `main`); the theme is (re)applied here so the first buffer is styled
    /// correctly, and the welcome messages are produced in the current locale.
    #[must_use]
    pub fn new(root: PathBuf, settings: Settings) -> Self {
        // Seed the in-memory palette recents from the persisted list.
        let command_recents = settings.command_recents.clone();
        let (editor, messages, lsp) = Self::build_core(&root, &settings);

        App {
            explorer: Explorer::new(root.clone()),
            root,
            editor,
            messages,
            menu: Menu::default(),
            palette: None, search: None, query_replace: None, workspace_search: None,
            prompt: None, paste: None, confirm: None, replace_confirm: None, unsaved: None, spell_suggest: None,
            context_menu: None, git_panel: None, branch_chooser: None, task_chooser: None, macro_chooser: None, workspace_chooser: None, diff_view: None, recent_chooser: None,
            location_chooser: None, nerd_palette: None, ascii_panel: None, edit_table: None,
            edit_outline: None, edit_value: None, edit_bytes: None, edit_sql: None, qrcode: None,
            x11_panel: None,
            media_type_panel: None,
            html_panel: None, system_info: None, dashboard: None, dashboard_rx: None,
            outline: None, outline_dock: None, outline_dock_key: None,
            welcome: None, file_info: None, text_info: None,
            markdown_preview: None, snippets: None, snippet_library: Vec::new(),
            snippet_library_key: None, snippet_session: None, contacts: None, vcard: None,
            lsp,
            lsp_synced: std::collections::HashMap::new(),
            blame_cache: None,
            dap: crate::dap::Dap::new(), breakpoints: std::collections::HashMap::new(),
            dap_stopped: None, dap_stack: Vec::new(), dap_variables: Vec::new(),
            dap_watches: Vec::new(), show_debug_panel: false, test_capture: false,
            test_buffer: Vec::new(), test_results: Vec::new(), test_selected: 0, show_test_panel: false,
            hover: None, completion: None, dialog: None, color_converter: None,
            unit_converter: None, calculator: None, regex_tester: None, code_actions: None,
            code_lens: None, pomodoro: None,
            pomodoro_open: false, pomodoro_last_tick: None,
            clip: Vec::new(), clip_cut: false,
            nav_history: Vec::new(), bookmarks: Vec::new(), nav_idx: 0,
            picker: None,
            show_explorer: settings.show_explorer,
            show_messages: settings.show_messages,
            show_status_bar: settings.show_status_bar,
            show_breadcrumbs: settings.show_breadcrumbs,
            zen_saved: None,
            show_scrollbar: settings.show_scrollbar,
            overwrite: false,
            show_ruler: false,
            macro_recording: false,
            macro_keys: Vec::new(),
            macro_playing: false,
            complete_session: None,
            rename_at: None,
            expand_selection_dir: None,
            show_inlay_hints: true,
            suspend_requested: false,
            linked_ranges: None,
            git_repo: false,
            git_branch: None,
            git_status: Vec::new(),
            git_head_cache: std::collections::HashMap::new(),
            spellcheck: settings.spellcheck,
            speller: None,
            speller_locale: None,
            show_bottom_dock: settings.show_bottom_dock,
            bottom_dock: crate::bottom_dock::BottomDock::with_scrollback(settings.scrollback),
            bottom_hscroll: 0,
            explorer_hscroll: 0,
            messages_hscroll: 0,
            show_calendar: false,
            calendar: crate::calendar::Calendar::new(),
            show_clock: false,
            clock: crate::clock::Clock::new(),
            show_help: false,
            help_filter: String::new(),
            focus: Focus::Editor,
            status: t!("status.ready").to_string(),
            should_quit: false,
            layout: Layout::default(),
            settings,
            file_index: Vec::new(),
            palette_origin: None,
            command_recents,
            last_search: None,
            closed_tabs: Vec::new(),
            running_command: None,
            ai_replace: None,
            ai_panel: None,
            terminal: None,
            ai_diff: None,
            theme_preview: None,
            scrollbar_active: false,
            hbar_active: None,
            split_resize: false,
            editor_hmax: 0,
            explorer_hmax: 0,
            messages_hmax: 0,
            bottom_hmax: 0,
            dock_resize: None,
            emacs_prefix: false,
            vim_insert: false,
            vim_cmd: None,
            spacemacs_insert: false,
            spacemacs_leader: None,
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
    pub fn open_initial(&mut self, path: &Path) {
        self.open_path(path, false);
    }

    /// A stable string key for the current workspace root (canonicalized when
    /// possible, so symlinked paths map to one session entry).
    fn session_key(&self) -> String {
        self.root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone())
            .to_string_lossy()
            .into_owned()
    }

    /// Reopen the previous session for this workspace: its open files, focused
    /// tab, and per-file cursor positions. No-op when session restore is
    /// disabled or there is no saved session for this root. Called by `main`
    /// only when no file was given on the command line.
    pub fn restore_session(&mut self) {
        if !self.settings.restore_session {
            return;
        }
        let key = self.session_key();
        let session = crate::session::Session::load();
        let Some(ws) = session.workspace(&key).cloned() else {
            return;
        };
        let opened = self.apply_session(&ws);
        if opened > 0 {
            self.status = t!("status.session_restored", count = opened).to_string();
        }
    }

    /// Reopen the files/cursors/active tab described by `ws`, returning how many
    /// files were actually opened. Pure (no config IO) so it can be tested
    /// directly; the fresh app's blank untitled buffer is dropped once at least
    /// one real file is reopened.
    pub fn apply_session(&mut self, ws: &crate::session::WorkspaceSession) -> usize {
        let had_blank = self.editor.tabs.len() == 1
            && self.editor.tabs[0].path.is_none()
            && !self.editor.tabs[0].dirty
            && self.editor.tabs[0].text().trim().is_empty();

        let mut opened = 0usize;
        for (i, file) in ws.files.iter().enumerate() {
            let path = PathBuf::from(file);
            if !path.is_file() {
                continue;
            }
            self.open_path(&path, false);
            if let Some(t) = self.editor.active_tab_mut() {
                let max = t.editor.code_ref().len_chars();
                let pos = ws.cursors.get(i).copied().unwrap_or(0).min(max);
                t.editor.set_cursor(pos);
                t.editor.set_offset_y(ws.scrolls.get(i).copied().unwrap_or(0));
            }
            opened += 1;
        }

        if opened > 0 {
            if had_blank {
                self.editor.tabs.remove(0);
            }
            self.editor.active = ws.active.min(self.editor.tabs.len().saturating_sub(1));
            // Restore the split only when every file reopened cleanly, so the
            // recorded pane index still lines up with the tab order.
            if had_blank && opened == ws.files.len() {
                self.restore_split(ws);
            }
        }
        opened
    }

    /// Rebuild the editor split recorded in `ws`, if any. Leaves index into the
    /// reopened files, which (after a clean restore) match the tab order.
    fn restore_split(&mut self, ws: &crate::session::WorkspaceSession) {
        let Some(s) = ws.split.as_ref() else { return };
        let count = self.editor.tabs.len();
        let tree = node_to_pane(&s.tree, count);
        if tree.leaf_count() < 2 {
            return;
        }
        self.editor.focused_leaf = s.focused.min(tree.leaf_count() - 1);
        if let Some(tab) = tree.leaf_tab(self.editor.focused_leaf) {
            self.editor.active = tab.min(count.saturating_sub(1));
        }
        self.editor.split_root = Some(tree);
    }

    /// Capture the current open files, focused tab, and cursor positions as a
    /// [`WorkspaceSession`](crate::session::WorkspaceSession). Untitled and image
    /// tabs are skipped. Pure (no config IO) so it can be tested directly.
    #[must_use]
    pub fn workspace_session(&self) -> crate::session::WorkspaceSession {
        let mut files = Vec::new();
        let mut cursors = Vec::new();
        let mut scrolls = Vec::new();
        let mut active = 0;
        // Map each editor tab index to its position in `files` (None for skipped
        // untitled/image tabs), so the split's pane index can be translated.
        let mut tab_to_file: Vec<Option<usize>> = Vec::with_capacity(self.editor.tabs.len());
        for (i, tab) in self.editor.tabs.iter().enumerate() {
            let Some(path) = tab.path.as_ref().filter(|_| !tab.is_image()) else {
                tab_to_file.push(None);
                continue;
            };
            if i == self.editor.active {
                active = files.len();
            }
            tab_to_file.push(Some(files.len()));
            files.push(path.to_string_lossy().into_owned());
            cursors.push(tab.editor.get_cursor());
            scrolls.push(tab.editor.get_offset_y());
        }
        let split = self.editor.split_root.as_ref().and_then(|root| {
            let tree = pane_to_node(root, &tab_to_file)?;
            Some(crate::session::SplitSession { tree, focused: self.editor.focused_leaf })
        });
        crate::session::WorkspaceSession { root: self.session_key(), files, active, cursors, scrolls, split }
    }

    /// Capture the current session and persist it to the per-workspace store.
    fn save_session(&self) {
        let ws = self.workspace_session();
        let mut session = crate::session::Session::load();
        session.set_workspace(ws);
        let _ = session.save();
    }

    // ----- top-level event entry -----------------------------------------

    /// Route `key` to the highest-priority open modal layer, if any. Returns
    /// `true` when a layer consumed the key (so [`App::on_key`] should stop).
    /// Extracted from `on_key` to keep that function within the line limit.
    /// Route a key to an open tool dialog (color/unit converter, calculator,
    /// regex tester, code-action chooser). Returns `true` if one consumed it.
    fn try_tool_dialog_key(&mut self, key: KeyEvent) -> bool {
        if self.color_converter.is_some() {
            self.color_converter_key(key);
        } else if self.unit_converter.is_some() {
            self.unit_converter_key(key);
        } else if self.calculator.is_some() {
            self.calculator_key(key);
        } else if self.regex_tester.is_some() {
            self.regex_tester_key(key);
        } else if self.code_actions.is_some() {
            self.code_action_key(key);
        } else if self.code_lens.is_some() {
            self.code_lens_key(key);
        } else {
            return false;
        }
        true
    }

    fn try_overlay_key(&mut self, key: KeyEvent) -> bool {
        if self.welcome.is_some() {
            self.welcome_key(key);
            return true;
        }
        if self.show_help {
            match key.code {
                KeyCode::Esc | KeyCode::F(1) => {
                    self.show_help = false;
                    self.help_filter.clear();
                }
                KeyCode::Backspace => {
                    self.help_filter.pop();
                }
                KeyCode::Char(c) if !Self::ctrl(&key) && !Self::alt(&key) => {
                    self.help_filter.push(c);
                }
                _ => {}
            }
            return true;
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
                return true;
            }
            if key.code == KeyCode::Esc {
                self.dialog = None;
                return true;
            }
            let area = self.dialog_field_area();
            if let Some(ed) = self.dialog.as_mut().and_then(|d| d.editor.as_mut()) {
                if Self::ctrl(&key) && matches!(key.code, KeyCode::Char('c')) {
                    ed.apply(CopyAction {});
                } else {
                    let _ = ed.input(key, &area);
                }
            }
            return true;
        }
        if self.try_tool_dialog_key(key) {
            return true;
        }
        if self.pomodoro_open {
            self.pomodoro_key(key);
            return true;
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
            return true;
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
            return true;
        }
        self.try_panel_key(key)
    }

    /// Route `key` to the lower-priority list/panel overlays (choosers, tool
    /// panels, prompt, palette, search, menu). Returns `true` when one consumed
    /// the key. Split out of [`App::try_overlay_key`] to keep both within the
    /// line limit; the priority order is preserved by calling this last.
    fn try_panel_key(&mut self, key: KeyEvent) -> bool {
        // Each panel that is open captures the key by delegating to its handler.
        macro_rules! panel {
            ($field:ident, $handler:ident) => {
                if self.$field.is_some() {
                    self.$handler(key);
                    return true;
                }
            };
        }
        // The integrated terminal captures all keys (forwarded to the shell)
        // except its close chord, handled inside the key handler.
        panel!(terminal, terminal_key);
        panel!(edit_table, edit_table_key);
        panel!(edit_outline, edit_outline_key);
        panel!(edit_value, edit_value_key);
        panel!(edit_bytes, edit_bytes_key);
        panel!(edit_sql, edit_sql_key);
        panel!(recent_chooser, recent_key);
        panel!(location_chooser, location_key);
        panel!(nerd_palette, nerd_key);
        panel!(ascii_panel, ascii_key);
        panel!(x11_panel, x11_key);
        panel!(media_type_panel, media_type_key);
        panel!(html_panel, html_key);
        panel!(system_info, system_info_key);
        panel!(file_info, file_info_key);
        panel!(text_info, text_info_key);
        panel!(markdown_preview, markdown_preview_key);
        panel!(snippets, snippets_key);
        panel!(vcard, vcard_key);
        panel!(contacts, contacts_key);
        if self.dashboard.is_some() {
            if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
                self.close_dashboard();
            }
            return true;
        }
        if self.qrcode.is_some() {
            if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q')) {
                self.qrcode = None;
            }
            return true;
        }
        panel!(ai_diff, ai_diff_key);
        panel!(ai_panel, ai_panel_key);
        panel!(outline, outline_key);
        panel!(query_replace, qr_key);
        panel!(replace_confirm, replace_confirm_key);
        panel!(workspace_search, ps_key);
        panel!(confirm, confirm_key);
        panel!(unsaved, unsaved_key);
        panel!(spell_suggest, spell_suggest_key);
        panel!(context_menu, context_menu_key);
        panel!(git_panel, git_panel_key);
        panel!(branch_chooser, branch_key);
        panel!(task_chooser, tasks_key);
        panel!(macro_chooser, macro_key);
        panel!(workspace_chooser, workspace_chooser_key);
        panel!(diff_view, diff_view_key);
        if self.paste.as_ref().is_some_and(|p| p.conflict.is_some()) {
            self.paste_key(key);
            return true;
        }
        panel!(prompt, prompt_key);
        panel!(palette, palette_key);
        panel!(search, search_key);
        if self.menu.is_open() {
            self.menu_key(key);
            return true;
        }
        false
    }

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
        if self.try_overlay_key(key) {
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
            Keymap::Spacemacs => {
                if self.spacemacs_key(key) || self.global_shared_key(key) {
                    return;
                }
            }
            Keymap::IntelliJMac => {
                if self.intellij_key(key, false) || self.global_shared_key(key) {
                    return;
                }
            }
            Keymap::IntelliJWin => {
                if self.intellij_key(key, true) || self.global_shared_key(key) {
                    return;
                }
            }
            Keymap::Eclipse => {
                if self.eclipse_key(key) || self.global_shared_key(key) {
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
            Keymap::Spacemacs => Some(if let Some(seq) = &self.spacemacs_leader {
                format!("SPC {seq}")
            } else if self.spacemacs_insert {
                t!("status.vim_insert").to_string()
            } else {
                t!("status.vim_normal").to_string()
            }),
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
        if Self::ctrl(&key)
            && let KeyCode::Char(c) = key.code {
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
                    't' => self.run_action("nav.goto_workspace_symbol"),
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
        if Self::ctrl(&key)
            && let KeyCode::Char(c) = key.code {
                match c.to_ascii_lowercase() {
                    'q' => self.run_action("file.quit"),
                    'n' => self.run_action("file.new"),
                    's' if Self::shift(&key) => self.run_action("file.save_as"),
                    's' => self.run_action("file.save"),
                    'w' if Self::shift(&key) => self.run_action("file.close_all"),
                    'w' => self.run_action("file.close"),
                    't' if Self::shift(&key) => self.run_action("file.reopen_closed"),
                    't' => self.run_action("nav.goto_workspace_symbol"),
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
        false
    }

    // ----- keymap: IntelliJ (macOS / Windows) -----------------------

    /// `IntelliJ` IDEA keymap dispatch (`Ctrl` stands in for `Cmd` on macOS).
    /// `win` selects the Windows/Linux go-to bindings (Ctrl+N / Ctrl+Shift+N /
    /// Ctrl+G) vs the macOS ones (Ctrl+O / Ctrl+Shift+O / Ctrl+L). Editing chords
    /// (undo/cut/copy/paste/select-all) fall through to the editor widget. Returns
    /// true if consumed.
    #[allow(clippy::too_many_lines)]
    fn intellij_key(&mut self, key: KeyEvent, win: bool) -> bool {
        // `Ctrl+Alt+L`: reformat (Reformat Code).
        if Self::ctrl(&key) && Self::alt(&key) {
            if let KeyCode::Char(c) = key.code {
                match c.to_ascii_lowercase() {
                    'l' => self.run_action("lsp.format"),
                    'o' => self.run_action("nav.goto_workspace_symbol"),
                    _ => return false,
                }
                return true;
            }
            return false;
        }
        if !Self::ctrl(&key) {
            return false;
        }
        let KeyCode::Char(c) = key.code else { return false };
        let shift = Self::shift(&key);
        match c.to_ascii_lowercase() {
            'a' if shift => self.run_action("tools.palette"), // Find Action
            's' if shift => self.run_action("file.save_as"),
            's' => self.run_action("file.save"),
            'w' if shift => self.run_action("file.close_all"),
            'w' => self.run_action("file.close"),
            'f' if shift => self.run_action("search.workspace"),
            'f' => self.run_action("edit.find"),
            'r' if shift => self.run_action("search.workspace_replace"),
            'r' => self.run_action("edit.replace"),
            'b' => self.run_action("nav.goto_definition"),    // Go to Declaration
            'd' => self.run_action("edit.duplicate_line"),    // Duplicate
            'y' if win => self.run_action("edit.delete_line"), // Win: delete line
            '/' | '7' | '_' => self.run_action("edit.toggle_comment"),
            ',' if !win => self.run_action("vix.settings"),   // macOS: Cmd+,
            // Go to file / class, and Go to Line differ by platform.
            'n' if win && shift => self.run_action("file.open"),         // Go to File
            'n' if win => self.run_action("nav.goto_symbol"),            // Go to Class
            'n' if !win => self.run_action("file.new"),
            'o' if !win && shift => self.run_action("file.open"),        // Go to File
            'o' if !win => self.run_action("nav.goto_symbol"),           // Go to Class
            'l' if !win => self.run_action("nav.goto_line"),             // macOS: Cmd+L
            'g' if win => self.run_action("nav.goto_line"),              // Win: Ctrl+G
            'g' if shift => self.run_action("edit.find_prev"),
            'g' => self.run_action("edit.find_next"),                    // macOS: Cmd+G
            _ => return false,
        }
        true
    }

    // ----- keymap: Eclipse ------------------------------------------------

    /// Eclipse (Windows) keymap dispatch. Editing chords fall through to the
    /// editor widget. Returns true if consumed.
    #[allow(clippy::too_many_lines)]
    fn eclipse_key(&mut self, key: KeyEvent) -> bool {
        // `Alt+/`: word completion.
        if Self::alt(&key) && !Self::ctrl(&key) {
            if matches!(key.code, KeyCode::Char('/')) {
                self.run_action("autocomplete");
                return true;
            }
            return false;
        }
        if !Self::ctrl(&key) {
            return false;
        }
        let KeyCode::Char(c) = key.code else { return false };
        let shift = Self::shift(&key);
        match c.to_ascii_lowercase() {
            'n' => self.run_action("file.new"),
            'w' if shift => self.run_action("file.close_all"),
            'w' => self.run_action("file.close"),
            's' if shift => self.run_action("file.save_as"),
            's' => self.run_action("file.save"),
            'y' => self.run_action("edit.redo"),              // Win redo
            'f' if shift => self.run_action("lsp.format"),    // Format
            'f' => self.run_action("edit.find"),
            'k' if shift => self.run_action("edit.find_prev"),
            'k' => self.run_action("edit.find_next"),
            'h' => self.run_action("search.workspace"),       // Search
            'l' => self.run_action("nav.goto_line"),
            'd' => self.run_action("edit.delete_line"),
            'o' => self.run_action("nav.goto_symbol"),        // Quick Outline
            'r' if shift => self.run_action("file.open"),     // Open Resource
            'r' => self.run_action("edit.replace"),
            't' if shift => self.run_action("nav.goto_workspace_symbol"), // Open Type
            'b' if shift => self.run_action("debug.toggle_breakpoint"),
            'b' => self.run_action("tools.test"),             // Build All
            '3' => self.run_action("tools.palette"),          // Quick Access
            '/' | '7' | '_' => self.run_action("edit.toggle_comment"),
            _ => return false,
        }
        true
    }

    /// Keys shared by every keymap: menu-bar mnemonics and function keys. Returns
    /// true if consumed. The menu-bar `Alt+letter` mnemonics live in
    /// [`menu_index_for_alt`].
    fn global_shared_key(&mut self, key: KeyEvent) -> bool {
        if Self::alt(&key)
            && let KeyCode::Char(c) = key.code
            && let Some(i) = menu_index_for_alt(c)
        {
            self.menu.open_index(i);
            return true;
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
                let a = if Self::shift(&key) { "edit.column_select_up" } else { "edit.move_line_up" };
                self.run_action(a);
                true
            }
            KeyCode::Down if Self::alt(&key) && self.focus == Focus::Editor => {
                let a = if Self::shift(&key) { "edit.column_select_down" } else { "edit.move_line_down" };
                self.run_action(a);
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
            KeyCode::F(2) => {
                self.run_action("lsp.rename");
                true
            }
            KeyCode::F(6) => {
                self.editor.focus_other_pane();
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
            KeyCode::Char('j' | 'J') if Self::alt(&key) => {
                self.run_action("nav.recent_locations");
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
            if Self::ctrl(&key)
                && let KeyCode::Char(c) = key.code {
                    match c.to_ascii_lowercase() {
                        'f' => self.run_action("file.open"),
                        's' => self.run_action("file.save"),
                        'c' => self.run_action("file.quit"),
                        _ => self.status = t!("status.emacs_no_chord").to_string(),
                    }
                    return true;
                }
            if let KeyCode::Char('k') = key.code {
                self.run_action("file.close");
                return true;
            }
            self.status = t!("status.emacs_no_chord").to_string();
            return true;
        }
        if Self::ctrl(&key)
            && let KeyCode::Char(c) = key.code {
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
                if let Some(s) = self.vim_cmd.as_mut()
                    && s.pop().is_none() {
                        self.vim_cmd = None;
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

    // ----- Spacemacs keymap ----------------------------------------------

    /// Spacemacs dispatch: Vim-style modal editing in Normal/Insert, plus a
    /// `Space` leader that opens menu-like command sequences. Returns true if the
    /// key was consumed.
    fn spacemacs_key(&mut self, key: KeyEvent) -> bool {
        // A leader sequence is in progress (after `SPC` in Normal mode).
        if self.spacemacs_leader.is_some() {
            self.spacemacs_leader_key(key);
            return true;
        }
        // Shared `:` command line (reuses the Vim command machinery).
        if self.vim_cmd.is_some() {
            self.vim_cmd_key(key);
            return true;
        }
        if self.spacemacs_insert {
            if key.code == KeyCode::Esc {
                self.spacemacs_insert = false;
                return true;
            }
            return false;
        }
        if Self::ctrl(&key) || Self::alt(&key) || matches!(key.code, KeyCode::F(_)) {
            return false;
        }
        // `SPC` opens the leader from the editor; elsewhere it types normally.
        if key.code == KeyCode::Char(' ') && self.focus == Focus::Editor {
            self.spacemacs_leader = Some(String::new());
            return true;
        }
        if key.code == KeyCode::Char(':') {
            self.vim_cmd = Some(String::new());
            return true;
        }
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
            KeyCode::Char('i') => self.spacemacs_insert = true,
            KeyCode::Char('a') => {
                self.editor_motion(KeyCode::Right);
                self.spacemacs_insert = true;
            }
            KeyCode::Char('o') => {
                self.editor_motion(KeyCode::End);
                self.editor_motion(KeyCode::Enter);
                self.spacemacs_insert = true;
            }
            KeyCode::Char('O') => {
                self.editor_motion(KeyCode::Home);
                self.editor_motion(KeyCode::Enter);
                self.editor_motion(KeyCode::Up);
                self.spacemacs_insert = true;
            }
            _ => {}
        }
        true
    }

    /// Handle a key while a Spacemacs `SPC` leader sequence is accumulating. Runs
    /// the action when the sequence matches, keeps waiting while it is a prefix,
    /// and aborts (with a status note) otherwise.
    fn spacemacs_leader_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            self.spacemacs_leader = None;
            return;
        }
        let KeyCode::Char(c) = key.code else { return };
        let mut seq = self.spacemacs_leader.take().unwrap_or_default();
        seq.push(c);
        match Self::spacemacs_leader_lookup(&seq) {
            LeaderHit::Action(action) => self.run_action(action),
            LeaderHit::Prefix => self.spacemacs_leader = Some(seq),
            LeaderHit::None => self.status = t!("status.spacemacs_no_leader", seq = seq).to_string(),
        }
    }

    /// Resolve a Spacemacs leader sequence: an exact action, a longer-prefix match
    /// (keep waiting), or nothing.
    fn spacemacs_leader_lookup(seq: &str) -> LeaderHit {
        /// `(sequence, action)` pairs for the `SPC` leader (Spacemacs-style).
        const LEADER: &[(&str, &str)] = &[
            (" ", "tools.palette"),   // SPC SPC — M-x / command palette
            ("ff", "file.open"),      // find file
            ("fr", "file.open_recent"),
            ("fs", "file.save"),
            ("fp", "file.switch_project"),
            ("bn", "tab.next"),       // buffers
            ("bp", "tab.prev"),
            ("bd", "file.close"),
            ("pf", "tools.palette"),  // project: find/command
            ("pp", "file.switch_project"),
            ("pt", "view.explorer"),  // project tree
            ("gs", "git.changes"),    // git status
            ("gg", "git.status"),
            ("gb", "git.blame"),
            ("w/", "view.split_vertical"),
            ("w-", "view.split_horizontal"),
            ("wd", "view.unsplit"),
            ("ww", "view.focus_other_pane"),
            ("ss", "edit.find"),      // search
            ("sp", "search.workspace"),
            ("tn", "view.line_numbers"), // toggles
            ("tw", "view.whitespace"),
            ("qq", "file.quit"),
            (";", "edit.toggle_comment"),
        ];
        if let Some(&(_, action)) = LEADER.iter().find(|&&(s, _)| s == seq) {
            LeaderHit::Action(action)
        } else if LEADER.iter().any(|&(s, _)| s.starts_with(seq)) {
            LeaderHit::Prefix
        } else {
            LeaderHit::None
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
            "file.switch_project" => self.open_workspace_chooser(),
            "nav.recent_locations" => self.open_location_chooser(),
            "bookmark.toggle" => self.toggle_bookmark(),
            "bookmark.next" => self.bookmark_goto(true),
            "bookmark.prev" => self.bookmark_goto(false),
            "bookmark.list" => self.list_bookmarks(),
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
            a if self.run_edit_action(a) => {}
            a if self.run_motion_action(a) => {}
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
            a if a.starts_with("view.locale:") => self.set_locale_by_code(&a["view.locale:".len()..]),
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
            a if self.open_edit_surface(a) => {}
            "tools.qrcode" => self.open_qrcode(),
            "tools.x11_colors" => self.open_x11_panel(),
            "tools.media_types" => self.open_media_type_panel(),
            "tools.html_chars" => self.open_html_panel(),
            "tools.system_info" => self.open_system_info(),
            "tools.file_info" => self.open_file_info(),
            "tools.text_info" => self.open_text_info(),
            "tools.markdown_preview" => self.open_markdown_preview(),
            "tools.snippets" => self.open_snippets(),
            "tools.contacts" => self.open_contacts(),
            "tools.clock" => {
                self.show_clock = !self.show_clock;
                if self.show_clock {
                    self.clock.selected = 0;
                }
            }
            a if a.starts_with("view.time_zone:") => {
                self.set_time_zone_by_name(&a["view.time_zone:".len()..]);
            }
            "tools.dashboard" => self.open_dashboard(),
            "tools.color_converter" => self.open_color_converter(),
            "tools.calculator" => self.open_calculator(),
            "tools.regex_tester" => self.open_regex_tester(),
            "tools.pomodoro" => self.open_pomodoro(),
            a if self.run_text_tool_action(a) => {}
            other if self.run_view_action(other) => {}
            other if self.run_git_action(other) => {}
            other if self.run_named_action(other) => {}
            other => self.messages.warn(t!("msg.unknown_action", action = other).to_string()),
        }
    }

    /// Dispatch a view/window, command, AI, tab, help, or `vix.*` action.
    /// Returns `true` if `action` was handled. Extracted from
    /// [`App::run_action`] to keep that function within the line limit.
    fn run_view_action(&mut self, action: &str) -> bool {
        match action {
            "tools.run_command" => {
                self.prompt =
                    Some(Prompt::new(PromptKind::RunCommand, t!("prompt.run_command").to_string()));
            }
            "tools.cancel_command" => self.cancel_command(),
            "tools.tasks" => self.open_tasks(),
            "tools.test" => self.run_tests(),
            "tools.test_panel" => self.show_test_panel = !self.show_test_panel,
            "tools.terminal" => self.toggle_terminal(),
            "tools.diff" => self.open_compare_prompt(),
            "tools.palette" => self.open_palette(),
            // The left/right docks are the explorer and message drawers. Both the
            // old action ids and the new dock-named ones route to one method.
            "view.split_vertical" => self.editor.set_split(SplitDir::Vertical),
            "view.split_horizontal" => self.editor.set_split(SplitDir::Horizontal),
            "view.unsplit" => self.editor.unsplit(),
            "view.focus_other_pane" => self.editor.focus_other_pane(),
            "view.line_numbers" | "tools.line_numbers" => self.toggle_editor_line_numbers(),
            "view.whitespace" => self.toggle_editor_whitespace(),
            "view.soft_wrap" => self.toggle_editor_soft_wrap(),
            "view.left_dock" | "view.explorer" => self.toggle_left_dock(),
            "view.right_dock" | "view.messages" => self.toggle_right_dock(),
            "view.status_bar" => self.toggle_status_bar(),
            "view.zen" => self.toggle_zen(),
            "view.breadcrumbs" => {
                self.show_breadcrumbs = !self.show_breadcrumbs;
                self.settings.show_breadcrumbs = self.show_breadcrumbs;
            }
            "view.outline_dock" => self.toggle_outline_dock(),
            "view.trim_on_save" => {
                self.settings.trim_trailing_whitespace = !self.settings.trim_trailing_whitespace;
            }
            "view.final_newline_on_save" => {
                self.settings.ensure_final_newline = !self.settings.ensure_final_newline;
            }
            "view.scrollbar" => self.toggle_scrollbar(),
            "view.spellcheck" => self.toggle_spellcheck(),
            "view.auto_pair" => self.toggle_auto_pair(),
            "view.zoom_in" => self.terminal_zoom(1),
            "view.zoom_out" => self.terminal_zoom(-1),
            "view.zoom_reset" => self.terminal_zoom(0),
            "spell.suggest" => self.open_spell_suggest(),
            "ai.chat" => self.open_ai_panel(),
            "ai.summarize" => self.ai_summarize(),
            "ai.explain" => self.ai_explain(),
            "ai.define" => self.ai_define(),
            "ai.annotate" => self.ai_annotate(),
            "ai.improve" => self.ai_improve(),
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
            _ => return false,
        }
        true
    }

    /// Dispatch a `git.*` action. Returns `true` if `action` was handled.
    /// Extracted from [`App::run_action`] to keep that function within the line
    /// limit.
    fn run_git_action(&mut self, action: &str) -> bool {
        match action {
            "git.changes" => self.open_git_panel(),
            "git.push" => self.git_remote_command("git push"),
            "git.pull_ff" => self.git_remote_command("git pull --ff-only"),
            "git.pull_rebase" => self.git_remote_command("git pull --rebase"),
            "git.pull_merge" => self.git_remote_command("git pull --no-rebase"),
            "git.pull_squash" => self.git_remote_command("git pull --squash"),
            "git.fetch" => self.git_remote_command("git fetch"),
            "git.switch_branch" => self.open_branch_chooser(),
            "git.merge_branch" => self.open_branch_chooser_mode(true),
            "git.init" => self.git_init(),
            "git.new_branch" => self.git_begin_new_branch(),
            "git.log" => self.git_log(),
            "git.log_graph" => self.git_log_graph(),
            "git.log_since_1_day_ago" => self.git_log_since(Some("1-day-ago")),
            "git.log_since_1_week_ago" => self.git_log_since(Some("1-week-ago")),
            "git.log_since_1_month_ago" => self.git_log_since(Some("1-month-ago")),
            "git.status" => self.git_status_to_dock(),
            "git.clone" => self.git_begin_clone(),
            "git.edit_description" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::GitEditDescription,
                    t!("prompt.git_edit_description").to_string(),
                ));
            }
            "git.delete_branch" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::GitDeleteBranch,
                    t!("prompt.git_delete_branch").to_string(),
                ));
            }
            "git.grep" => {
                self.prompt =
                    Some(Prompt::new(PromptKind::GitGrep, t!("prompt.git_grep").to_string()));
            }
            "git.blame" => self.git_blame_line(),
            "git.blame_inline" => self.toggle_inline_blame(),
            "debug.start" => self.start_debugger(),
            "debug.stop" => self.stop_debugger(),
            "debug.toggle_breakpoint" => self.toggle_breakpoint(),
            "debug.continue" => self.dap.continue_(),
            "debug.step_over" => self.dap.step_over(),
            "debug.step_into" => self.dap.step_into(),
            "debug.step_out" => self.dap.step_out(),
            "debug.pause" => self.dap.pause(),
            "debug.panel" => self.show_debug_panel = !self.show_debug_panel,
            "debug.repl" => {
                self.prompt = Some(Prompt::new(PromptKind::DebugRepl, t!("prompt.debug_repl").to_string()));
            }
            "debug.watch" => {
                self.prompt = Some(Prompt::new(PromptKind::DebugWatch, t!("prompt.debug_watch").to_string()));
            }
            "git.revert_hunk" => self.revert_hunk(),
            "git.stage_hunk" => self.stage_hunk(),
            "git.unstage_hunk" => self.unstage_hunk(),
            "git.conflict_ours" => self.resolve_conflict(crate::conflict_tool::Resolution::Ours),
            "git.conflict_theirs" => self.resolve_conflict(crate::conflict_tool::Resolution::Theirs),
            "git.conflict_both" => self.resolve_conflict(crate::conflict_tool::Resolution::Both),
            "git.conflict_next" => self.conflict_next(),
            "git.stash" => self.git_op(crate::git::stash_push, "status.git_stashed"),
            "git.stash_pop" => self.git_op(crate::git::stash_pop, "status.git_stash_popped"),
            "git.amend" => self.git_op(crate::git::commit_amend, "status.git_amended"),
            "git.diff_next" => self.diff_goto(true),
            "git.diff_prev" => self.diff_goto(false),
            _ => return false,
        }
        true
    }

    /// Dispatch a buffer-editing action (`edit.undo`/`redo`/`cut`/`copy`/
    /// `paste`/`toggle_comment`, the `edit.case_*` transforms, and the
    /// whole-line operations). Returns `true` if `action` was handled. Extracted
    /// from [`App::run_action`] to keep that function within the line limit.
    fn run_edit_action(&mut self, action: &str) -> bool {
        match action {
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
                if let Some(t) = self.editor.active_tab_mut()
                    && !t.is_image() {
                        // Comments the cursor line, or every line touched by the
                        // selection; the editor picks the language's token.
                        t.editor.apply(ToggleComment {});
                        t.dirty = true;
                        t.preview = false;
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
            "edit.join_lines" => self.editor.join_lines(),
            "edit.sort_lines" => self.editor.sort_lines(),
            "edit.trim_trailing_whitespace" => self.editor.trim_trailing_whitespace(),
            "edit.remove_duplicate_lines" => self.editor.remove_duplicate_lines(),
            "edit.reverse_lines" => self.editor.reverse_lines(),
            "edit.sort_unique" => self.editor.sort_unique(),
            "edit.shuffle" => self.editor.shuffle_lines(),
            _ => return false,
        }
        true
    }

    /// Dispatch an editor motion, selection, find/search, or navigation action
    /// (`edit.*` motions, `search.*`, `nav.*`, `lsp.*`). Returns `true` if
    /// `action` was handled. Extracted from [`App::run_action`] to keep that
    /// function within the line limit.
    fn run_motion_action(&mut self, action: &str) -> bool {
        match action {
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
            "edit.select_all_occurrences" => {
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.add_all_occurrences();
                }
            }
            "edit.column_select_down" => {
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.column_select(true);
                }
            }
            "edit.column_select_up" => {
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.column_select(false);
                }
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
            "nav.goto_symbol" => self.open_palette_seeded("@"),
            "nav.goto_workspace_symbol" => self.open_palette_seeded("@@"),
            "nav.outline" => self.open_outline(),
            _ => return self.run_lsp_action(action),
        }
        true
    }

    /// Dispatch a language-server action (definition/implementation/references/
    /// formatting/symbols/hover/completion/diagnostics). Returns `true` if
    /// `action` was handled. Extracted to keep [`App::run_motion_action`] within
    /// the line limit.
    fn run_lsp_action(&mut self, action: &str) -> bool {
        match action {
            "nav.goto_definition" => self.goto_definition(),
            "nav.goto_implementation" => self.goto_implementation(),
            "nav.goto_type_definition" => self.goto_type_definition(),
            "lsp.references" => self.find_references(),
            "lsp.format" => self.lsp_format(),
            "lsp.document_symbols" => self.request_document_symbols(),
            "lsp.signature_help" => self.lsp_signature_help(),
            "lsp.workspace_symbols" => {
                if self.active_path().is_some_and(|p| self.lsp.handles(&p)) {
                    self.prompt = Some(Prompt::new(
                        PromptKind::WorkspaceSymbol,
                        t!("prompt.workspace_symbol").to_string(),
                    ));
                } else {
                    self.status = t!("status.lsp_inactive").to_string();
                }
            }
            "lsp.hover" => self.lsp_hover(),
            "lsp.complete" => self.lsp_complete(),
            "lsp.diagnostics" => self.open_diagnostics_panel(),
            "lsp.rename" => self.begin_lsp_rename(),
            "lsp.code_action" => self.request_code_action(),
            "lsp.expand_selection" => self.request_selection_range(true),
            "lsp.shrink_selection" => self.request_selection_range(false),
            "lsp.highlight" => self.request_document_highlight(),
            "lsp.linked_edit" => self.request_linked_editing(),
            "lsp.code_lens" => self.request_code_lens(),
            "view.inlay_hints" => self.toggle_inlay_hints(),
            "editor.fold_toggle" => self.toggle_fold_at_cursor(),
            "editor.fold_all" => {
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.fold_all();
                }
            }
            "editor.unfold_all" => {
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.unfold_all();
                }
            }
            _ => return false,
        }
        true
    }

    /// Dispatch a text-transforming tool action (`tools.insert.*`,
    /// `tools.checksum.*`, `tools.convert.*`, `tools.format.*`). Returns `true`
    /// if `action` was handled. Extracted from [`App::run_action`] to keep that
    /// function within the line limit.
    fn run_text_tool_action(&mut self, action: &str) -> bool {
        match action {
            "tools.insert.uuid.v1" => self.insert_content(&crate::uuid_tool::v1()),
            "tools.insert.uuid.v2" => self.insert_content(&crate::uuid_tool::v2()),
            "tools.insert.uuid.v3" => self.insert_content(&crate::uuid_tool::v3()),
            "tools.insert.uuid.v4" => self.insert_content(&crate::uuid_tool::v4()),
            "tools.insert.uuid.v5" => self.insert_content(&crate::uuid_tool::v5()),
            "tools.insert.uuid.v6" => self.insert_content(&crate::uuid_tool::v6()),
            "tools.insert.uuid.v7" => self.insert_content(&crate::uuid_tool::v7()),
            "tools.insert.uuid.v8" => self.insert_content(&crate::uuid_tool::v8()),
            "tools.insert.zid.128" => self.insert_content(&crate::zid_tool::generate(16)),
            "tools.insert.zid.256" => self.insert_content(&crate::zid_tool::generate(32)),
            "tools.insert.zid.512" => self.insert_content(&crate::zid_tool::generate(64)),
            a if self.insert_markdown(a) => {}
            a if self.insert_html(a) => {}
            a if self.insert_sql(a) => {}
            a if self.insert_latex(a) => {}
            a if self.insert_org(a) => {}
            a if self.insert_marker(a) => {}
            a if self.insert_block(a) => {}
            a if self.org_action(a) => {}
            a if self.insert_dynamic(a) => {}
            "tools.checksum.sha256" => {
                self.transform_selection_or_buffer(crate::checksum_tool::sha256_hex);
            }
            "tools.checksum.sha512" => {
                self.transform_selection_or_buffer(crate::checksum_tool::sha512_hex);
            }
            "tools.checksum.md5" => self.transform_selection_or_buffer(crate::checksum_tool::md5_hex),
            "tools.checksum.crc32" => self.transform_selection_or_buffer(crate::checksum_tool::crc32_hex),
            "tools.convert.base64.encode" => {
                self.transform_selection_or_buffer_try(crate::base64_tool::encode);
            }
            "tools.convert.base64.decode" => {
                self.transform_selection_or_buffer_try(crate::base64_tool::decode);
            }
            "tools.convert.url.encode" => {
                self.transform_selection_or_buffer_try(crate::url_tool::encode);
            }
            "tools.convert.url.decode" => {
                self.transform_selection_or_buffer_try(crate::url_tool::decode);
            }
            "tools.convert.csv.json" => {
                self.transform_selection_or_buffer_try(crate::convert_from_csv_into_json_tool::convert);
            }
            "tools.convert.csv.tsv" => {
                self.transform_selection_or_buffer_try(crate::convert_from_csv_into_tsv_tool::convert);
            }
            "tools.convert.tsv.csv" => {
                self.transform_selection_or_buffer_try(crate::convert_from_tsv_into_csv_tool::convert);
            }
            "tools.convert.tsv.json" => {
                self.transform_selection_or_buffer_try(crate::convert_from_tsv_into_json_tool::convert);
            }
            "tools.convert.json.csv" => {
                self.transform_selection_or_buffer_try(crate::convert_from_json_into_csv_tool::convert);
            }
            "tools.convert.json.tsv" => {
                self.transform_selection_or_buffer_try(crate::convert_from_json_into_tsv_tool::convert);
            }
            "tools.convert.json.yaml" => {
                self.transform_selection_or_buffer_try(crate::convert_from_json_into_yaml_tool::convert);
            }
            "tools.convert.yaml.json" => {
                self.transform_selection_or_buffer_try(crate::convert_from_yaml_into_json_tool::convert);
            }
            "tools.convert.json.toml" => {
                self.transform_selection_or_buffer_try(crate::convert_from_json_into_toml_tool::convert);
            }
            "tools.convert.toml.json" => {
                self.transform_selection_or_buffer_try(crate::convert_from_toml_into_json_tool::convert);
            }
            "tools.convert.markdown.html" => {
                self.transform_selection_or_buffer_try(crate::convert_from_markdown_into_html_tool::convert);
            }
            "tools.convert.html.markdown" => {
                self.transform_selection_or_buffer_try(crate::convert_from_html_into_markdown_tool::convert);
            }
            "tools.convert.unit" => self.open_unit_converter(),
            "tools.convert.jwt" => self.transform_selection_or_buffer_try(crate::jwt_tool::decode),
            "tools.convert.number.dec" => self.transform_selection_or_buffer_try(crate::base_tool::to_dec),
            "tools.convert.number.hex" => self.transform_selection_or_buffer_try(crate::base_tool::to_hex),
            "tools.convert.number.bin" => self.transform_selection_or_buffer_try(crate::base_tool::to_bin),
            "tools.convert.number.oct" => self.transform_selection_or_buffer_try(crate::base_tool::to_oct),
            "tools.format.json_pretty" => {
                self.transform_selection_or_buffer_try(crate::format_tool::json_pretty);
            }
            "tools.format.json_minify" => {
                self.transform_selection_or_buffer_try(crate::format_tool::json_minify);
            }
            "tools.format.yaml" => {
                self.transform_selection_or_buffer_try(crate::format_tool::yaml_format);
            }
            "tools.format.toml" => {
                self.transform_selection_or_buffer_try(crate::format_tool::toml_format);
            }
            _ => return false,
        }
        true
    }

    /// Dispatch a `snake_case` action from the `spec/actions/actions.tsv` catalog.
    /// Returns `true` if the id was handled. Editing actions are applied to the
    /// active tab's editor; app-level ones delegate to existing behavior; a few
    /// mode/macro actions are not yet implemented (they report via the status).
    /// Dispatch a cursor-movement, selection, word, line-motion, or paragraph
    /// catalog action against the active editor. `view_h` is the viewport height
    /// for page-relative motions. Returns `true` if `id` was handled. Extracted
    /// from [`App::run_named_action`] to keep that function within the line
    /// limit.
    fn run_cursor_action(&mut self, id: &str, view_h: usize) -> bool {
        // Editor motion/selection (no buffer change).
        macro_rules! ed {
            ($m:ident) => {{
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.$m();
                }
            }};
        }
        // Editor motion that needs the viewport height.
        macro_rules! edh {
            ($m:ident) => {{
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.$m(view_h);
                }
            }};
        }
        match id {
            // cursor movement
            "cursor_up" => ed!(cursor_up),
            "cursor_down" => ed!(cursor_down),
            "cursor_left" => ed!(cursor_left),
            "cursor_right" => ed!(cursor_right),
            "cursor_start" | "start" => ed!(cursor_start),
            "cursor_end" | "end" => ed!(cursor_end),
            "cursor_page_up" | "page_up" => edh!(page_up),
            "cursor_page_down" | "page_down" => edh!(page_down),
            "half_page_up" => edh!(half_page_up),
            "half_page_down" => edh!(half_page_down),
            "cursor_to_view_top" => ed!(cursor_to_view_top),
            "cursor_to_view_center" => edh!(cursor_to_view_center),
            "cursor_to_view_bottom" => edh!(cursor_to_view_bottom),
            "center" => edh!(center),
            "scroll_up" => ed!(scroll_up),
            "scroll_down" => edh!(scroll_down),
            // selection
            "select_up" => ed!(select_up),
            "select_down" => ed!(select_down),
            "select_left" => ed!(select_left),
            "select_right" => ed!(select_right),
            "select_to_start" => ed!(select_to_start),
            "select_to_end" => ed!(select_to_end),
            "select_page_up" => edh!(select_page_up),
            "select_page_down" => edh!(select_page_down),
            "select_all" => ed!(select_all),
            "select_line" => ed!(select_line),
            "deselect" => ed!(deselect),
            // word / sub-word
            "word_right" => ed!(word_right),
            "word_left" => ed!(word_left),
            "sub_word_right" => ed!(sub_word_right),
            "sub_word_left" => ed!(sub_word_left),
            "select_word_right" => ed!(select_word_right),
            "select_word_left" => ed!(select_word_left),
            "select_sub_word_right" => ed!(select_sub_word_right),
            "select_sub_word_left" => ed!(select_sub_word_left),
            // line motions
            "start_of_line" => ed!(start_of_line),
            "end_of_line" => ed!(end_of_line),
            "start_of_text" => ed!(start_of_text),
            "start_of_text_toggle" => ed!(start_of_text_toggle),
            "select_to_start_of_line" => ed!(select_to_start_of_line),
            "select_to_end_of_line" => ed!(select_to_end_of_line),
            "select_to_start_of_text" => ed!(select_to_start_of_text),
            "select_to_start_of_text_toggle" => ed!(select_to_start_of_text_toggle),
            // paragraph
            "paragraph_next" => ed!(paragraph_next),
            "paragraph_previous" => ed!(paragraph_previous),
            "select_to_paragraph_next" => ed!(select_to_paragraph_next),
            "select_to_paragraph_previous" => ed!(select_to_paragraph_previous),
            _ => return false,
        }
        true
    }

    /// Dispatch a line-operation, navigation, search, file, tab, split, or
    /// app-toggle catalog action, mostly by delegating to [`App::run_action`].
    /// Returns `true` if `id` was handled. Extracted from
    /// [`App::run_named_action`] to keep that function within the line limit.
    fn run_app_action(&mut self, id: &str) -> bool {
        match id {
            "move_lines_up" => self.run_action("edit.move_line_up"),
            "move_lines_down" => self.run_action("edit.move_line_down"),
            "join_lines" => self.run_action("edit.join_lines"),
            "sort_lines" => self.run_action("edit.sort_lines"),
            "trim_trailing_whitespace" => self.run_action("edit.trim_trailing_whitespace"),
            "remove_duplicate_lines" => self.run_action("edit.remove_duplicate_lines"),
            "reverse_lines" => self.run_action("edit.reverse_lines"),
            "sort_unique" => self.run_action("edit.sort_unique"),
            "shuffle" => self.run_action("edit.shuffle"),
            // navigation
            "jump_to_matching_brace" => self.run_action("edit.match_bracket"),
            "jump_line" => self.run_action("nav.goto_line"),
            // search
            "find" | "find_literal" => self.run_action("edit.find"),
            "find_next" => self.run_action("edit.find_next"),
            "find_previous" => self.run_action("edit.find_prev"),
            "unhighlight_search" | "reset_search" => {
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.remove_marks();
                }
            }
            "toggle_highlight_search" => self.toggle_search_highlight(),
            // files
            "save" | "save_all" => self.save(),
            "save_as" => self.run_action("file.save_as"),
            "open_file" => self.run_action("file.open"),
            // tabs
            "add_tab" => self.run_action("file.new"),
            "next_tab" => self.run_action("tab.next"),
            "previous_tab" => self.run_action("tab.prev"),
            "first_tab" => self.editor.active = 0,
            "last_tab" => self.editor.active = self.editor.tabs.len().saturating_sub(1),
            // splits
            "vsplit" => self.run_action("view.split_vertical"),
            "hsplit" => self.run_action("view.split_horizontal"),
            "unsplit" => self.run_action("view.unsplit"),
            "next_split" | "previous_split" => self.run_action("view.focus_other_pane"),
            "first_split" => self.focus_split_pane(0),
            "last_split" => self.focus_split_pane(usize::MAX),
            // toggles / app
            "toggle_help" | "toggle_key_menu" => self.show_help = !self.show_help,
            "toggle_diff_gutter" => self.run_action("view.scrollbar"),
            "escape" => {
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.clear_carets();
                    t.editor.clear_selection();
                }
            }
            "clear_status" | "clear_info" => self.status = String::new(),
            "quit" | "quit_all" | "force_quit" => self.run_action("file.quit"),
            "none" => {}
            _ => return false,
        }
        true
    }

    fn run_named_action(&mut self, id: &str) -> bool {
        let view_h = self.editor_view().height as usize;
        // Editor motion/selection (no buffer change).
        macro_rules! ed {
            ($m:ident) => {{
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.$m();
                }
            }};
        }
        // Editor edit (marks the buffer dirty).
        macro_rules! edm {
            ($m:ident) => {{
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.$m();
                }
                self.mark_active_dirty();
            }};
        }
        match id {
            _ if self.run_cursor_action(id, view_h) => {}
            "delete_word_right" => edm!(delete_word_right),
            "delete_word_left" => edm!(delete_word_left),
            "delete_sub_word_right" => edm!(delete_sub_word_right),
            "delete_sub_word_left" => edm!(delete_sub_word_left),
            // editing
            "insert_newline" => edm!(insert_newline),
            "insert_tab" => edm!(insert_tab),
            "backspace" => edm!(backspace),
            "delete" => edm!(delete),
            "undo" => edm!(undo),
            "redo" => edm!(redo),
            "copy" => ed!(copy),
            "copy_line" => ed!(copy_line),
            "cut" => edm!(cut),
            "cut_line" => edm!(cut_line),
            "paste" | "paste_primary" => edm!(paste),
            "duplicate" => edm!(duplicate),
            "duplicate_line" => edm!(duplicate_line),
            "delete_line" => edm!(delete_line),
            "indent_line" | "indent_selection" => edm!(indent_line),
            "outdent_line" | "outdent_selection" => edm!(outdent_line),
            // multiple cursors
            "spawn_multi_cursor" | "spawn_multi_cursor_select" | "skip_multi_cursor"
            | "skip_multi_cursor_back" => edm!(add_next_occurrence),
            "remove_multi_cursor" | "remove_all_multi_cursors" => ed!(clear_carets),
            _ if self.run_app_action(id) => {}
            // not implemented yet (modes, macros, suspend, autocomplete, …)
            "diff_next" => self.diff_goto(true),
            "diff_previous" => self.diff_goto(false),
            "spawn_multi_cursor_up" => ed!(add_caret_above),
            "spawn_multi_cursor_down" => ed!(add_caret_below),
            "toggle_overwrite_mode" => {
                self.overwrite = !self.overwrite;
                self.status = t!(if self.overwrite { "status.overwrite_on" } else { "status.overwrite_off" }).to_string();
            }
            "toggle_ruler" => {
                self.show_ruler = !self.show_ruler;
                self.status = t!(if self.show_ruler { "status.ruler_on" } else { "status.ruler_off" }).to_string();
            }
            "macro.record" => {
                self.macro_recording = !self.macro_recording;
                if self.macro_recording {
                    self.macro_keys.clear();
                    self.status = t!("status.macro_recording").to_string();
                } else {
                    self.status = t!("status.macro_recorded", count = self.macro_keys.len()).to_string();
                }
            }
            "macro.play" => self.play_macro(),
            "macro.save" => self.begin_save_macro(),
            "macro.play_saved" => self.open_macro_chooser(),
            "autocomplete" => self.autocomplete(true),
            "cycle_autocomplete_back" => self.autocomplete(false),
            "command_mode" => self.open_palette(),
            "shell_mode" => {
                self.prompt =
                    Some(Prompt::new(PromptKind::RunCommand, t!("prompt.run_command").to_string()));
            }
            "suspend" => self.suspend_requested = true,
            _ => return false,
        }
        true
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
                if self.lsp.handles(&p) {
                    let text = self.editor.active_tab().map(Tab::text).unwrap_or_default();
                    self.lsp.did_save(&p, &text);
                }
                self.refresh_git();
            }
            Err(e) => self.messages.error(t!("msg.save_failed", error = e).to_string()),
        }
    }

    /// On-save normalization options derived from the current settings.
    fn save_options(&self) -> crate::editor::SaveOptions {
        let mut opts = crate::editor::SaveOptions {
            trim_trailing_whitespace: self.settings.trim_trailing_whitespace,
            ensure_final_newline: self.settings.ensure_final_newline,
        };
        // Let the active file's .editorconfig override the global on-save rules.
        if self.settings.editorconfig
            && let Some(path) = self.editor.active_tab().and_then(|t| t.path.as_deref())
        {
            let ec = crate::editorconfig::resolve(path);
            if let Some(v) = ec.trim_trailing_whitespace {
                opts.trim_trailing_whitespace = v;
            }
            if let Some(v) = ec.insert_final_newline {
                opts.ensure_final_newline = v;
            }
        }
        opts
    }

    // ----- view toggles ---------------------------------------------------

    /// Toggle the left dock (the file explorer). Revealing it also reveals the
    /// active file in the tree.
    fn toggle_left_dock(&mut self) {
        self.show_explorer = !self.show_explorer;
        self.settings.show_explorer = self.show_explorer;
        if self.show_explorer
            && let Some(p) = self.editor.active_tab().and_then(|t| t.path.clone()) {
                self.explorer.reveal(&p);
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

    /// Toggle zen (focus) mode: hide the explorer, messages, bottom dock, and
    /// status bar for distraction-free editing, restoring them on the next toggle.
    /// The change is runtime-only — it does not overwrite the saved settings.
    fn toggle_zen(&mut self) {
        if let Some((explorer, messages, bottom, status)) = self.zen_saved.take() {
            self.show_explorer = explorer;
            self.show_messages = messages;
            self.show_bottom_dock = bottom;
            self.show_status_bar = status;
        } else {
            self.zen_saved =
                Some((self.show_explorer, self.show_messages, self.show_bottom_dock, self.show_status_bar));
            self.show_explorer = false;
            self.show_messages = false;
            self.show_bottom_dock = false;
            self.show_status_bar = false;
        }
    }

    /// Whether zen (focus) mode is active.
    #[must_use]
    pub fn is_zen(&self) -> bool {
        self.zen_saved.is_some()
    }

    /// The breadcrumb for the active buffer: its file name, then the enclosing
    /// symbol at the cursor (`file ▸ symbol`). Empty when there is no buffer.
    #[must_use]
    pub fn breadcrumb(&self) -> String {
        let Some(tab) = self.editor.active_tab() else {
            return String::new();
        };
        let name = tab
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map_or_else(|| t!("ui.untitled").to_string(), |n| n.to_string_lossy().into_owned());
        if tab.is_image() {
            return name;
        }
        let line = self.editor.cursor_1based().0;
        let symbols = crate::palette::symbols(&tab.text());
        match symbols.iter().rev().find(|s| s.line <= line) {
            Some(sym) => format!("{name}  \u{25b8}  {}", sym.name),
            None => name,
        }
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

    /// Apply a text transform to the selection, or to the whole buffer when
    /// nothing is selected, replacing it with the result. Used by the Convert and
    /// Checksum tools. No-op (with a status) on an image tab or empty input.
    fn transform_selection_or_buffer(&mut self, f: impl Fn(&str) -> String) {
        self.transform_selection_or_buffer_try(|input| Ok(f(input)));
    }

    /// Like [`Self::transform_selection_or_buffer`] but for fallible transforms
    /// (Convert tools that parse their input). On `Err` the buffer is left
    /// untouched and the error is shown in the status line. No-op (with a status)
    /// on an image tab or empty input.
    fn transform_selection_or_buffer_try(
        &mut self,
        f: impl Fn(&str) -> Result<String, String>,
    ) {
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        if tab.is_image() {
            self.status = t!("status.no_selection").into();
            return;
        }
        let target = match tab.editor.get_selection() {
            Some(sel) if !sel.is_empty() => Some((sel.start, sel.end)),
            _ => None,
        };
        let input = match target {
            Some((s, e)) => tab.editor.get_content_slice(s, e),
            None => tab.editor.get_content(),
        };
        if input.is_empty() {
            self.status = t!("status.no_selection").into();
            return;
        }
        let output = match f(&input) {
            Ok(out) => out,
            Err(e) => {
                self.status = t!("status.convert_failed", error = e).to_string();
                return;
            }
        };
        let output_len = output.chars().count();
        if let Some(tab) = self.editor.active_tab_mut() {
            // Place the caret at the end of the replacement so it stays in range
            // even when the new text is shorter than the old (set_content leaves
            // the old cursor untouched, which would otherwise point past the end).
            let (new, caret) = match target {
                None => (output, output_len),
                Some((start, end)) => {
                    let chars: Vec<char> = tab.editor.get_content().chars().collect();
                    let n = chars.len();
                    let start = start.min(n);
                    let end = end.min(n).max(start);
                    let mut out: String = chars[..start].iter().collect();
                    out.push_str(&output);
                    out.extend(&chars[end..]);
                    (out, start + output_len)
                }
            };
            tab.editor.set_content(&new);
            tab.editor.set_selection(None);
            tab.editor.set_cursor(caret);
            tab.dirty = true;
            tab.preview = false;
        }
    }

    /// Dispatch an `org.*` action against the active buffer. Returns `true` if
    /// `action` was an Org command.
    fn org_action(&mut self, action: &str) -> bool {
        match action {
            "org.cycle_visibility" => self.run_action("editor.fold_toggle"),
            "org.promote" => self.org_rewrite_line(crate::org::promote),
            "org.demote" => self.org_rewrite_line(crate::org::demote),
            "org.cycle_todo" => self.org_rewrite_line(crate::org::cycle_todo),
            "org.toggle_checkbox" => self.org_rewrite_line(crate::org::toggle_checkbox),
            "org.move_up" => self.org_move_subtree(crate::org::move_subtree_up),
            "org.move_down" => self.org_move_subtree(crate::org::move_subtree_down),
            "org.export_markdown" => self.org_export(crate::org::to_markdown, "md"),
            "org.export_html" => self.org_export(crate::org::to_html, "html"),
            "org.capture" => self.org_capture(),
            "org.agenda" => self.org_agenda(),
            "org.time_report" => self.org_time_report(),
            _ => return false,
        }
        true
    }

    /// Run an Org transform that rewrites the buffer based on the cursor line
    /// (promote/demote/cycle-todo/toggle-checkbox), keeping the cursor's line.
    fn org_rewrite_line(&mut self, f: fn(&str, usize) -> Option<String>) {
        let Some(tab) = self.editor.active_tab_mut() else { return };
        let line = tab.editor.cursor_line();
        let text = tab.editor.get_content();
        if let Some(new) = f(&text, line) {
            tab.editor.set_content(&new);
            tab.editor.set_cursor_line(line);
            tab.dirty = true;
        } else {
            self.status = t!("status.org_not_headline").to_string();
        }
    }

    /// Run an Org subtree move, following the cursor to the subtree's new line.
    fn org_move_subtree(&mut self, f: fn(&str, usize) -> Option<(String, usize)>) {
        let Some(tab) = self.editor.active_tab_mut() else { return };
        let line = tab.editor.cursor_line();
        let text = tab.editor.get_content();
        if let Some((new, new_line)) = f(&text, line) {
            tab.editor.set_content(&new);
            tab.editor.set_cursor_line(new_line);
            tab.dirty = true;
        } else {
            self.status = t!("status.org_no_sibling").to_string();
        }
    }

    /// Export the active buffer with `f` into a new untitled tab named with `ext`.
    fn org_export(&mut self, f: fn(&str) -> String, ext: &str) {
        let Some(text) = self.editor.active_tab().map(crate::editor::Tab::text) else { return };
        let converted = f(&text);
        self.editor.new_tab_with_content(&converted);
        self.status = t!("status.org_exported", ext = ext).to_string();
    }

    /// Open the Org capture dialog: a single-line prompt whose text is inserted as
    /// a `* TODO` headline at the cursor.
    fn org_capture(&mut self) {
        self.prompt = Some(Prompt::new(PromptKind::OrgCapture, t!("prompt.org_capture").to_string()));
    }

    /// Compile an agenda from every `.org` file in the project into a new tab.
    fn org_agenda(&mut self) {
        let mut files: Vec<(String, String)> = Vec::new();
        for path in &self.file_index {
            if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("org"))
                && let Ok(content) = std::fs::read_to_string(path)
            {
                let name = path.strip_prefix(&self.root).unwrap_or(path).to_string_lossy().into_owned();
                files.push((name, content));
            }
        }
        let agenda = crate::org::agenda(&files);
        self.editor.new_tab_with_content(&agenda);
        self.status = t!("status.org_agenda", count = files.len()).to_string();
    }

    /// Build a clock-time report from the active buffer into a new tab.
    fn org_time_report(&mut self) {
        let Some(text) = self.editor.active_tab().map(crate::editor::Tab::text) else { return };
        let report = crate::org::time_report(&text);
        self.editor.new_tab_with_content(&report);
        self.status = t!("status.org_time_report").to_string();
    }

    /// Insert generator output (a UUID, ZID, …) at the cursor in the active
    /// editor, reporting it in the status line. No-op when no buffer is editable.
    fn insert_content(&mut self, text: &str) {
        let area = self.layout.editor;
        if self.editor.insert_str(text, area) {
            self.status = t!("status.generated", text = text).to_string();
        }
    }

    /// Insert a Markdown snippet for a `tools.insert.markdown.*` action at the
    /// cursor. Returns `true` if `action` was a known Markdown snippet.
    fn insert_markdown(&mut self, action: &str) -> bool {
        let snippet = match action {
            "tools.insert.markdown.headline1" => "# Headline 1\n\n",
            "tools.insert.markdown.headline2" => "## Headline 2\n\n",
            "tools.insert.markdown.headline3" => "### Headline 3\n\n",
            "tools.insert.markdown.link" => "[Example](https://www.example.com)",
            "tools.insert.markdown.list" => "- Item\n- Item\n- Item\n\n",
            "tools.insert.markdown.table" => {
                "| x | x | x |\n|---|---|---|\n| x | x | x |\n| x | x | x |\n\n"
            }
            "tools.insert.markdown.todos" => "- [ ] Todo\n- [ ] Todo\n- [ ] Todo\n\n",
            _ => return false,
        };
        self.insert_content(snippet);
        true
    }

    /// Insert an HTML snippet for a `tools.insert.html.*` action at the cursor.
    /// Returns `true` if `action` was a known HTML snippet.
    fn insert_html(&mut self, action: &str) -> bool {
        let snippet = match action {
            "tools.insert.html.headline1" => "<h1>Headline</h1>\n\n",
            "tools.insert.html.headline2" => "<h2>Headline</h2>\n\n",
            "tools.insert.html.headline3" => "<h3>Headline</h3>\n\n",
            "tools.insert.html.link" => "<a href=\"https://www.example.com\">Example</a>",
            "tools.insert.html.list" => {
                "<ul>\n  <li>Item</li>\n  <li>Item</li>\n  <li>Item</li>\n</ul>\n\n"
            }
            "tools.insert.html.table" => concat!(
                "<table>\n",
                "  <thead>\n",
                "    <tr><th>x</th><th>x</th><th>x</th></tr>\n",
                "  </thead>\n",
                "  <tbody>\n",
                "    <tr><td>x</td><td>x</td><td>x</td></tr>\n",
                "    <tr><td>x</td><td>x</td><td>x</td></tr>\n",
                "  </tbody>\n",
                "  <tfoot>\n",
                "    <tr><th>x</th><th>x</th><th>x</th></tr>\n",
                "  </tfoot>\n",
                "</table>\n\n",
            ),
            _ => return false,
        };
        self.insert_content(snippet);
        true
    }

    /// Insert a SQL (`PostgreSQL`) snippet for a `tools.insert.sql.*` action at the
    /// cursor. Returns `true` if `action` was a known SQL snippet.
    fn insert_sql(&mut self, action: &str) -> bool {
        let snippet = match action {
            "tools.insert.sql.alter_role" => {
                "-- Enable a user to create a database.\nALTER ROLE alice WITH CREATEDB;\n"
            }
            "tools.insert.sql.create_extension" => SQL_CREATE_EXTENSION,
            "tools.insert.sql.create_function" => concat!(
                "CREATE FUNCTION updated_at()\n",
                "RETURNS TRIGGER AS $$\n",
                "BEGIN\n",
                "    NEW.updated_at = NOW();\n",
                "    RETURN NEW;\n",
                "END;\n",
                "$$ LANGUAGE plpgsql;\n",
            ),
            "tools.insert.sql.create_user" => {
                "CREATE USER alice\nWITH LOGIN\nENCRYPTED PASSWORD 'secret';\n"
            }
            "tools.insert.sql.grant_create" => concat!(
                "-- Enable a user to create new tables, views, etc. inside the public schema.\n",
                "GRANT CREATE ON SCHEMA public TO alice;\n",
            ),
            "tools.insert.sql.grant_usage" => concat!(
                "-- Enable a user to see and use objects in the public schema.\n",
                "GRANT USAGE ON SCHEMA public TO alice;\n",
            ),
            "tools.insert.sql.create_table" => SQL_CREATE_TABLE,
            _ => return false,
        };
        self.insert_content(snippet);
        true
    }

    /// Insert an Org/LaTeX markup snippet for a `tools.insert.latex.*` action at
    /// the cursor. Returns `true` if `action` was a known snippet.
    fn insert_latex(&mut self, action: &str) -> bool {
        let snippet = match action {
            "tools.insert.latex.headline" => "* Headline\n",
            "tools.insert.latex.subheadline" => "** Subheadline\n",
            "tools.insert.latex.link" => "[[https://org.mode][Org]]",
            "tools.insert.latex.bold" => "*hello*",
            "tools.insert.latex.italic" => "/hello/",
            "tools.insert.latex.underline" => "_hello_",
            "tools.insert.latex.table" => {
                "| x | x | x |\n|---|---|---|\n| x | x | x |\n| x | x | x |\n"
            }
            "tools.insert.latex.deadline" => "DEADLINE: <YYYY-MM-DD Day>\n",
            "tools.insert.latex.scheduled" => "SCHEDULED: <YYYY-MM-DD Day>\n",
            "tools.insert.latex.time_range" => {
                "<2004-08-23 Mon 10:00-11:00>--<2004-08-26 Thu 10:00-11:00>"
            }
            "tools.insert.latex.timestamp" => "<2006-11-02 Thu 10:00-12:00>",
            "tools.insert.latex.timestamp_repeater" => "<2006-11-02 Thu 10:00-12:00 +1w>",
            "tools.insert.latex.quote" => concat!(
                "#+BEGIN_QUOTE\n",
                "Everything should be made\n",
                "as simple as possible,\n",
                "but not any simpler.\n",
                "---Albert Einstein\n",
                "#+END_QUOTE\n",
            ),
            "tools.insert.latex.verse" => concat!(
                "#+BEGIN_VERSE\n",
                "I write, erase, rewrite\n",
                "Erase again, and then\n",
                "A poppy blooms.\n",
                "---Katsushika Hokusai\n",
                "#+END_VERSE\n",
            ),
            "tools.insert.latex.center" => concat!(
                "#+BEGIN_CENTER\n",
                "Nature is an infinite sphere\n",
                "of which the center is everywhere\n",
                "and the circumference nowhere.\n",
                "--- Blaise Pascal\n",
                "#+END_CENTER\n",
            ),
            "tools.insert.latex.drawer" => concat!(
                ":DRAWERNAME:\n",
                "This is inside the drawer.\n",
                ":END:\n",
            ),
            _ => return false,
        };
        self.insert_content(snippet);
        true
    }

    /// Insert an Org-mode snippet for a `tools.insert.org.*` action at the cursor.
    /// Returns `true` if `action` was a known Org snippet.
    fn insert_org(&mut self, action: &str) -> bool {
        let snippet = match action {
            "tools.insert.org.title" => "#+title: Hello World\n",
            "tools.insert.org.author" => "#+author: Alice Adams\n",
            "tools.insert.org.headline" => "* Headline\n",
            "tools.insert.org.subheadline" => "** Subheadline\n",
            "tools.insert.org.link" => "[[https://org.mode][Org]]",
            "tools.insert.org.image" => "[[https://example.com]]",
            "tools.insert.org.list" => "- Alfa\n- Bravo\n- Charlie\n",
            "tools.insert.org.ordered_list" => "1. Alfa\n2. Bravo\n3. Charlie\n",
            "tools.insert.org.check_list" => concat!(
                "- [ ] Alfa work ready to do\n",
                "- [-] Bravo work in progress\n",
                "- [x] Charlie work complete\n",
            ),
            "tools.insert.org.table" => {
                "| x | x | x |\n|---|---|---|\n| x | x | x |\n| x | x | x |\n"
            }
            "tools.insert.org.todo" => "**** TODO A todo item.\n",
            "tools.insert.org.done" => "**** DONE A todo item that has been done.\n",
            "tools.insert.org.deadline" => "DEADLINE: <YYYY-MM-DD Day>\n",
            "tools.insert.org.scheduled" => "SCHEDULED: <YYYY-MM-DD Day>\n",
            "tools.insert.org.time_range" => {
                "<2004-08-23 Mon 10:00-11:00>--<2004-08-26 Thu 10:00-11:00>"
            }
            "tools.insert.org.timestamp" => "<2006-11-02 Thu 10:00-12:00>",
            "tools.insert.org.timestamp_repeater" => "<2006-11-02 Thu 10:00-12:00 +1w>",
            "tools.insert.org.drawer" => ":DRAWERNAME:\nThis is inside the drawer.\n:END:\n",
            _ => return false,
        };
        self.insert_content(snippet);
        true
    }

    /// Toggle an Org inline emphasis marker for a `tools.insert.marker.*` action.
    /// Returns `true` if `action` was a known marker.
    fn insert_marker(&mut self, action: &str) -> bool {
        let ch = match action {
            "tools.insert.marker.bold" => "*",
            "tools.insert.marker.italic" => "/",
            "tools.insert.marker.underline" => "_",
            "tools.insert.marker.strikethrough" => "+",
            "tools.insert.marker.code" => "~",
            "tools.insert.marker.verbatim" => "=",
            _ => return false,
        };
        self.toggle_wrap(ch, ch);
        true
    }

    /// Toggle an Org `#+BEGIN_…`/`#+END_…` block for a `tools.insert.block.*`
    /// action. Returns `true` if `action` was a known block.
    fn insert_block(&mut self, action: &str) -> bool {
        let name = match action {
            "tools.insert.block.comment" => "COMMENT",
            "tools.insert.block.center" => "CENTER",
            "tools.insert.block.quote" => "QUOTE",
            "tools.insert.block.verse" => "VERSE",
            _ => return false,
        };
        self.toggle_wrap(&format!("#+BEGIN_{name}\n"), &format!("\n#+END_{name}"));
        true
    }

    /// Toggle `prefix`/`suffix` around the active selection (a conventional wrap;
    /// see [`crate::affix::toggle`]). With no selection, insert the empty pair and
    /// leave the cursor between the two halves.
    fn toggle_wrap(&mut self, prefix: &str, suffix: &str) {
        let area = self.layout.editor;
        let selection = self.editor.active_tab_mut().and_then(|t| t.editor.get_selection_text());
        match selection {
            Some(sel) if !sel.is_empty() => {
                let wrapped = crate::affix::toggle(&sel, prefix, suffix);
                self.editor.insert_str(&wrapped, area);
            }
            _ => {
                self.editor.insert_str(&format!("{prefix}{suffix}"), area);
                if let Some(t) = self.editor.active_tab_mut() {
                    let cursor = t.editor.get_cursor();
                    t.editor.set_cursor(cursor.saturating_sub(suffix.chars().count()));
                }
            }
        }
    }

    /// Insert dynamically-generated content (Lorem ipsum, date/time presets) for
    /// a `tools.insert.*` action. Returns `true` if `action` was handled.
    fn insert_dynamic(&mut self, action: &str) -> bool {
        let text = match action {
            "tools.insert.lorem.words" => crate::lorem::words(8),
            "tools.insert.lorem.sentence" => crate::lorem::sentence(),
            "tools.insert.lorem.paragraph" => format!("{}\n\n", crate::lorem::paragraph()),
            "tools.insert.datetime.iso8601" => crate::clock::iso8601(&crate::clock::now_local()),
            "tools.insert.datetime.rfc3339" => crate::clock::rfc3339(&crate::clock::now_local()),
            "tools.insert.datetime.epoch" => crate::clock::epoch_seconds(&crate::clock::now_local()),
            _ => return false,
        };
        self.insert_content(&text);
        true
    }

    // ----- Color Converter ------------------------------------------------

    /// Open the Color Converter dialog, seeding it from the selection when that
    /// text parses as a HEX/RGB/HSL color.
    fn open_color_converter(&mut self) {
        let mut conv = crate::color_converter_tool::Converter::new();
        if let Some(sel) = self.editor.active_tab_mut().and_then(|t| t.editor.get_selection_text()) {
            let s = sel.trim();
            let color = crate::color_converter_tool::Color::from_hex(s)
                .or_else(|| crate::color_converter_tool::Color::from_rgb(s))
                .or_else(|| crate::color_converter_tool::Color::from_hsl(s));
            if let Some(c) = color {
                conv.set_color(c);
            }
        }
        self.color_converter = Some(conv);
    }

    /// Handle a key for the Color Converter dialog: type into the focused field
    /// (live-updating the others), Tab/arrows to switch fields, Enter to insert
    /// the focused value into the editor, Esc to close.
    fn color_converter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.color_converter = None,
            KeyCode::Tab | KeyCode::Down => {
                if let Some(c) = self.color_converter.as_mut() {
                    c.focus_next();
                }
            }
            KeyCode::BackTab | KeyCode::Up => {
                if let Some(c) = self.color_converter.as_mut() {
                    c.focus_prev();
                }
            }
            KeyCode::Backspace => {
                if let Some(c) = self.color_converter.as_mut() {
                    c.backspace();
                }
            }
            KeyCode::Enter => self.insert_color_value(),
            KeyCode::Char(ch) if !Self::ctrl(&key) && !Self::alt(&key) => {
                if let Some(c) = self.color_converter.as_mut() {
                    c.push(ch);
                }
            }
            _ => {}
        }
    }

    /// Insert the focused field's value into the editor and close the dialog.
    fn insert_color_value(&mut self) {
        let Some(value) = self.color_converter.as_ref().map(|c| c.current().to_string()) else {
            return;
        };
        if value.is_empty() {
            self.color_converter = None;
            return;
        }
        let area = self.layout.editor;
        self.editor.insert_str(&value, area);
        self.status = t!("status.generated", text = value).to_string();
        self.color_converter = None;
    }

    // ----- Unit Converter -------------------------------------------------

    /// Open the Unit Converter dialog (seeded with `1 m → km`).
    fn open_unit_converter(&mut self) {
        self.unit_converter = Some(crate::unit_converter_tool::Converter::new());
    }

    /// Handle a key for the Unit Converter dialog: type a number into the value
    /// field, Tab/Up/Down to switch field, Left/Right to cycle the focused unit
    /// selector, Enter to insert the result, Esc to close.
    fn unit_converter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.unit_converter = None,
            KeyCode::Tab | KeyCode::Down => {
                if let Some(c) = self.unit_converter.as_mut() {
                    c.focus_next();
                }
            }
            KeyCode::BackTab | KeyCode::Up => {
                if let Some(c) = self.unit_converter.as_mut() {
                    c.focus_prev();
                }
            }
            KeyCode::Left => {
                if let Some(c) = self.unit_converter.as_mut() {
                    c.cycle(-1);
                }
            }
            KeyCode::Right => {
                if let Some(c) = self.unit_converter.as_mut() {
                    c.cycle(1);
                }
            }
            KeyCode::Backspace => {
                if let Some(c) = self.unit_converter.as_mut() {
                    c.backspace();
                }
            }
            KeyCode::Enter => self.insert_unit_value(),
            KeyCode::Char(ch) if !Self::ctrl(&key) && !Self::alt(&key) => {
                if let Some(c) = self.unit_converter.as_mut() {
                    c.push(ch);
                }
            }
            _ => {}
        }
    }

    /// Insert the converted `<value> <unit>` into the editor and close the dialog.
    fn insert_unit_value(&mut self) {
        let Some(text) = self.unit_converter.as_ref().map(crate::unit_converter_tool::Converter::insert_text) else {
            return;
        };
        if !text.is_empty() {
            let area = self.layout.editor;
            self.editor.insert_str(&text, area);
            self.status = t!("status.generated", text = text).to_string();
        }
        self.unit_converter = None;
    }

    // ----- Calculator -----------------------------------------------------

    /// Open the Calculator dialog, seeding the formula from the selection.
    fn open_calculator(&mut self) {
        let mut calc = crate::calculator_tool::Calculator::new();
        if let Some(sel) = self.editor.active_tab_mut().and_then(|t| t.editor.get_selection_text()) {
            calc.input = sel.trim().to_string();
        }
        self.calculator = Some(calc);
    }

    /// Handle a key for the Calculator dialog: type the formula, Enter runs it
    /// (or, with the Insert button focused, inserts the result), Tab cycles the
    /// Input/Run/Insert controls, Esc closes.
    fn calculator_key(&mut self, key: KeyEvent) {
        use crate::calculator_tool::Focus;
        match key.code {
            KeyCode::Esc => self.calculator = None,
            KeyCode::Tab => {
                if let Some(c) = self.calculator.as_mut() {
                    c.focus_next();
                }
            }
            KeyCode::BackTab => {
                if let Some(c) = self.calculator.as_mut() {
                    c.focus_prev();
                }
            }
            KeyCode::Enter => {
                let focus = self.calculator.as_ref().map(|c| c.focus);
                match focus {
                    Some(Focus::Insert) => self.insert_calculator_result(),
                    Some(_) => {
                        if let Some(c) = self.calculator.as_mut() {
                            c.run();
                        }
                    }
                    None => {}
                }
            }
            KeyCode::Backspace => {
                if let Some(c) = self.calculator.as_mut() {
                    c.backspace();
                }
            }
            KeyCode::Char(ch) if !Self::ctrl(&key) && !Self::alt(&key) => {
                if let Some(c) = self.calculator.as_mut()
                    && c.focus == Focus::Input {
                        c.push(ch);
                    }
            }
            _ => {}
        }
    }

    /// Insert the Calculator's current result into the editor and close it.
    fn insert_calculator_result(&mut self) {
        let result = self.calculator.as_ref().and_then(|c| c.result().map(str::to_string));
        if let Some(value) = result {
            let area = self.layout.editor;
            self.editor.insert_str(&value, area);
            self.status = t!("status.generated", text = value).to_string();
            self.calculator = None;
        }
    }

    // ----- Regex tester ---------------------------------------------------

    /// Open the regex tester, seeding the subject from the selection.
    fn open_regex_tester(&mut self) {
        let subject = self
            .editor
            .active_tab_mut()
            .and_then(|t| t.editor.get_selection_text())
            .unwrap_or_default();
        self.regex_tester = Some(crate::regex_tool::Tester::new(subject));
    }

    /// Handle a key for the regex tester: type into the focused field, Tab
    /// switches pattern/subject, Esc closes.
    fn regex_tester_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.regex_tester = None,
            KeyCode::Tab => {
                if let Some(t) = self.regex_tester.as_mut() {
                    t.toggle_field();
                }
            }
            KeyCode::Backspace => {
                if let Some(t) = self.regex_tester.as_mut() {
                    t.backspace();
                }
            }
            KeyCode::Char(c) if !Self::ctrl(&key) && !Self::alt(&key) => {
                if let Some(t) = self.regex_tester.as_mut() {
                    t.push(c);
                }
            }
            _ => {}
        }
    }

    /// Toggle the fold whose range starts at the cursor's line (LSP-provided
    /// ranges). Reports when there is nothing foldable there.
    fn toggle_fold_at_cursor(&mut self) {
        if let Some(t) = self.editor.active_tab_mut() {
            let line = {
                let code = t.editor.code_ref();
                code.char_to_line(t.editor.get_cursor())
            };
            if t.editor.toggle_fold(line) {
                return;
            }
        }
        self.status = t!("status.no_fold").to_string();
    }

    // ----- Inlay hints ----------------------------------------------------

    /// Request inlay hints for the whole document `path` (when display is on).
    fn request_inlay_hints(&mut self, path: &Path) {
        if !self.show_inlay_hints || !self.lsp.handles(path) {
            return;
        }
        let lines = self
            .editor
            .active_tab()
            .map_or(0, |t| t.editor.code_ref().len_lines());
        let end = u32::try_from(lines).unwrap_or(u32::MAX);
        self.lsp.request_inlay_hint(path, (0, 0), (end, 0));
    }

    /// Store inlay hints on the active buffer, converting each LSP `character`
    /// (encoding units) to a char column within its line.
    fn apply_inlay_hints(&mut self, hints: &[(u32, u32, String)]) {
        if !self.show_inlay_hints {
            return;
        }
        let Some(path) = self.active_path() else { return };
        let enc = self.lsp.encoding_for(&path);
        let Some(t) = self.editor.active_tab_mut() else { return };
        let converted: Vec<(usize, usize, String)> = {
            let code = t.editor.code_ref();
            hints
                .iter()
                .filter_map(|&(line, character, ref label)| {
                    let line_idx = line as usize;
                    if line_idx >= code.len_lines() {
                        return None;
                    }
                    let abs = lsp_pos_to_char(code, line, character, enc);
                    let col = abs.saturating_sub(code.line_to_char(line_idx));
                    Some((line_idx, col, label.clone()))
                })
                .collect()
        };
        t.editor.set_inlay_hints(converted);
    }

    /// Toggle inlay-hint display: clear them when turning off, refetch when on.
    fn toggle_inlay_hints(&mut self) {
        self.show_inlay_hints = !self.show_inlay_hints;
        if self.show_inlay_hints {
            if let Some(path) = self.active_path() {
                self.request_inlay_hints(&path);
            }
            self.status = t!("status.inlay_on").to_string();
        } else {
            if let Some(t) = self.editor.active_tab_mut() {
                t.editor.set_inlay_hints(Vec::new());
            }
            self.status = t!("status.inlay_off").to_string();
        }
    }

    // ----- Document highlight ---------------------------------------------

    /// Request the occurrences of the symbol under the cursor to highlight (LSP).
    fn request_document_highlight(&mut self) {
        if let Some(path) = self.active_path()
            && self.lsp.handles(&path)
        {
            let (line, character) = self.cursor_lsp_position(&path);
            self.lsp.request_document_highlight(&path, line, character);
        } else {
            self.status = t!("status.lsp_inactive").to_string();
        }
    }

    /// Highlight the document-highlight occurrences in the active buffer (reuses
    /// the search-mark layer).
    fn apply_document_highlights(&mut self, ranges: &[crate::lsp_core::Range]) {
        let Some(path) = self.active_path() else { return };
        let enc = self.lsp.encoding_for(&path);
        let Some(t) = self.editor.active_tab_mut() else { return };
        let marks: Vec<(usize, usize, &str)> = {
            let code = t.editor.code_ref();
            ranges
                .iter()
                .map(|r| {
                    let s = lsp_pos_to_char(code, r.start.line, r.start.character, enc);
                    let e = lsp_pos_to_char(code, r.end.line, r.end.character, enc);
                    (s.min(e), s.max(e), SEARCH_MARK)
                })
                .collect()
        };
        t.editor.set_marks(marks);
        self.status = t!("status.highlights_n", n = ranges.len()).to_string();
    }

    // ----- Code lens ------------------------------------------------------

    /// Request the code lenses for the active file (LSP).
    fn request_code_lens(&mut self) {
        if let Some(path) = self.active_path()
            && self.lsp.handles(&path)
        {
            self.lsp.request_code_lens(&path);
        } else {
            self.status = t!("status.lsp_inactive").to_string();
        }
    }

    /// Handle a key for the code-lens chooser: Up/Down move, Enter runs, Esc
    /// closes.
    fn code_lens_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.code_lens = None,
            KeyCode::Up => {
                if let Some(m) = self.code_lens.as_mut() {
                    m.selected = m.selected.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if let Some(m) = self.code_lens.as_mut() {
                    m.selected = (m.selected + 1).min(m.lenses.len().saturating_sub(1));
                }
            }
            KeyCode::Enter => self.run_selected_lens(),
            _ => {}
        }
    }

    /// Execute the highlighted code lens's command (`workspace/executeCommand`).
    fn run_selected_lens(&mut self) {
        let Some(menu) = self.code_lens.take() else { return };
        let Some((_, title, command, args)) = menu.lenses.into_iter().nth(menu.selected) else { return };
        if let Some(path) = self.active_path() {
            self.lsp.execute_command(&path, &command, &args);
            self.status = t!("status.lens_run", title = title).to_string();
        }
    }

    // ----- Selection range (expand / shrink) ------------------------------

    /// Request selection ranges at the cursor; `expand` chooses the direction the
    /// response is applied in.
    fn request_selection_range(&mut self, expand: bool) {
        let Some(path) = self.active_path() else {
            self.status = t!("status.lsp_inactive").to_string();
            return;
        };
        if !self.lsp.handles(&path) {
            self.status = t!("status.lsp_inactive").to_string();
            return;
        }
        let (line, character) = self.cursor_lsp_position(&path);
        self.expand_selection_dir = Some(expand);
        self.lsp.request_selection_range(&path, line, character);
    }

    /// Apply a selection-range chain: expand to the smallest range strictly
    /// larger than the current selection, or shrink to the largest range strictly
    /// inside it.
    fn apply_selection_range(&mut self, ranges: &[crate::lsp_core::Range]) {
        let expand = self.expand_selection_dir.take().unwrap_or(true);
        let Some(path) = self.active_path() else { return };
        let enc = self.lsp.encoding_for(&path);
        let Some(t) = self.editor.active_tab_mut() else { return };
        let (cur_s, cur_e) = {
            let sel = t.editor.get_selection();
            let cursor = t.editor.get_cursor();
            sel.map_or((cursor, cursor), |s| (s.start.min(s.end), s.start.max(s.end)))
        };
        // Resolve the chain to char offsets.
        let resolved: Vec<(usize, usize)> = {
            let code = t.editor.code_ref();
            ranges
                .iter()
                .map(|r| {
                    let s = lsp_pos_to_char(code, r.start.line, r.start.character, enc);
                    let e = lsp_pos_to_char(code, r.end.line, r.end.character, enc);
                    (s.min(e), s.max(e))
                })
                .collect()
        };
        let target = if expand {
            // Smallest range that strictly contains the current selection.
            resolved
                .iter()
                .filter(|(s, e)| *s <= cur_s && *e >= cur_e && (*s < cur_s || *e > cur_e))
                .min_by_key(|(s, e)| e - s)
        } else {
            // Largest range strictly inside the current selection.
            resolved
                .iter()
                .filter(|(s, e)| *s >= cur_s && *e <= cur_e && (*s > cur_s || *e < cur_e))
                .max_by_key(|(s, e)| e - s)
        };
        if let Some(&(s, e)) = target {
            t.editor.set_selection(Some(crate::editor_core::selection::Selection { start: s, end: e }));
            t.editor.set_cursor(e);
        }
    }

    // ----- Code actions ---------------------------------------------------

    /// Request code actions for the selection (or cursor line), passing the
    /// overlapping diagnostics in the request context.
    fn request_code_action(&mut self) {
        let Some(path) = self.active_path() else {
            self.status = t!("status.lsp_inactive").to_string();
            return;
        };
        if !self.lsp.handles(&path) {
            self.status = t!("status.lsp_inactive").to_string();
            return;
        }
        let enc = self.lsp.encoding_for(&path);
        // Range: the selection if any, else the whole cursor line.
        let (start, end) = {
            let Some(t) = self.editor.active_tab_mut() else { return };
            let sel = t.editor.get_selection().filter(|s| !s.is_empty());
            let code = t.editor.code_ref();
            if let Some(s) = sel {
                (
                    char_to_lsp_pos(code, s.start.min(s.end), enc),
                    char_to_lsp_pos(code, s.start.max(s.end), enc),
                )
            } else {
                let cur = t.editor.get_cursor();
                let line = code.char_to_line(cur);
                let line_start = code.line_to_char(line);
                let line_end = line_start + code.line_len(line);
                (char_to_lsp_pos(code, line_start, enc), char_to_lsp_pos(code, line_end, enc))
            }
        };
        // Reconstruct minimal LSP diagnostic objects overlapping the range.
        let diags: Vec<serde_json::Value> = self
            .lsp
            .diagnostics_for(&path)
            .iter()
            .filter(|d| d.range.start.line <= end.0 && d.range.end.line >= start.0)
            .map(|d| {
                serde_json::json!({
                    "range": {
                        "start": { "line": d.range.start.line, "character": d.range.start.character },
                        "end": { "line": d.range.end.line, "character": d.range.end.character }
                    },
                    "severity": severity_number(d.severity),
                    "message": d.message,
                })
            })
            .collect();
        self.lsp.request_code_action(&path, start, end, &serde_json::Value::Array(diags));
    }

    /// Handle a key for the code-action chooser: Up/Down move, Enter applies,
    /// Esc closes.
    fn code_action_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.code_actions = None,
            KeyCode::Up => {
                if let Some(m) = self.code_actions.as_mut() {
                    m.selected = m.selected.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if let Some(m) = self.code_actions.as_mut() {
                    m.selected = (m.selected + 1).min(m.actions.len().saturating_sub(1));
                }
            }
            KeyCode::Enter => self.apply_selected_code_action(),
            _ => {}
        }
    }

    /// Apply the highlighted code action's workspace edit and close the chooser.
    fn apply_selected_code_action(&mut self) {
        let Some(menu) = self.code_actions.take() else { return };
        if let Some((title, edit)) = menu.actions.into_iter().nth(menu.selected) {
            if edit.is_empty() {
                self.status = t!("status.code_action_no_edit", title = title).to_string();
            } else {
                self.apply_workspace_edit(&edit);
            }
        }
    }

    // ----- Pomodoro -------------------------------------------------------

    /// Open the Pomodoro dialog. Reveals an already-running timer if there is
    /// one; otherwise starts a fresh idle timer at the default 25 minutes.
    fn open_pomodoro(&mut self) {
        if self.pomodoro.is_none() {
            self.pomodoro = Some(crate::pomodoro_tool::Timer::new());
            self.pomodoro_last_tick = None;
        }
        self.pomodoro_open = true;
    }

    /// Run the dialog's primary button: Start while idle (which closes the dialog
    /// and lets the countdown run in the background), or Stop/Cancel while
    /// running (which resets to idle and keeps the dialog open).
    fn pomodoro_primary(&mut self) {
        use crate::pomodoro_tool::Phase;
        match self.pomodoro.as_ref().map(|t| t.phase) {
            Some(Phase::Idle) => {
                self.pomodoro_last_tick = Some(std::time::Instant::now());
                if let Some(t) = self.pomodoro.as_mut() {
                    t.start();
                }
                self.pomodoro_open = false; // run in the background
            }
            Some(_) => {
                if let Some(t) = self.pomodoro.as_mut() {
                    t.stop();
                }
            }
            None => {}
        }
    }

    /// Whether a Pomodoro countdown is currently running (so the event loop
    /// ticks faster to keep the display current).
    #[must_use]
    pub fn pomodoro_running(&self) -> bool {
        self.pomodoro.as_ref().is_some_and(crate::pomodoro_tool::Timer::is_running)
    }

    /// Advance a running Pomodoro countdown by the real time elapsed since the
    /// last tick, performing phase transitions. Called once per event-loop pass.
    pub fn poll_pomodoro(&mut self) {
        use crate::pomodoro_tool::Tick;
        if !self.pomodoro_running() {
            self.pomodoro_last_tick = None;
            return;
        }
        let now = std::time::Instant::now();
        let last = *self.pomodoro_last_tick.get_or_insert(now);
        let secs = now.duration_since(last).as_secs();
        if secs == 0 {
            return;
        }
        // Keep the fractional remainder so the countdown stays accurate over time.
        self.pomodoro_last_tick = Some(last + std::time::Duration::from_secs(secs));
        if let Some(timer) = self.pomodoro.as_mut() {
            match timer.tick(secs) {
                Tick::BreakStarted => {
                    self.status = t!("status.pomodoro_break").to_string();
                    self.pomodoro_open = true; // surface the break alert
                }
                Tick::Finished => {
                    self.status = t!("status.pomodoro_done").to_string();
                    self.pomodoro = None;
                    self.pomodoro_open = false;
                }
                Tick::None => {}
            }
        }
    }

    /// Handle a key for the Pomodoro dialog. While idle, ↑/↓ adjust the work
    /// length and Enter starts (closing the dialog); while running, Enter stops;
    /// during the break, Enter cancels. Esc closes the dialog (cancelling a run).
    fn pomodoro_key(&mut self, key: KeyEvent) {
        use crate::pomodoro_tool::Phase;
        match key.code {
            KeyCode::Up | KeyCode::Right => {
                if let Some(t) = self.pomodoro.as_mut() {
                    t.adjust_minutes(1);
                }
            }
            KeyCode::Down | KeyCode::Left => {
                if let Some(t) = self.pomodoro.as_mut() {
                    t.adjust_minutes(-1);
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => self.pomodoro_primary(),
            KeyCode::Esc => {
                // Cancel a running work/break and drop the timer, then hide.
                if let Some(t) = self.pomodoro.as_mut() {
                    t.stop();
                }
                if matches!(self.pomodoro.as_ref().map(|t| t.phase), Some(Phase::Idle) | None) {
                    self.pomodoro = None;
                }
                self.pomodoro_open = false;
            }
            _ => {}
        }
    }

    /// Refresh the cached git state (repo?, branch, changed files) for the workspace
    /// root. Cheap enough to call after saves and git actions; not per-frame.
    pub fn refresh_git(&mut self) {
        self.git_repo = crate::git::is_repo(&self.root);
        // HEAD may have moved (commit/checkout) or the working tree changed; drop
        // the cached HEAD blobs so the diff gutter refetches.
        self.git_head_cache.clear();
        if self.git_repo {
            self.git_branch = crate::git::branch(&self.root);
            self.git_status = crate::git::status(&self.root);
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
                .and_then(|rel| crate::git::head_blob(&self.root, &rel))
                .unwrap_or_default();
            self.git_head_cache.insert(path.clone(), head);
        }
        let marks = crate::git::diff_marks(&self.git_head_cache[&path], &current);
        let styled: Vec<(usize, &str)> = marks.iter().map(|&(line, m)| (line, gutter_hex(m))).collect();
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_gutter_marks(styled);
        }
    }

    /// The diff hunks for the active tab (committed HEAD vs the current buffer),
    /// populating the HEAD blob cache on demand. Empty outside a repo or for
    /// images / unsaved buffers.
    fn active_hunks(&mut self) -> Vec<crate::git::Hunk> {
        if !self.git_repo {
            return Vec::new();
        }
        let Some((path, current)) = self.editor.active_tab().and_then(|t| {
            if t.is_image() {
                return None;
            }
            t.path.clone().map(|p| (p, t.text()))
        }) else {
            return Vec::new();
        };
        if !self.git_head_cache.contains_key(&path) {
            let head = path
                .strip_prefix(&self.root)
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .and_then(|rel| crate::git::head_blob(&self.root, &rel))
                .unwrap_or_default();
            self.git_head_cache.insert(path.clone(), head);
        }
        crate::git::hunks(&self.git_head_cache[&path], &current)
    }

    /// Move the cursor to the next (or previous) changed hunk, wrapping around.
    fn diff_goto(&mut self, forward: bool) {
        let hunks = self.active_hunks();
        if hunks.is_empty() {
            self.status = t!("status.no_changes").to_string();
            return;
        }
        let line = self.editor.active_tab().map_or(0, |t| t.editor.cursor_line());
        let target = if forward {
            hunks.iter().map(|h| h.current_start).find(|&s| s > line)
        } else {
            hunks.iter().rev().map(|h| h.current_start).find(|&s| s < line)
        };
        // Wrap to the first/last hunk when there is none beyond the cursor.
        let target = target.unwrap_or_else(|| {
            if forward { hunks[0].current_start } else { hunks[hunks.len() - 1].current_start }
        });
        let area = self.editor_view();
        self.editor.goto(target + 1, None, area);
    }

    /// Restore the committed (HEAD) version of the hunk under the cursor in the
    /// working buffer. No-op (with a status) when there is no hunk there.
    fn revert_hunk(&mut self) {
        let hunks = self.active_hunks();
        if hunks.is_empty() {
            self.status = t!("status.no_changes").to_string();
            return;
        }
        let line = self.editor.active_tab().map_or(0, |t| t.editor.cursor_line());
        let Some(hunk) = hunks.into_iter().find(|h| h.contains(line)) else {
            self.status = t!("status.no_hunk").to_string();
            return;
        };
        let Some(content) = self.editor.active_tab().map(Tab::text) else {
            return;
        };
        let lines: Vec<&str> = content.split_inclusive('\n').collect();
        let start = hunk.current_start.min(lines.len());
        let end = hunk.current_end.min(lines.len()).max(start);
        let mut rebuilt = String::new();
        rebuilt.push_str(&lines[..start].concat());
        rebuilt.push_str(&hunk.head_text);
        rebuilt.push_str(&lines[end..].concat());
        let caret: usize = lines[..start].iter().map(|l| l.chars().count()).sum();
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_content(&rebuilt);
            t.editor.set_cursor(caret);
            t.editor.set_selection(None);
            t.dirty = true;
            t.preview = false;
        }
        self.refresh_git_gutter();
        self.status = t!("status.hunk_reverted").to_string();
    }

    // ----- merge conflicts ------------------------------------------------

    /// Resolve the merge conflict at (or after) the cursor by keeping `how`.
    fn resolve_conflict(&mut self, how: crate::conflict_tool::Resolution) {
        let Some((content, line)) = self
            .editor
            .active_tab()
            .filter(|t| !t.is_image())
            .map(|t| (t.text(), t.editor.cursor_line()))
        else {
            return;
        };
        let Some(conflict) = crate::conflict_tool::find(&content, line) else {
            self.status = t!("status.no_conflict").to_string();
            return;
        };
        let lines: Vec<&str> = content.split_inclusive('\n').collect();
        let mut rebuilt = lines[..conflict.start].concat();
        rebuilt.push_str(&conflict.resolved(how));
        rebuilt.push_str(&lines[conflict.end.min(lines.len())..].concat());
        let caret: usize = lines[..conflict.start].iter().map(|l| l.chars().count()).sum();
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_content(&rebuilt);
            t.editor.set_cursor(caret);
            t.editor.set_selection(None);
            t.dirty = true;
            t.preview = false;
        }
        self.refresh_git_gutter();
        self.status = t!("status.conflict_resolved").to_string();
    }

    /// Move the cursor to the next merge conflict at or after it.
    fn conflict_next(&mut self) {
        let Some((content, line)) = self
            .editor
            .active_tab()
            .filter(|t| !t.is_image())
            .map(|t| (t.text(), t.editor.cursor_line()))
        else {
            return;
        };
        // Search from the line after the cursor so repeated calls advance.
        match crate::conflict_tool::find(&content, line + 1).or_else(|| crate::conflict_tool::find(&content, 0)) {
            Some(c) => {
                let area = self.editor_view();
                self.editor.goto(c.start + 1, None, area);
            }
            None => self.status = t!("status.no_conflict").to_string(),
        }
    }

    /// Run a workspace-level git op (stash/amend), then refresh state and report
    /// success with `ok_key` or the error in the status line.
    fn git_op(&mut self, op: fn(&Path) -> Result<(), String>, ok_key: &str) {
        if !self.git_repo {
            self.status = t!("status.git_not_repo").to_string();
            return;
        }
        match op(&self.root) {
            Ok(()) => {
                self.refresh_git();
                self.refresh_git_gutter();
                self.status = t!(ok_key).to_string();
            }
            Err(e) => self.status = t!("msg.git_failed", error = e).to_string(),
        }
    }

    /// Stage just the hunk under the cursor into the git index, leaving the rest
    /// of the file's changes unstaged and the working tree untouched. Safe: it
    /// only stages when the index still matches HEAD for the hunk's region.
    fn stage_hunk(&mut self) {
        let hunks = self.active_hunks();
        if hunks.is_empty() {
            self.status = t!("status.no_changes").to_string();
            return;
        }
        let line = self.editor.active_tab().map_or(0, |t| t.editor.cursor_line());
        let Some(hunk) = hunks.into_iter().find(|h| h.contains(line)) else {
            self.status = t!("status.no_hunk").to_string();
            return;
        };
        let Some((path, current)) = self
            .editor
            .active_tab()
            .and_then(|t| t.path.clone().map(|p| (p, t.text())))
        else {
            return;
        };
        let Ok(rel) = path.strip_prefix(&self.root) else {
            self.status = t!("status.stage_hunk_failed", error = "outside workspace").to_string();
            return;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        // Base the new index content on the currently-staged version (== HEAD
        // when nothing is staged yet), so other hunks stay unstaged.
        let base = crate::git::index_blob(&self.root, &rel)
            .or_else(|| self.git_head_cache.get(&path).cloned())
            .unwrap_or_default();
        let base_lines: Vec<&str> = base.split_inclusive('\n').collect();
        let head_count = hunk.head_text.split_inclusive('\n').count();
        // The hunk's committed lines must still be present in the index region,
        // or staging this hunk could corrupt other staged changes.
        let region = base_lines.get(hunk.head_start..hunk.head_start + head_count);
        if region.map(<[&str]>::concat).as_deref() != Some(hunk.head_text.as_str()) {
            self.status = t!("status.stage_hunk_failed", error = "index diverged").to_string();
            return;
        }
        let cur_lines: Vec<&str> = current.split_inclusive('\n').collect();
        let added = cur_lines
            .get(hunk.current_start..hunk.current_end)
            .map(<[&str]>::concat)
            .unwrap_or_default();
        let mut new_index = base_lines[..hunk.head_start].concat();
        new_index.push_str(&added);
        new_index.push_str(&base_lines[hunk.head_start + head_count..].concat());
        match crate::git::stage_content(&self.root, &rel, &new_index) {
            Ok(()) => {
                self.refresh_git();
                self.status = t!("status.hunk_staged").to_string();
            }
            Err(e) => self.status = t!("status.stage_hunk_failed", error = e).to_string(),
        }
    }

    /// Unstage just the hunk under the cursor from the git index, leaving the
    /// rest of the file's staged changes and the working tree untouched. The
    /// mirror of [`App::stage_hunk`]: safe — it only unstages when the hunk's
    /// working-tree lines are present in the index at the expected position.
    fn unstage_hunk(&mut self) {
        let hunks = self.active_hunks();
        if hunks.is_empty() {
            self.status = t!("status.no_changes").to_string();
            return;
        }
        let line = self.editor.active_tab().map_or(0, |t| t.editor.cursor_line());
        let Some(hunk) = hunks.into_iter().find(|h| h.contains(line)) else {
            self.status = t!("status.no_hunk").to_string();
            return;
        };
        let Some((path, current)) = self
            .editor
            .active_tab()
            .and_then(|t| t.path.clone().map(|p| (p, t.text())))
        else {
            return;
        };
        let Ok(rel) = path.strip_prefix(&self.root) else {
            self.status = t!("status.unstage_hunk_failed", error = "outside workspace").to_string();
            return;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        // The index must currently carry this hunk's working-tree lines (i.e. it
        // is staged); replacing them with the committed text removes it.
        let Some(base) = crate::git::index_blob(&self.root, &rel) else {
            self.status = t!("status.unstage_hunk_failed", error = "nothing staged").to_string();
            return;
        };
        let base_lines: Vec<&str> = base.split_inclusive('\n').collect();
        let cur_lines: Vec<&str> = current.split_inclusive('\n').collect();
        let added = cur_lines
            .get(hunk.current_start..hunk.current_end)
            .map(<[&str]>::concat)
            .unwrap_or_default();
        let added_count = hunk.current_end - hunk.current_start;
        let region = base_lines.get(hunk.head_start..hunk.head_start + added_count);
        if region.map(<[&str]>::concat).as_deref() != Some(added.as_str()) {
            self.status = t!("status.unstage_hunk_failed", error = "hunk not staged").to_string();
            return;
        }
        let mut new_index = base_lines[..hunk.head_start].concat();
        new_index.push_str(&hunk.head_text);
        new_index.push_str(&base_lines[hunk.head_start + added_count..].concat());
        match crate::git::stage_content(&self.root, &rel, &new_index) {
            Ok(()) => {
                self.refresh_git();
                self.status = t!("status.hunk_unstaged").to_string();
            }
            Err(e) => self.status = t!("status.unstage_hunk_failed", error = e).to_string(),
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
    pub fn git_change_for(&self, path: &Path) -> Option<crate::git::Change> {
        if !self.git_repo {
            return None;
        }
        let rel = path.strip_prefix(&self.root).ok()?.to_string_lossy().replace('\\', "/");
        self.git_status.iter().find(|s| s.path == rel).and_then(crate::git::FileStatus::primary)
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
        self.speller = crate::spellcheck::load_for(&self.settings.dictionary_path, &locale).ok();
        if let Some(sc) = self.speller.as_mut() {
            for word in Self::load_user_words() {
                sc.add_word(&word);
            }
        }
        self.speller_locale = Some(locale);
    }

    /// Read the persisted personal word list (one word per line). Empty when the
    /// file is missing or unreadable.
    fn load_user_words() -> Vec<String> {
        let Some(path) = Settings::user_dictionary_path() else { return Vec::new() };
        let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
        text.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect()
    }

    /// Append `word` to the persisted personal word list, creating it if needed
    /// and skipping duplicates.
    fn persist_user_word(word: &str) {
        use std::io::Write;
        let Some(path) = Settings::user_dictionary_path() else { return };
        if Self::load_user_words().iter().any(|w| w == word) {
            return;
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(f, "{word}");
        }
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
                if let Some(p) = self.spell_suggest.as_mut()
                    && p.selected + 1 < p.suggestions.len() {
                        p.selected += 1;
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
        if let Some(p) = self.spell_suggest.as_mut()
            && row < p.suggestions.len() {
                p.selected = row;
                self.spell_apply_selected();
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

    /// Add the misspelled word to the user dictionary, persisting it across
    /// sessions in `user_dictionary.txt`.
    fn spell_add_word(&mut self) {
        let Some(p) = self.spell_suggest.take() else {
            return;
        };
        if let Some(sc) = self.speller.as_mut() {
            sc.add_word(&p.word);
        }
        Self::persist_user_word(&p.word);
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
        if let Some(action) = action
            && action != SEP_ACTION {
                self.run_action(action);
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
                if let Some(p) = self.git_panel.as_mut()
                    && p.selected + 1 < self.git_status.len() {
                        p.selected += 1;
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
            crate::git::stage(&self.root, &path)
        } else {
            crate::git::unstage(&self.root, &path)
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
            .is_some_and(crate::git::FileStatus::is_staged);
        self.git_stage_selected(!staged);
    }

    /// Begin a commit: prompt for a message (only when something is staged).
    fn git_begin_commit(&mut self) {
        let any_staged = self.git_status.iter().any(crate::git::FileStatus::is_staged);
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
        match crate::git::commit(&self.root, message) {
            Ok(()) => self.status = t!("status.git_committed").into(),
            Err(e) => self.messages.error(t!("msg.git_commit_failed", error = e).to_string()),
        }
        self.refresh_git();
    }

    /// Begin creating a topic branch: prompt for its name (only in a repo).
    fn git_begin_new_branch(&mut self) {
        if !crate::git::is_repo(&self.root) {
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
        match crate::git::create_branch(&self.root, name) {
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
        self.git_log_since(None);
    }

    /// Show the commit log, optionally limited to commits newer than `since`
    /// (a git date spec like `1-day-ago`), streaming it into the bottom dock.
    fn git_log_since(&mut self, since: Option<&str>) {
        if !crate::git::is_repo(&self.root) {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        self.git_panel = None;
        let filter = since.map(|s| format!(" --since={s}")).unwrap_or_default();
        self.run_command(&format!("git --no-pager log{filter}"));
    }

    /// Show a decorated commit graph across all refs, streaming it into the
    /// bottom dock.
    fn git_log_graph(&mut self) {
        if !crate::git::is_repo(&self.root) {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        self.git_panel = None;
        self.run_command(
            "git --no-pager log --graph --topo-order --date=iso8601-strict \
             --no-abbrev-commit --decorate --all --boundary \
             --pretty=format:'%ad %h -%d %s [%aN <%aE>] %G?'",
        );
    }

    /// Show the working-tree status, streaming `git status` into the bottom dock.
    fn git_status_to_dock(&mut self) {
        if !crate::git::is_repo(&self.root) {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        self.git_panel = None;
        self.run_command("git --no-pager status");
    }

    /// Initialize a git repository in the workspace, refusing (for safety) if one
    /// already exists (a `.git` directory or a detected repo).
    fn git_init(&mut self) {
        if self.root.join(".git").exists() || crate::git::is_repo(&self.root) {
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

    /// Set the current branch's description (`git branch --edit-description`),
    /// feeding the prompted text via a throwaway `GIT_EDITOR` that copies it into
    /// the description file (so no interactive editor opens).
    fn git_edit_description(&mut self, desc: &str) {
        if !crate::git::is_repo(&self.root) {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        let tmp = std::env::temp_dir().join(format!("vix-branchdesc-{}.txt", std::process::id()));
        if std::fs::write(&tmp, desc).is_err() {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        let path = tmp.display();
        self.git_panel = None;
        self.run_command(&format!("GIT_EDITOR='cp \"{path}\"' git branch --edit-description"));
    }

    /// Delete the named branch (`git branch --delete`), streaming the result to
    /// the bottom dock.
    fn git_delete_branch(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        if !crate::git::is_repo(&self.root) {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        self.git_panel = None;
        self.run_command(&format!("git branch --delete {name}"));
    }

    /// Search the repository for `pattern` (`git grep`), streaming matches with
    /// line numbers into the bottom dock.
    fn git_grep(&mut self, pattern: &str) {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            return;
        }
        if !crate::git::is_repo(&self.root) {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        self.git_panel = None;
        // Single-quote the pattern so shell metacharacters in the regex are safe.
        let quoted = format!("'{}'", pattern.replace('\'', "'\\''"));
        self.run_command(&format!("git --no-pager grep -n -e {quoted}"));
    }

    /// Annotate the cursor's current line with its `git blame` attribution
    /// (short hash, author, date, and commit summary) in the status bar.
    fn git_blame_line(&mut self) {
        if !crate::git::is_repo(&self.root) {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        let Some((path, line)) = self
            .editor
            .active_tab()
            .and_then(|t| t.path.clone().map(|p| (p, t.editor.cursor_line() + 1)))
        else {
            self.status = t!("status.blame_no_file").into();
            return;
        };
        // Blame from the file's own directory so git resolves the repo itself —
        // robust to symlinked roots (e.g. macOS `/var` → `/private/var`).
        let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
            self.status = t!("status.blame_no_file").into();
            return;
        };
        let rel = name.to_string_lossy();
        match crate::git::blame_line(dir, &rel, line) {
            Some(b) if b.is_uncommitted() => {
                self.status = t!("status.blame_uncommitted", line = line).to_string();
            }
            Some(b) => {
                self.status = t!(
                    "status.blame",
                    line = line,
                    hash = b.hash,
                    author = b.author,
                    date = b.date,
                    summary = b.summary
                )
                .to_string();
            }
            None => self.status = t!("status.blame_none").into(),
        }
    }

    /// Toggle the inline (end-of-line) git blame for the cursor's line, persisting
    /// the preference. Clears the annotation immediately when turned off.
    fn toggle_inline_blame(&mut self) {
        self.settings.inline_blame = !self.settings.inline_blame;
        let on = self.settings.inline_blame;
        if !on {
            self.blame_cache = None;
            if let Some(t) = self.editor.active_tab_mut() {
                t.editor.set_eol_note(None);
            }
        }
        self.status = t!(if on { "status.inline_blame_on" } else { "status.inline_blame_off" }).to_string();
    }

    /// Refresh the inline blame for the cursor's line when enabled. Cheap: blames
    /// only when the cursor moves to a different line (cached in `blame_cache`).
    /// Called once per event-loop iteration.
    pub fn refresh_inline_blame(&mut self) {
        if !self.settings.inline_blame {
            if self.blame_cache.take().is_some()
                && let Some(t) = self.editor.active_tab_mut()
            {
                t.editor.set_eol_note(None);
            }
            return;
        }
        let here = self
            .editor
            .active_tab()
            .filter(|t| !t.is_image())
            .and_then(|t| t.path.clone().map(|p| (p, t.editor.cursor_line())));
        let Some((path, line0)) = here else {
            if let Some(t) = self.editor.active_tab_mut() {
                t.editor.set_eol_note(None);
            }
            return;
        };
        let line = line0 + 1;
        if self.blame_cache.as_ref() == Some(&(path.clone(), line)) {
            return;
        }
        self.blame_cache = Some((path.clone(), line));
        let note = match (path.parent(), path.file_name()) {
            (Some(dir), Some(name)) => match crate::git::blame_line(dir, &name.to_string_lossy(), line) {
                Some(b) if b.is_uncommitted() => t!("blame.uncommitted").to_string(),
                Some(b) => t!("blame.inline", author = b.author, date = b.date, summary = b.summary).to_string(),
                None => String::new(),
            },
            _ => String::new(),
        };
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_eol_note(if note.is_empty() { None } else { Some((line0, note)) });
        }
    }

    // ----- Debugger (DAP) -------------------------------------------------

    /// The debug adapter configured for `path`'s file extension, if any.
    fn adapter_for(&self, path: &Path) -> Option<crate::dap::DebugAdapter> {
        let ext = path.extension().and_then(|e| e.to_str())?.to_ascii_lowercase();
        self.settings.debug_adapters.iter().find(|a| a.extensions.iter().any(|e| e == &ext)).cloned()
    }

    /// Toggle a breakpoint on the cursor's line in the active file.
    fn toggle_breakpoint(&mut self) {
        let Some((path, line)) = self
            .editor
            .active_tab()
            .and_then(|t| t.path.clone().map(|p| (p, t.editor.cursor_line() + 1)))
        else {
            self.status = t!("status.blame_no_file").to_string();
            return;
        };
        let set = self.breakpoints.entry(path.clone()).or_default();
        if !set.remove(&line) {
            set.insert(line);
        }
        let lines: Vec<usize> = set.iter().copied().collect();
        if self.dap.is_active() {
            self.dap.set_breakpoints(&path.to_string_lossy(), &lines);
        }
        self.refresh_debug_markers();
    }

    /// Start debugging the active file with its configured adapter.
    fn start_debugger(&mut self) {
        let Some(path) = self.editor.active_tab().and_then(|t| t.path.clone()) else {
            self.status = t!("status.debug_no_file").to_string();
            return;
        };
        let Some(adapter) = self.adapter_for(&path) else {
            self.status = t!("status.debug_no_adapter").to_string();
            return;
        };
        let bps: std::collections::HashMap<String, Vec<usize>> = self
            .breakpoints
            .iter()
            .map(|(p, l)| (p.to_string_lossy().into_owned(), l.iter().copied().collect()))
            .collect();
        if self.dap.start(&adapter, &path.to_string_lossy(), bps) {
            self.show_debug_panel = true;
            self.status = t!("status.debug_started").to_string();
        } else {
            self.status = t!("status.debug_failed").to_string();
        }
    }

    /// Stop the active debug session and clear its state.
    fn stop_debugger(&mut self) {
        self.dap.stop();
        self.dap_stopped = None;
        self.dap_stack.clear();
        self.dap_variables.clear();
        self.refresh_debug_markers();
        self.status = t!("status.debug_stopped").to_string();
    }

    /// Push the current breakpoints and stopped line into the active tab's editor
    /// so the gutter shows them.
    fn refresh_debug_markers(&mut self) {
        let stopped = self.dap_stopped.clone();
        let Some(path) = self.editor.active_tab().and_then(|t| t.path.clone()) else { return };
        let lines: Vec<usize> =
            self.breakpoints.get(&path).map(|s| s.iter().map(|l| l.saturating_sub(1)).collect()).unwrap_or_default();
        let debug_line = stopped.filter(|(p, _)| *p == path).map(|(_, l)| l.saturating_sub(1));
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_breakpoints(lines);
            t.editor.set_debug_line(debug_line);
        }
    }

    /// Whether a debug session is active (drives the fast poll cadence).
    #[must_use]
    pub fn dap_busy(&self) -> bool {
        self.dap.busy()
    }

    /// The breakpoint lines (1-based) set on the active file. For tests/tools.
    #[must_use]
    pub fn active_breakpoints(&self) -> Vec<usize> {
        self.editor
            .active_tab()
            .and_then(|t| t.path.as_ref())
            .and_then(|p| self.breakpoints.get(p))
            .map(|s| s.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Drain debug-adapter events and apply them. Called each event-loop iteration.
    pub fn poll_dap(&mut self) {
        if !self.dap.is_active() {
            return;
        }
        for event in self.dap.poll() {
            match event {
                crate::dap::DapEvent::Running => self.status = t!("status.debug_running").to_string(),
                crate::dap::DapEvent::Stopped { reason, .. } => {
                    self.status = t!("status.debug_stopped_at", reason = reason).to_string();
                }
                crate::dap::DapEvent::Output(text) => self.bottom_dock.push(text),
                crate::dap::DapEvent::Stack(frames) => {
                    if let Some(top) = frames.first()
                        && let Some(p) = top.path.clone()
                    {
                        self.dap_stopped = Some((PathBuf::from(&p), top.line));
                        self.jump_to_debug_location(&p, top.line);
                    }
                    self.dap_stack = frames;
                    self.refresh_debug_markers();
                }
                crate::dap::DapEvent::Variables(vars) => self.dap_variables = vars,
                crate::dap::DapEvent::Evaluated { expr, result } => {
                    // Update a matching watch, else echo to the debug console.
                    if let Some(w) = self.dap_watches.iter_mut().find(|(e, _)| *e == expr) {
                        w.1 = result;
                    } else {
                        self.bottom_dock.push(format!("{expr} = {result}"));
                    }
                }
                crate::dap::DapEvent::Terminated => {
                    self.dap_stopped = None;
                    self.dap_stack.clear();
                    self.dap_variables.clear();
                    self.refresh_debug_markers();
                    self.status = t!("status.debug_terminated").to_string();
                }
            }
        }
        // Re-evaluate watches whenever we are stopped.
        if self.dap.is_stopped() {
            let exprs: Vec<String> = self.dap_watches.iter().map(|(e, _)| e.clone()).collect();
            for e in exprs {
                self.dap.evaluate(&e);
            }
        }
    }

    /// Open `path` and move the cursor to `line` (1-based) for a debug stop.
    fn jump_to_debug_location(&mut self, path: &str, line: usize) {
        let p = PathBuf::from(path);
        if p.is_file() {
            self.with_jump(|s| {
                s.open_path(&p, false);
                let area = s.editor_view();
                s.editor.goto(line, None, area);
                s.focus = Focus::Editor;
            });
        }
    }

    /// Run a remote git command (push/pull/fetch) asynchronously, streaming its
    /// output to the bottom dock. Git state refreshes when it completes.
    fn git_remote_command(&mut self, cmd: &str) {
        if !crate::git::is_repo(&self.root) {
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
        if !crate::git::is_repo(&self.root) {
            self.status = t!("status.git_not_repo").into();
            return;
        }
        let branches = crate::git::local_branches(&self.root);
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
        if let Some(idx) = self.chooser_row(mouse)
            && let Some(c) = self.branch_chooser.as_mut()
                && idx < c.branches.len() {
                    c.selected = idx;
                    self.checkout_selected_branch();
                }
    }

    // ----- Tasks (tasks.toml runner) --------------------------------------

    /// Open the task chooser from the workspace's `tasks.toml`, or report when no
    /// tasks are defined.
    fn open_tasks(&mut self) {
        let tasks = crate::tasks::load(&self.root);
        if tasks.is_empty() {
            self.status = t!("status.no_tasks").to_string();
            return;
        }
        self.task_chooser = Some(TaskChooser { tasks, selected: 0 });
    }

    fn tasks_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(c) = self.task_chooser.as_mut() {
                    let n = c.tasks.len();
                    c.selected = (c.selected + n - 1) % n;
                }
            }
            KeyCode::Down => {
                if let Some(c) = self.task_chooser.as_mut() {
                    c.selected = (c.selected + 1) % c.tasks.len();
                }
            }
            KeyCode::Enter => self.run_selected_task(),
            KeyCode::Esc => self.task_chooser = None,
            _ => {}
        }
    }

    fn tasks_mouse(&mut self, mouse: MouseEvent) {
        if let Some(idx) = self.chooser_row(mouse)
            && let Some(c) = self.task_chooser.as_mut()
            && idx < c.tasks.len()
        {
            c.selected = idx;
            self.run_selected_task();
        }
    }

    /// Run the highlighted task's command through the async Run pipeline.
    fn run_selected_task(&mut self) {
        let Some(c) = self.task_chooser.take() else { return };
        if let Some(task) = c.tasks.get(c.selected) {
            let command = task.command.clone();
            self.run_command(&command);
        }
    }

    // ----- Compare With File (diff view) ----------------------------------

    /// Prompt for a file path to compare the active buffer against.
    fn open_compare_prompt(&mut self) {
        if self.editor.active_tab().is_none() {
            return;
        }
        self.prompt = Some(Prompt::new(PromptKind::CompareFile, t!("prompt.compare_file").to_string()));
    }

    /// Open a read-only unified-diff overlay comparing the active buffer with the
    /// file at `input` (resolved relative to the workspace root).
    fn open_diff_with(&mut self, input: &str) {
        if input.is_empty() {
            return;
        }
        let other = self.resolve(input);
        let Ok(other_text) = std::fs::read_to_string(&other) else {
            self.messages.error(t!("msg.open_failed", error = other.display()).to_string());
            return;
        };
        let Some(tab) = self.editor.active_tab() else { return };
        let current = tab.editor.get_content();
        let here = tab.path.as_ref().and_then(|p| p.file_name()).map_or_else(
            || t!("ui.untitled").to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let there = other.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let lines = crate::diff_view::build(&other_text, &current);
        if lines.is_empty() {
            self.status = t!("status.diff_identical").to_string();
            return;
        }
        self.diff_view = Some(DiffViewState { title: format!("{there} ↔ {here}"), lines, scroll: 0 });
    }

    fn diff_view_key(&mut self, key: KeyEvent) {
        let page = self.layout.editor.height.max(1) as usize;
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.diff_view = None,
            KeyCode::Up => {
                if let Some(d) = self.diff_view.as_mut() {
                    d.scroll = d.scroll.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if let Some(d) = self.diff_view.as_mut() {
                    d.scroll = (d.scroll + 1).min(d.lines.len().saturating_sub(1));
                }
            }
            KeyCode::PageUp => {
                if let Some(d) = self.diff_view.as_mut() {
                    d.scroll = d.scroll.saturating_sub(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(d) = self.diff_view.as_mut() {
                    d.scroll = (d.scroll + page).min(d.lines.len().saturating_sub(1));
                }
            }
            _ => {}
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
        match crate::git::checkout(&self.root, &branch) {
            Ok(()) => {
                self.status = t!("status.git_switched", branch = branch).to_string();
                self.refresh_git();
                // Files on disk may now differ; refresh the explorer tree and
                // reload any open clean buffers so they reflect the new branch.
                self.explorer.rebuild();
                let n = self.editor.reload_clean_from_disk();
                if n > 0 {
                    self.messages.info(t!("status.git_reloaded", count = n).to_string());
                }
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
        // Capture editor-bound keys into a macro while recording (modal/menu keys
        // never reach here, so they are naturally excluded).
        if self.macro_recording && !self.macro_playing {
            self.macro_keys.push(key);
        }
        // While a snippet is expanding, Tab walks its fields and Esc ends it.
        if self.snippet_active() {
            match key.code {
                KeyCode::Tab if !Self::ctrl(&key) && !Self::alt(&key) => {
                    self.snippet_tab();
                    return;
                }
                KeyCode::Esc => {
                    self.snippet_session = None;
                    return;
                }
                _ => {}
            }
        }
        // A plain Tab after a snippet prefix word expands that snippet.
        if matches!(key.code, KeyCode::Tab) && !Self::ctrl(&key) && !Self::alt(&key) && !Self::shift(&key)
            && self.expand_snippet_prefix()
        {
            return;
        }
        let area = self.editor_view();
        match key.code {
            KeyCode::Home => return self.editor.cursor_line_home(),
            KeyCode::End => return self.editor.cursor_line_end(),
            KeyCode::Delete => {
                let multi = self.editor.active_tab().is_some_and(|t| t.editor.has_multi_carets());
                if multi {
                    if let Some(t) = self.editor.active_tab_mut() {
                        t.editor.multi_delete(true);
                    }
                } else {
                    self.editor.delete_forward();
                }
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

        // Overwrite mode: a plain character types over the one under the cursor
        // (delete it first, then the normal insert below replaces it). Skipped at
        // end-of-line, with a selection, or with multiple carets.
        if self.overwrite && matches!(key.code, KeyCode::Char(_)) && !Self::ctrl(&key) && !Self::alt(&key) {
            let over_char = self.editor.active_tab_mut().is_some_and(|t| {
                if t.editor.has_multi_carets() || t.editor.get_selection().is_some_and(|s| !s.is_empty()) {
                    return false;
                }
                let cur = t.editor.get_cursor();
                let code = t.editor.code_ref();
                let line = code.char_to_line(cur);
                cur < code.line_to_char(line) + code.line_len(line)
            });
            if over_char {
                self.editor.delete_forward();
            }
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

    /// Replay the recorded macro's editor keys at the current cursor. No-op while
    /// recording or when nothing has been recorded.
    fn play_macro(&mut self) {
        if self.macro_recording || self.macro_keys.is_empty() {
            return;
        }
        self.macro_playing = true;
        for key in self.macro_keys.clone() {
            self.editor_key(key);
        }
        self.macro_playing = false;
        self.status = t!("status.macro_played").to_string();
    }

    /// Prompt for a name to save the just-recorded macro under (persisted to
    /// `macros.toml`). No-ops with a status note when nothing was recorded.
    fn begin_save_macro(&mut self) {
        if self.macro_keys.is_empty() {
            self.status = t!("status.macro_empty").to_string();
            return;
        }
        self.prompt = Some(Prompt::new(PromptKind::SaveMacro, t!("prompt.save_macro").to_string()));
    }

    /// Persist the recorded macro under `name` to `macros.toml`.
    fn save_macro(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() || self.macro_keys.is_empty() {
            return;
        }
        let Some(path) = Settings::macros_path() else { return };
        let mac = crate::macros::Macro { name: name.to_string(), keys: crate::macros::encode(&self.macro_keys) };
        match crate::macros::upsert(&path, mac) {
            Ok(()) => self.status = t!("status.macro_saved", name = name).to_string(),
            Err(e) => self.messages.error(t!("msg.save_failed", error = e).to_string()),
        }
    }

    /// Open the saved-macro chooser, or report when none are saved.
    fn open_macro_chooser(&mut self) {
        let macros = Settings::macros_path().map(|p| crate::macros::load(&p)).unwrap_or_default();
        if macros.is_empty() {
            self.status = t!("status.no_macros").to_string();
            return;
        }
        self.macro_chooser = Some(MacroChooser { macros, selected: 0 });
    }

    fn macro_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(c) = self.macro_chooser.as_mut() {
                    let n = c.macros.len();
                    c.selected = (c.selected + n - 1) % n;
                }
            }
            KeyCode::Down => {
                if let Some(c) = self.macro_chooser.as_mut() {
                    c.selected = (c.selected + 1) % c.macros.len();
                }
            }
            KeyCode::Enter => self.run_selected_macro(),
            KeyCode::Esc => self.macro_chooser = None,
            _ => {}
        }
    }

    fn macro_mouse(&mut self, mouse: MouseEvent) {
        if let Some(idx) = self.chooser_row(mouse)
            && let Some(c) = self.macro_chooser.as_mut()
            && idx < c.macros.len()
        {
            c.selected = idx;
            self.run_selected_macro();
        }
    }

    /// Load the highlighted saved macro into the active macro buffer and play it.
    fn run_selected_macro(&mut self) {
        let Some(c) = self.macro_chooser.take() else { return };
        if let Some(mac) = c.macros.get(c.selected) {
            self.macro_keys = crate::macros::decode(&mac.keys);
            self.focus = Focus::Editor;
            self.play_macro();
        }
    }

    // ----- Recent-projects switcher ---------------------------------------

    /// Open the recent-projects chooser from the saved session, excluding the
    /// current workspace. Reports when there is nowhere else to switch to.
    fn open_workspace_chooser(&mut self) {
        let current = self.session_key();
        let roots: Vec<String> = crate::session::Session::load()
            .workspaces
            .into_iter()
            .map(|w| w.root)
            .filter(|r| *r != current)
            .collect();
        if roots.is_empty() {
            self.status = t!("status.no_recent_projects").to_string();
            return;
        }
        self.workspace_chooser = Some(WorkspaceChooser { roots, selected: 0 });
    }

    fn workspace_chooser_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(c) = self.workspace_chooser.as_mut() {
                    let n = c.roots.len();
                    c.selected = (c.selected + n - 1) % n;
                }
            }
            KeyCode::Down => {
                if let Some(c) = self.workspace_chooser.as_mut() {
                    c.selected = (c.selected + 1) % c.roots.len();
                }
            }
            KeyCode::Enter => self.switch_to_selected_workspace(),
            KeyCode::Esc => self.workspace_chooser = None,
            _ => {}
        }
    }

    fn workspace_chooser_mouse(&mut self, mouse: MouseEvent) {
        if let Some(idx) = self.chooser_row(mouse)
            && let Some(c) = self.workspace_chooser.as_mut()
            && idx < c.roots.len()
        {
            c.selected = idx;
            self.switch_to_selected_workspace();
        }
    }

    /// Switch to the highlighted recent project.
    fn switch_to_selected_workspace(&mut self) {
        let Some(c) = self.workspace_chooser.take() else { return };
        let Some(root) = c.roots.get(c.selected).cloned() else { return };
        let path = PathBuf::from(&root);
        if !path.is_dir() {
            self.status = t!("status.project_missing", path = root).to_string();
            return;
        }
        self.switch_workspace(&path);
    }

    /// Re-root the app at `new_root`: persist the current session, restart the
    /// LSP, rebuild the explorer, reset the tabs, refresh git, and restore the new
    /// workspace's saved session.
    fn switch_workspace(&mut self, new_root: &Path) {
        self.save_session();
        self.lsp.shutdown();
        self.lsp_synced.clear();
        self.root = new_root.to_path_buf();
        self.explorer = Explorer::new(new_root.to_path_buf());
        self.lsp = crate::lsp::Lsp::new(
            self.settings.lsp_enabled,
            self.settings.lsp_servers.clone(),
            new_root,
        );
        self.editor.close_all();
        self.refresh_git();
        let key = self.session_key();
        if let Some(ws) = crate::session::Session::load().workspace(&key).cloned() {
            self.apply_session(&ws);
        }
        self.focus = Focus::Editor;
        self.status = t!("status.project_switched", path = new_root.display()).to_string();
    }

    /// Buffer-word autocomplete: complete the word before the cursor from other
    /// words in the buffer, cycling on repeated calls (`forward` chooses the
    /// direction). Like classic "dynamic abbreviation" expansion.
    fn autocomplete(&mut self, forward: bool) {
        let Some(cur) = self.editor.active_tab().filter(|t| !t.is_image()).map(|t| t.editor.get_cursor()) else {
            return;
        };
        // Continue an active cycle when the cursor still sits at the last insert.
        let cycling = self.complete_session.as_ref().is_some_and(|s| s.end == cur);
        if cycling {
            let s = self.complete_session.as_mut().unwrap();
            let n = s.candidates.len();
            s.index = if forward { (s.index + 1) % n } else { (s.index + n - 1) % n };
            let (anchor, end, word) = (s.anchor, s.end, s.candidates[s.index].clone());
            self.replace_range_chars(anchor, end, &word);
            if let Some(s) = self.complete_session.as_mut() {
                s.end = anchor + word.chars().count();
            }
            return;
        }
        // Start a new cycle: collect distinct buffer words sharing the prefix.
        let content = self.editor.active_tab().map(Tab::text).unwrap_or_default();
        let chars: Vec<char> = content.chars().collect();
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        let mut start = cur.min(chars.len());
        while start > 0 && is_word(chars[start - 1]) {
            start -= 1;
        }
        let prefix: String = chars[start..cur.min(chars.len())].iter().collect();
        if prefix.is_empty() {
            self.status = t!("status.no_completions").to_string();
            return;
        }
        let mut seen = std::collections::HashSet::new();
        let mut candidates: Vec<String> = Vec::new();
        for word in content.split(|c: char| !is_word(c)) {
            if word.len() > prefix.len() && word.starts_with(&prefix) && seen.insert(word) {
                candidates.push(word.to_string());
            }
        }
        if candidates.is_empty() {
            self.status = t!("status.no_completions").to_string();
            return;
        }
        let word = candidates[0].clone();
        self.replace_range_chars(start, cur, &word);
        self.complete_session = Some(CompleteSession {
            anchor: start,
            candidates,
            index: 0,
            end: start + word.chars().count(),
        });
    }

    /// Replace the character range `[start, end)` of the active buffer with `text`
    /// and put the cursor after it (one undoable edit).
    fn replace_range_chars(&mut self, start: usize, end: usize, text: &str) {
        if let Some(t) = self.editor.active_tab_mut() {
            let chars: Vec<char> = t.editor.get_content().chars().collect();
            let n = chars.len();
            let (a, b) = (start.min(n), end.min(n).max(start.min(n)));
            let mut out: String = chars[..a].iter().collect();
            out.push_str(text);
            out.extend(&chars[b..]);
            t.editor.set_content(&out);
            t.editor.set_cursor(a + text.chars().count());
            t.dirty = true;
            t.preview = false;
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
        let character = crate::lsp_core::position::char_to_col(&line_text, cur - line_start, enc);
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
            self.lsp.request_folding_range(&path); // fetch foldable ranges once on open
            self.request_inlay_hints(&path);
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
                crate::lsp::LspEvent::Hover(text) | crate::lsp::LspEvent::SignatureHelp(text) => {
                    self.hover = Some(HoverPopup { text });
                }
                crate::lsp::LspEvent::Definition { path, line, character } => {
                    self.lsp_jump(&path, line, character);
                }
                crate::lsp::LspEvent::Completion(items) => {
                    if !items.is_empty() {
                        self.completion = Some(CompletionPopup { items, selected: 0 });
                        self.resolve_selected_completion();
                    }
                }
                crate::lsp::LspEvent::CompletionDetail(text) => {
                    if let Some(popup) = self.completion.as_mut()
                        && let Some(item) = popup.items.get_mut(popup.selected)
                    {
                        item.detail = Some(text);
                    }
                }
                crate::lsp::LspEvent::References(locs) => self.show_references(&locs),
                crate::lsp::LspEvent::Edits(edits) => self.apply_lsp_edits(&edits),
                crate::lsp::LspEvent::DocumentSymbols(syms) => self.show_document_symbols(&syms),
                crate::lsp::LspEvent::WorkspaceSymbols(syms) => self.show_workspace_symbols(&syms),
                crate::lsp::LspEvent::WorkspaceEdit(edits) => self.apply_workspace_edit(&edits),
                crate::lsp::LspEvent::CodeActions(actions) => {
                    self.code_actions = Some(CodeActionMenu { actions, selected: 0 });
                }
                crate::lsp::LspEvent::CodeLenses(lenses) => {
                    self.code_lens = Some(CodeLensMenu { lenses, selected: 0 });
                }
                crate::lsp::LspEvent::SelectionRanges(ranges) => self.apply_selection_range(&ranges),
                crate::lsp::LspEvent::Highlights(ranges) => self.apply_document_highlights(&ranges),
                crate::lsp::LspEvent::InlayHints(hints) => self.apply_inlay_hints(&hints),
                crate::lsp::LspEvent::LinkedRanges(ranges) => self.begin_linked_edit(&ranges),
                crate::lsp::LspEvent::FoldingRanges(ranges) => {
                    let ranges: Vec<(usize, usize)> =
                        ranges.iter().map(|&(s, e)| (s as usize, e as usize)).collect();
                    if let Some(t) = self.editor.active_tab_mut() {
                        t.editor.set_fold_ranges(ranges);
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
        let ranges: Vec<(crate::lsp_core::Range, crate::lsp_core::Severity)> = self
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
                let col = crate::lsp_core::position::col_to_char(&line_text, character, enc);
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
        if self.completion.is_none() {
            return false;
        }
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.completion.as_mut() {
                    p.selected = p.selected.saturating_sub(1);
                }
                self.resolve_selected_completion();
                true
            }
            KeyCode::Down => {
                if let Some(p) = self.completion.as_mut()
                    && p.selected + 1 < p.items.len()
                {
                    p.selected += 1;
                }
                self.resolve_selected_completion();
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

    /// Lazily fetch fuller detail for the highlighted completion item via
    /// `completionItem/resolve`, when it has no detail yet but carries resolve
    /// data.
    fn resolve_selected_completion(&mut self) {
        let Some(path) = self.active_path() else { return };
        if !self.lsp.handles(&path) {
            return;
        }
        let Some(popup) = self.completion.as_ref() else { return };
        let Some(item) = popup.items.get(popup.selected) else { return };
        if item.detail.is_some() {
            return; // already has detail to show
        }
        let Some(data) = item.data.clone() else { return };
        let label = item.label.clone();
        self.lsp.request_completion_resolve(&path, &label, Some(&data));
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
                if let Some(op) = self.paste.take()
                    && op.cut {
                        self.clip.clear();
                        self.clip_cut = false;
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
        if let Some(node) = self.explorer.selected_node()
            && !node.is_dir && !is_image_path(&node.path) {
                let path = node.path.clone();
                let _ = self.editor.open(&path, true);
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
                self.apply_editorconfig_indent(path);
                self.status = t!("status.opened", path = path.display()).to_string();
            }
            Err(e) => self.messages.error(t!("msg.open_failed", error = e).to_string()),
        }
    }

    /// Apply the `.editorconfig` indent (style/size) for `path` to the active tab,
    /// when `EditorConfig` support is enabled and the file's config specifies one.
    fn apply_editorconfig_indent(&mut self, path: &Path) {
        let auto_pair = self.settings.auto_pair;
        if let Some(tab) = self.editor.active_tab_mut() {
            tab.editor.set_auto_pair(auto_pair);
        }
        if !self.settings.editorconfig {
            return;
        }
        if let Some(indent) = crate::editorconfig::resolve(path).indent_string()
            && let Some(tab) = self.editor.active_tab_mut()
        {
            tab.editor.set_indent(Some(indent));
        }
    }

    /// Best-effort terminal font zoom. A TUI cannot portably resize the font, so
    /// this emits the escape sequence for terminals that support one (xterm
    /// `OSC 50`, urxvt `OSC 720/721`) based on `$TERM`; on other terminals it
    /// reports that font size is controlled by the terminal itself. `delta`: +1
    /// larger, -1 smaller, 0 reset.
    fn terminal_zoom(&mut self, delta: i32) {
        use std::io::Write;
        let term = std::env::var("TERM").unwrap_or_default();
        let seq: Option<&[u8]> = if term.contains("rxvt") {
            match delta {
                d if d > 0 => Some(b"\x1b]720;1\x07"),
                d if d < 0 => Some(b"\x1b]721;1\x07"),
                _ => None, // urxvt has no reset sequence
            }
        } else if term.contains("xterm") {
            match delta {
                d if d > 0 => Some(b"\x1b]50;#+1\x07"),
                d if d < 0 => Some(b"\x1b]50;#-1\x07"),
                _ => Some(b"\x1b]50;#0\x07"),
            }
        } else {
            None
        };
        if let Some(bytes) = seq {
            let mut out = std::io::stdout();
            let _ = out.write_all(bytes);
            let _ = out.flush();
            self.status = t!("status.zoom_sent").to_string();
        } else {
            self.status = t!("status.zoom_unsupported").to_string();
        }
    }

    /// Toggle bracket/quote auto-pairing for every open buffer and persist it.
    fn toggle_auto_pair(&mut self) {
        self.settings.auto_pair = !self.settings.auto_pair;
        let on = self.settings.auto_pair;
        for tab in &mut self.editor.tabs {
            tab.editor.set_auto_pair(on);
        }
        self.status = t!(if on { "status.auto_pair_on" } else { "status.auto_pair_off" }).to_string();
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

    // ----- bookmarks ------------------------------------------------------

    /// Toggle a bookmark on the current file's current line.
    fn toggle_bookmark(&mut self) {
        let Some(loc) = self.current_location() else {
            return;
        };
        if let Some(i) = self.bookmarks.iter().position(|b| b.path == loc.path && b.line == loc.line) {
            self.bookmarks.remove(i);
            self.status = t!("status.bookmark_removed").to_string();
        } else {
            self.bookmarks.push(loc);
            self.status = t!("status.bookmark_added").to_string();
        }
    }

    /// Jump to the next (or previous) bookmark across all files, wrapping. Sorted
    /// by path then line so navigation order is stable.
    fn bookmark_goto(&mut self, forward: bool) {
        if self.bookmarks.is_empty() {
            self.status = t!("status.no_bookmarks").to_string();
            return;
        }
        let mut marks = self.bookmarks.clone();
        marks.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
        let here = self.current_location();
        let key = |l: &Location| (l.path.clone(), l.line);
        let target = if forward {
            here.as_ref()
                .and_then(|h| marks.iter().find(|m| key(m) > key(h)))
                .or_else(|| marks.first())
        } else {
            here.as_ref()
                .and_then(|h| marks.iter().rev().find(|m| key(m) < key(h)))
                .or_else(|| marks.last())
        };
        if let Some(loc) = target.cloned() {
            let area = self.editor_view();
            self.with_jump(|s| {
                s.open_path(&loc.path, false);
                s.editor.goto(loc.line, Some(loc.col), area);
                s.focus = Focus::Editor;
            });
        }
    }

    /// Open the bookmark list in the location chooser (Enter jumps).
    fn list_bookmarks(&mut self) {
        if self.bookmarks.is_empty() {
            self.status = t!("status.no_bookmarks").to_string();
            return;
        }
        let mut entries = self.bookmarks.clone();
        entries.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
        self.location_chooser = Some(LocationChooser { entries, selected: 0 });
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
        if let Some(o) = origin
            && self.nav_history.last() != Some(&o) {
                self.nav_history.push(o);
            }
        if let Some(d) = dest
            && self.nav_history.last() != Some(&d) {
                self.nav_history.push(d);
            }
        self.nav_idx = self.nav_history.len().saturating_sub(1);
    }

    fn nav_back(&mut self) {
        if self.nav_history.is_empty() || self.nav_idx == 0 {
            self.status = t!("status.no_earlier").into();
            return;
        }
        self.nav_idx -= 1;
        let loc = self.nav_history[self.nav_idx].clone();
        self.navigate_to(&loc);
    }

    fn nav_forward(&mut self) {
        if self.nav_idx + 1 >= self.nav_history.len() {
            self.status = t!("status.no_later").into();
            return;
        }
        self.nav_idx += 1;
        let loc = self.nav_history[self.nav_idx].clone();
        self.navigate_to(&loc);
    }

    /// Go to a recorded location without itself recording a new jump.
    fn navigate_to(&mut self, loc: &Location) {
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

    /// Handle a left click in one of the field-based tool dialogs (Color
    /// Converter, Calculator, Regex tester, Unit Converter), focusing the
    /// clicked field or running its action. Returns `true` if such a dialog was
    /// open. Extracted from [`App::try_overlay_mouse`] to keep it within the
    /// line limit.
    fn try_tool_dialog_mouse(&mut self, mouse: MouseEvent) -> bool {
        // The Color Converter dialog: a left click on a field row focuses it.
        if self.color_converter.is_some() {
            if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                use crate::color_converter_tool::Field;
                for field in Field::ALL {
                    if rect_contains(self.layout.color_converter_rows[field.index()], mouse.column, mouse.row) {
                        if let Some(c) = self.color_converter.as_mut() {
                            c.set_focus(field);
                        }
                        break;
                    }
                }
            }
            return true;
        }
        // The Calculator dialog: clicking the input focuses it; clicking Run
        // evaluates; clicking Insert inserts the result.
        if self.calculator.is_some() {
            if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                use crate::calculator_tool::Focus;
                let (col, row) = (mouse.column, mouse.row);
                if rect_contains(self.layout.calculator_rects[0], col, row) {
                    if let Some(c) = self.calculator.as_mut() {
                        c.focus = Focus::Input;
                    }
                } else if rect_contains(self.layout.calculator_rects[1], col, row) {
                    if let Some(c) = self.calculator.as_mut() {
                        c.focus = Focus::Run;
                        c.run();
                    }
                } else if rect_contains(self.layout.calculator_rects[2], col, row) {
                    if let Some(c) = self.calculator.as_mut() {
                        c.focus = Focus::Insert;
                    }
                    self.insert_calculator_result();
                }
            }
            return true;
        }
        // The Regex tester: a left click on a field row focuses it.
        if self.regex_tester.is_some() {
            if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                use crate::regex_tool::Field;
                for (i, f) in [Field::Pattern, Field::Subject].into_iter().enumerate() {
                    if rect_contains(self.layout.regex_tester_rows[i], mouse.column, mouse.row) {
                        if let Some(t) = self.regex_tester.as_mut() {
                            t.focus = f;
                        }
                        break;
                    }
                }
            }
            return true;
        }
        // The Unit Converter dialog: a left click on a row focuses it.
        if self.unit_converter.is_some() {
            if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                use crate::unit_converter_tool::Focus;
                for (i, focus) in [Focus::Value, Focus::From, Focus::To].into_iter().enumerate() {
                    if rect_contains(self.layout.unit_converter_rows[i], mouse.column, mouse.row) {
                        if let Some(c) = self.unit_converter.as_mut() {
                            c.focus = focus;
                        }
                        break;
                    }
                }
            }
            return true;
        }
        false
    }

    /// Route `mouse` to the highest-priority open overlay, returning `true` when
    /// one consumed it (overlays swallow mouse input rather than letting it fall
    /// through to the panes underneath). Extracted from [`App::on_mouse`] to keep
    /// that function within the line limit.
    fn try_overlay_mouse(&mut self, mouse: MouseEvent) -> bool {
        // The welcome overlay is modal: the wheel scrolls it, nothing else.
        if self.welcome.is_some() {
            self.welcome_mouse(mouse);
            return true;
        }
        // The right-click context menu takes all clicks while open (a click on a
        // row runs it; a click elsewhere dismisses it).
        if self.context_menu.is_some() {
            self.context_menu_mouse(mouse);
            return true;
        }
        // A right-click in the editor opens the context menu.
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Right))
            && rect_contains(self.layout.editor, mouse.column, mouse.row)
        {
            self.open_context_menu(mouse.column, mouse.row);
            return true;
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
            return true;
        }
        if self.try_tool_dialog_mouse(mouse) {
            return true;
        }
        // List/panel overlays: an open one consumes the click by delegating to
        // its handler (e.g. a left click on a row highlights it).
        macro_rules! panel {
            ($field:ident, $handler:ident) => {
                if self.$field.is_some() {
                    self.$handler(mouse);
                    return true;
                }
            };
        }
        panel!(recent_chooser, recent_mouse);
        panel!(location_chooser, location_mouse);
        panel!(nerd_palette, nerd_mouse);
        panel!(ascii_panel, ascii_mouse);
        panel!(x11_panel, x11_mouse);
        panel!(media_type_panel, media_type_mouse);
        panel!(html_panel, html_mouse);
        panel!(system_info, system_info_mouse);
        panel!(file_info, file_info_mouse);
        panel!(text_info, text_info_mouse);
        if self.markdown_preview.is_some() {
            if let Some(p) = self.markdown_preview.as_mut() {
                match mouse.kind {
                    MouseEventKind::ScrollDown => p.down(3),
                    MouseEventKind::ScrollUp => p.up(3),
                    _ => {}
                }
            }
            return true;
        }
        panel!(snippets, snippets_mouse);
        panel!(vcard, vcard_mouse);
        panel!(contacts, contacts_mouse);
        panel!(spell_suggest, spell_suggest_mouse);
        panel!(git_panel, git_panel_mouse);
        panel!(branch_chooser, branch_mouse);
        panel!(task_chooser, tasks_mouse);
        panel!(macro_chooser, macro_mouse);
        panel!(workspace_chooser, workspace_chooser_mouse);
        panel!(outline, outline_mouse);
        // The find / replace box: a left click focuses the Find or Replace field.
        panel!(search, search_mouse);
        // The calendar box: a left click inserts a date-time line or a day.
        if self.show_calendar {
            self.calendar_mouse(mouse);
            return true;
        }
        // The clock box: a left click inserts the picked time row.
        if self.show_clock {
            self.clock_mouse(mouse);
            return true;
        }
        // The Pomodoro dialog: a left click on the Start/Stop/Cancel button runs
        // it (Start closes the dialog and keeps the countdown running).
        if self.pomodoro_open {
            if let MouseEventKind::Down(MouseButton::Left) = mouse.kind
                && rect_contains(self.layout.pomodoro_button, mouse.column, mouse.row) {
                    self.pomodoro_primary();
                }
            return true;
        }
        // Keyboard-only modal overlays swallow all mouse input rather than
        // letting a click fall through to the editor/explorer underneath.
        self.show_help
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
    }

    /// Handle a press/drag on the window chrome at `(col, row)`: the vertical
    /// scrollbar, the horizontal scrollbars, a dock resize edge, or the split
    /// divider. Returns `true` if the event was consumed (a button release only
    /// clears the relevant active-drag flag and returns `false`). Extracted from
    /// [`App::on_mouse`] to keep it within the line limit.
    fn try_chrome_mouse(&mut self, mouse: MouseEvent, col: u16, row: u16) -> bool {
        // Editor scrollbar: press the thumb/track to jump there, then drag to
        // scroll. The drag continues even if the pointer leaves the 1-column
        // track (tracked by `scrollbar_active`), and ends on button release.
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left)
                if rect_contains(self.layout.scrollbar, col, row) =>
            {
                self.scrollbar_active = true;
                self.scrollbar_drag(row);
                return true;
            }
            MouseEventKind::Drag(MouseButton::Left) if self.scrollbar_active => {
                self.scrollbar_drag(row);
                return true;
            }
            MouseEventKind::Up(MouseButton::Left) => self.scrollbar_active = false,
            _ => {}
        }

        // Horizontal scrollbars (editor + docks): press the track to jump there,
        // then drag to scroll. Tracked by `hbar_active` so the drag continues off
        // the one-row track.
        let hbars = [
            (self.layout.editor_hscrollbar, HBar::Editor),
            (self.layout.explorer_hscrollbar, HBar::Explorer),
            (self.layout.messages_hscrollbar, HBar::Messages),
            (self.layout.bottom_hscrollbar, HBar::Bottom),
        ];
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                for (rect, target) in hbars {
                    if rect.width > 0 && rect_contains(rect, col, row) {
                        self.hbar_active = Some(target);
                        self.hbar_drag(target, col);
                        return true;
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(target) = self.hbar_active {
                    self.hbar_drag(target, col);
                    return true;
                }
            }
            MouseEventKind::Up(MouseButton::Left) => self.hbar_active = None,
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
                return true;
            }
            MouseEventKind::Down(MouseButton::Left) if Some(col) == left_edge => {
                self.dock_resize = Some(DockResize::Left);
                return true;
            }
            MouseEventKind::Down(MouseButton::Left) if Some(col) == right_edge => {
                self.dock_resize = Some(DockResize::Right);
                return true;
            }
            MouseEventKind::Drag(MouseButton::Left) if self.dock_resize.is_some() => {
                if matches!(self.dock_resize, Some(DockResize::Bottom)) {
                    self.resize_bottom_dock(row);
                } else {
                    self.resize_dock(col);
                }
                return true;
            }
            MouseEventKind::Up(MouseButton::Left) => self.dock_resize = None,
            _ => {}
        }

        // Split divider: press and drag to change the pane ratio. The press hits a
        // divider when `resize_split_at` reports it resized one.
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left)
                if self.editor.is_split()
                    && self.editor.resize_split_at(self.layout.editor_region, col, row) =>
            {
                self.split_resize = true;
                return true;
            }
            MouseEventKind::Drag(MouseButton::Left) if self.split_resize => {
                self.resize_split(col, row);
                return true;
            }
            MouseEventKind::Up(MouseButton::Left) => self.split_resize = false,
            _ => {}
        }
        false
    }

    /// Handle a mouse event, dispatching to whichever pane it lands in.
    pub fn on_mouse(&mut self, mouse: MouseEvent) {
        if self.try_overlay_mouse(mouse) {
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

        if self.try_chrome_mouse(mouse, col, row) {
            return;
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
                // The wheel scrolls a long (sub)menu by moving the highlight,
                // which the renderer keeps in view.
                MouseEventKind::ScrollDown => {
                    self.menu.down();
                    self.preview_current_theme();
                }
                MouseEventKind::ScrollUp => {
                    self.menu.up();
                    self.preview_current_theme();
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
        // Split panes: a click focuses the pane under the pointer, then maps the
        // click within it.
        if self.editor.is_split()
            && self.editor.focus_pane_at(self.layout.editor_region, col, row)
        {
            self.focus = Focus::Editor;
            self.editor_mouse(mouse);
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
        if self.settings.show_outline_dock
            && rect_contains(self.layout.outline_dock, col, row)
            && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        {
            let r = self.layout.outline_dock;
            self.outline_dock_click((row - r.y) as usize);
            return;
        }
        if self.show_test_panel
            && rect_contains(self.layout.test_panel, col, row)
            && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        {
            let r = self.layout.test_panel;
            let idx = (row - r.y) as usize;
            if idx < self.test_results.len() {
                self.jump_to_test(idx);
            }
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

    /// Set a view's horizontal scroll offset from a pointer column `col` on its
    /// horizontal scrollbar.
    fn hbar_drag(&mut self, target: HBar, col: u16) {
        let (rect, max) = match target {
            HBar::Editor => (self.layout.editor_hscrollbar, self.editor_hmax),
            HBar::Explorer => (self.layout.explorer_hscrollbar, self.explorer_hmax),
            HBar::Messages => (self.layout.messages_hscrollbar, self.messages_hmax),
            HBar::Bottom => (self.layout.bottom_hscrollbar, self.bottom_hmax),
        };
        let pos = crate::ui::scrollbar_pos_from_col(rect, col, max);
        match target {
            HBar::Editor => {
                if let Some(tab) = self.editor.active_tab_mut() {
                    tab.editor.set_offset_x(pos);
                }
            }
            HBar::Explorer => self.explorer_hscroll = pos,
            HBar::Messages => self.messages_hscroll = pos,
            HBar::Bottom => self.bottom_hscroll = pos,
        }
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

    /// Resize the split whose divider is under the pointer, as a percentage of
    /// that split's area (clamped to 10..=90).
    fn resize_split(&mut self, col: u16, row: u16) {
        let r = self.layout.editor_region;
        self.editor.resize_split_at(r, col, row);
    }

    /// Focus the split pane at in-order leaf index `idx` (clamped), making its tab
    /// active and directing cursor/mouse mapping at its rect.
    fn focus_split_pane(&mut self, idx: usize) {
        let last = self.editor.split_layout(self.layout.editor_region).len().saturating_sub(1);
        self.editor.focus_leaf(idx.min(last));
        self.focus = Focus::Editor;
    }

    fn editor_mouse(&mut self, mouse: MouseEvent) {
        self.focus = Focus::Editor;
        let area = self.layout.editor;
        let alt = mouse.modifiers.contains(crossterm::event::KeyModifiers::ALT);
        if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
            // Alt+click adds an extra caret; a plain click collapses to one.
            if alt {
                if let Some(t) = self.editor.active_tab_mut()
                    && let Some(pos) = t.editor.cursor_from_mouse(mouse.column, mouse.row, &area) {
                        t.editor.add_caret_at(pos);
                    }
                return;
            }
            if let Some(t) = self.editor.active_tab_mut() {
                t.preview = false;
                t.editor.clear_carets();
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
            let w = u16::try_from(tab.title().chars().count()).unwrap_or(u16::MAX);
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
        // The third-level submenu (drawn rightmost) takes top priority.
        if self.menu.subsubmenu_open() {
            let sd = self.layout.subsubmenu_dropdown;
            if rect_contains(sd, col, row) {
                let top = sd.y + 1;
                if let Some(items) = self.menu.subsubmenu_items() {
                    let offset =
                        crate::ui::dropdown_scroll(self.menu.subsub, sd.height.saturating_sub(2) as usize, items.len());
                    let idx = offset + row.saturating_sub(top) as usize;
                    if row >= top && idx < items.len() && !items[idx].is_separator() {
                        let action = items[idx].action;
                        self.run_action(action);
                        self.close_menu();
                    }
                }
                return;
            }
        }
        // The open submenu (drawn to the right of its parent) takes priority.
        if self.menu.submenu_open() {
            let sd = self.layout.submenu_dropdown;
            if rect_contains(sd, col, row) {
                let top = sd.y + 1;
                if let Some(items) = self.menu.submenu_items() {
                    let offset =
                        crate::ui::dropdown_scroll(self.menu.sub, sd.height.saturating_sub(2) as usize, items.len());
                    let idx = offset + row.saturating_sub(top) as usize;
                    if row >= top && idx < items.len() && !items[idx].is_separator() {
                        if items[idx].has_submenu() {
                            self.menu.highlight_sub(idx);
                            self.menu.right(); // opens the third level (nothing highlighted)
                        } else {
                            let action = items[idx].action;
                            self.run_action(action);
                            self.close_menu();
                        }
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
                let offset =
                    crate::ui::dropdown_scroll(self.menu.item, dd.height.saturating_sub(2) as usize, items.len());
                let idx = offset + row.saturating_sub(top) as usize;
                if row >= top && idx < items.len() && !items[idx].is_separator() {
                    if items[idx].has_submenu() {
                        self.menu.highlight_item(idx);
                        self.menu.right(); // opens the submenu (nothing highlighted)
                    } else {
                        let action = items[idx].action;
                        self.run_action(action);
                        self.close_menu();
                    }
                }
            }
            return;
        }
        // Clicked outside the bar and every dropdown: dismiss the menu.
        self.close_menu();
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
        let mut x = self.layout.menu.x;
        for (i, m) in menus().iter().enumerate() {
            let w = u16::try_from(m.title().chars().count()).unwrap_or(u16::MAX) + 2;
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
            if let Some(i) = self.top_menu_index_at(col)
                && self.menu.open != Some(i) {
                    self.menu.open_index(i);
                }
            // Left any open submenu: drop a live theme preview.
            self.revert_theme_preview();
            return;
        }
        // Third-level submenu (drawn rightmost) takes priority while open.
        if self.menu.subsubmenu_open() {
            let sd = self.layout.subsubmenu_dropdown;
            if rect_contains(sd, col, row) {
                let top = sd.y + 1;
                if let Some(items) = self.menu.subsubmenu_items() {
                    let offset =
                        crate::ui::dropdown_scroll(self.menu.subsub, sd.height.saturating_sub(2) as usize, items.len());
                    let idx = offset + row.saturating_sub(top) as usize;
                    if row >= top && idx < items.len() && !items[idx].is_separator() {
                        self.menu.subsub = Some(idx);
                    }
                }
                self.revert_theme_preview();
                return;
            }
        }
        if self.menu.submenu_open() {
            let sd = self.layout.submenu_dropdown;
            if rect_contains(sd, col, row) {
                let top = sd.y + 1;
                let mut previewed = false;
                if let Some(items) = self.menu.submenu_items() {
                    let offset =
                        crate::ui::dropdown_scroll(self.menu.sub, sd.height.saturating_sub(2) as usize, items.len());
                    let idx = offset + row.saturating_sub(top) as usize;
                    if row >= top && idx < items.len() && !items[idx].is_separator() {
                        self.menu.highlight_sub(idx);
                        if items[idx].has_submenu() {
                            self.menu.right(); // reveal the third level on hover
                        }
                        self.preview_menu_theme(items[idx].action);
                        previewed = true;
                    }
                }
                // Hovering a gap/separator in the submenu is not a choice.
                if !previewed {
                    self.revert_theme_preview();
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
                let offset =
                    crate::ui::dropdown_scroll(self.menu.item, dd.height.saturating_sub(2) as usize, items.len());
                let idx = offset + row.saturating_sub(top) as usize;
                if row >= top && idx < items.len() && !items[idx].is_separator() {
                    self.menu.highlight_item(idx);
                    if items[idx].has_submenu() {
                        self.menu.right(); // reveal the submenu on hover (nothing highlighted)
                    }
                }
            }
            // Pointer is over the parent dropdown, not a theme item.
            self.revert_theme_preview();
            return;
        }
        // Pointer is off every dropdown: drop a live theme preview.
        self.revert_theme_preview();
    }

    /// If `action` is a `view.theme:<name>` item, apply that theme live as a
    /// hover preview (without persisting it); [`Self::close_menu`] reverts to the
    /// committed theme when the menu closes.
    fn preview_menu_theme(&mut self, action: &str) {
        let Some(name) = action.strip_prefix("view.theme:") else {
            return;
        };
        // Already previewing this theme — avoid re-reading themes from disk.
        if self.theme_preview.as_deref() == Some(name) {
            return;
        }
        if let Some(theme) = Self::available_custom_themes().into_iter().find(|t| t.name == name) {
            crate::theme_model::apply(&theme);
            self.editor.refresh_theme();
            self.theme_preview = Some(name.to_string());
        }
    }

    /// Close the menu bar, reverting any live theme hover-preview to the
    /// committed theme (`settings.theme`).
    fn close_menu(&mut self) {
        self.menu.close();
        self.revert_theme_preview();
    }

    // ----- menu -----------------------------------------------------------

    fn menu_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Left => self.menu.left(),
            KeyCode::Right => {
                self.menu.right();
                self.preview_current_theme();
            }
            KeyCode::Up => {
                self.menu.up();
                self.preview_current_theme();
            }
            KeyCode::Down => {
                self.menu.down();
                self.preview_current_theme();
            }
            KeyCode::Enter => {
                if let Some(action) = self.menu.enter() {
                    self.run_action(action);
                    self.close_menu();
                }
            }
            KeyCode::Esc => {
                if self.menu.subsub_open {
                    self.menu.subsub_open = false;
                    self.menu.subsub = None;
                } else if self.menu.sub.is_some() {
                    self.menu.sub = None;
                    self.revert_theme_preview();
                } else {
                    self.close_menu();
                }
            }
            KeyCode::F(10) => self.close_menu(),
            // Type-ahead: a plain letter jumps to the next matching item.
            KeyCode::Char(c) if !Self::ctrl(&key) && !Self::alt(&key) => {
                self.menu.type_ahead(c);
                self.preview_current_theme();
            }
            _ => {}
        }
    }

    /// Preview the theme of the currently-highlighted submenu item, if it is a
    /// `view.theme:<name>` entry (for keyboard navigation, mirroring hover).
    fn preview_current_theme(&mut self) {
        if let (Some(sidx), Some(items)) = (self.menu.sub, self.menu.submenu_items())
            && let Some(it) = items.get(sidx) {
                self.preview_menu_theme(it.action);
            }
    }

    /// Revert a live theme hover/keyboard preview to the committed theme.
    fn revert_theme_preview(&mut self) {
        if self.theme_preview.take().is_some() {
            Self::apply_saved_theme(&self.settings.theme);
            self.editor.refresh_theme();
        }
    }

    // ----- theme chooser --------------------------------------------------

    /// Custom themes available to choose from: those installed in the user's
    /// themes directory first (so they win on a name clash), then the themes
    /// bundled into the binary.
    fn available_custom_themes() -> Vec<crate::theme::CustomTheme> {
        let mut themes = Settings::themes_dir()
            .map(|d| crate::theme_model::load_custom_themes(&d))
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
        crate::theme_model::apply(&theme);
        self.editor.refresh_theme();
        self.settings.theme.clone_from(&theme.name);
        // The committed theme is now the baseline; no preview to revert.
        self.theme_preview = None;
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

    // ----- locale ---------------------------------------------------------

    /// Apply the locale with the given `code` (from the View → Locale submenu),
    /// persist it, and update the UI language. Unknown codes are ignored.
    fn set_locale_by_code(&mut self, code: &str) {
        let Some(loc) = crate::locale_model::by_code(code) else {
            return;
        };
        rust_i18n::set_locale(loc.code);
        self.settings.locale = loc.code.to_string();
        self.status = t!("status.locale", locale = loc.code).to_string();
    }

    // ----- keymap ---------------------------------------------------------

    /// Apply the keymap with the given `id` (from the View → Keymap submenu),
    /// persist it, and reset per-keymap session state. Unknown ids are ignored.
    fn set_keymap(&mut self, id: &str) {
        let Some(km) = crate::keymap_model::by_id(id) else {
            return;
        };
        self.settings.keymap = km.id.to_string();
        self.reset_keymap_modes();
        self.status = t!("status.keymap", keymap = km.id).to_string();
    }

    // ----- time zone ------------------------------------------------------

    /// Apply the time zone with the given canonical `name` (from the View → Time
    /// Zone submenu), persist it, and update the app-wide active zone. Unknown
    /// names are ignored.
    fn set_time_zone_by_name(&mut self, name: &str) {
        if crate::time_zone_model::set_active(name) {
            self.settings.time_zone = name.to_string();
            self.status = t!("status.time_zone", zone = name).to_string();
        }
    }

    /// Reset per-keymap session state (Emacs chord prefix, Vim mode/command line)
    /// so a freshly chosen keymap starts clean — Vim begins in Normal mode.
    fn reset_keymap_modes(&mut self) {
        self.emacs_prefix = false;
        self.vim_insert = false;
        self.vim_cmd = None;
        self.spacemacs_insert = false;
        self.spacemacs_leader = None;
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
        if let Some(rc) = self.recent_chooser.take()
            && let Some(path) = rc.entries.get(rc.selected).cloned() {
                self.with_jump(|s| {
                    s.open_path(&path, false);
                    s.focus = Focus::Editor;
                });
            }
    }

    fn recent_mouse(&mut self, mouse: MouseEvent) {
        if let Some(idx) = self.chooser_row(mouse)
            && let Some(rc) = self.recent_chooser.as_mut()
                && idx < rc.entries.len() {
                    // A click selects the row and opens it (no live preview to
                    // justify a two-step interaction).
                    rc.selected = idx;
                    self.open_selected_recent();
                }
    }

    /// Open the recent-locations (jump list) chooser, listing the position
    /// history most-recent first with consecutive duplicates removed.
    fn open_location_chooser(&mut self) {
        let mut entries: Vec<Location> = Vec::new();
        for loc in self.nav_history.iter().rev() {
            if entries.last() != Some(loc) {
                entries.push(loc.clone());
            }
        }
        if entries.is_empty() {
            self.status = t!("status.no_recent_locations").to_string();
            return;
        }
        self.location_chooser = Some(LocationChooser { entries, selected: 0 });
    }

    fn location_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(lc) = self.location_chooser.as_mut() {
                    let n = lc.entries.len();
                    lc.selected = (lc.selected + n - 1) % n;
                }
            }
            KeyCode::Down => {
                if let Some(lc) = self.location_chooser.as_mut() {
                    lc.selected = (lc.selected + 1) % lc.entries.len();
                }
            }
            KeyCode::Enter => self.open_selected_location(),
            KeyCode::Esc => {
                self.location_chooser = None;
            }
            _ => {}
        }
    }

    /// Jump to the highlighted location and close the chooser.
    fn open_selected_location(&mut self) {
        if let Some(lc) = self.location_chooser.take()
            && let Some(loc) = lc.entries.get(lc.selected).cloned() {
                self.navigate_to(&loc);
            }
    }

    fn location_mouse(&mut self, mouse: MouseEvent) {
        if let Some(idx) = self.chooser_row(mouse)
            && let Some(lc) = self.location_chooser.as_mut()
                && idx < lc.entries.len() {
                    lc.selected = idx;
                    self.open_selected_location();
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
        if let Some(p) = self.nerd_palette.as_mut()
            && p.select_at(row, col) {
                self.insert_selected_glyph();
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

    /// Open the table editor on the active buffer, parsed as CSV or TSV (per the
    /// file extension; CSV by default). Warns when there is no editable buffer.
    fn open_edit_table(&mut self) {
        let Some(tab) = self.editor.active_tab() else {
            self.messages.warn(t!("msg.edit_table_no_buffer").to_string());
            return;
        };
        if tab.is_image() {
            self.messages.warn(t!("msg.edit_table_no_buffer").to_string());
            return;
        }
        let tsv = tab
            .path
            .as_ref()
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("tsv"));
        let text = tab.text();
        self.edit_table = Some(crate::edit_table::Grid::from_text(&text, tsv));
    }

    /// Route a key to the open table editor and act on its outcome.
    fn edit_table_key(&mut self, key: KeyEvent) {
        let page = usize::from(self.layout.edit_table.height).saturating_sub(1).max(1);
        let outcome = match self.edit_table.as_mut() {
            Some(grid) => grid.handle_key(key, page),
            None => return,
        };
        match outcome {
            crate::edit_table::Outcome::Close => self.edit_table = None,
            crate::edit_table::Outcome::Save => self.save_edit_table(),
            crate::edit_table::Outcome::Consumed => {}
        }
    }

    /// Serialize the table editor back into the active buffer and save it,
    /// reusing the normal file-save flow (which handles Save As when untitled).
    fn save_edit_table(&mut self) {
        let Some(text) = self.edit_table.as_ref().map(crate::edit_table::Grid::to_text) else {
            return;
        };
        if let Some(tab) = self.editor.active_tab_mut() {
            tab.editor.set_content(&text);
            tab.dirty = true;
        }
        if let Some(grid) = self.edit_table.as_mut() {
            grid.mark_saved();
        }
        self.run_action("file.save");
    }

    /// Open the outline editor on the active buffer (parsed as an indented
    /// outline). Warns when there is no editable buffer.
    fn open_edit_outline(&mut self) {
        let Some(tab) = self.editor.active_tab() else {
            self.messages.warn(t!("msg.edit_outline_no_buffer").to_string());
            return;
        };
        if tab.is_image() {
            self.messages.warn(t!("msg.edit_outline_no_buffer").to_string());
            return;
        }
        let text = tab.text();
        self.edit_outline = Some(crate::edit_outline::Tree::from_text(&text));
    }

    /// Route a key to the open outline editor and act on its outcome.
    fn edit_outline_key(&mut self, key: KeyEvent) {
        let page = usize::from(self.layout.edit_outline.height).saturating_sub(1).max(1);
        let outcome = match self.edit_outline.as_mut() {
            Some(tree) => tree.handle_key(key, page),
            None => return,
        };
        match outcome {
            crate::edit_outline::Outcome::Close => self.edit_outline = None,
            crate::edit_outline::Outcome::Save => self.save_edit_outline(),
            crate::edit_outline::Outcome::Consumed => {}
        }
    }

    /// Serialize the outline editor back into the active buffer and save it,
    /// reusing the normal file-save flow (which handles Save As when untitled).
    fn save_edit_outline(&mut self) {
        let Some(text) = self.edit_outline.as_ref().map(crate::edit_outline::Tree::to_text) else {
            return;
        };
        if let Some(tab) = self.editor.active_tab_mut() {
            tab.editor.set_content(&text);
            tab.dirty = true;
        }
        if let Some(tree) = self.edit_outline.as_mut() {
            tree.mark_saved();
        }
        self.run_action("file.save");
    }

    /// Dispatch a `tools.edit_*` edit-surface action. Returns `true` if handled.
    fn open_edit_surface(&mut self, action: &str) -> bool {
        match action {
            "tools.edit_table" => self.open_edit_table(),
            "tools.edit_outline" => self.open_edit_outline(),
            "tools.edit_sql" => self.open_edit_sql(),
            "tools.edit_json" => self.open_edit_value(crate::edit_value::Format::Json),
            "tools.edit_yaml" => self.open_edit_value(crate::edit_value::Format::Yaml),
            "tools.edit_bytes" => self.open_edit_bytes(),
            _ => return false,
        }
        true
    }

    /// Open the SQL statement editor on the active buffer.
    fn open_edit_sql(&mut self) {
        let Some(tab) = self.editor.active_tab() else {
            self.messages.warn(t!("msg.edit_sql_no_buffer").to_string());
            return;
        };
        if tab.is_image() {
            self.messages.warn(t!("msg.edit_sql_no_buffer").to_string());
            return;
        }
        let text = tab.text();
        self.edit_sql = Some(crate::edit_sql::Editor::from_text(&text));
    }

    /// Route a key to the open SQL editor and act on its outcome.
    fn edit_sql_key(&mut self, key: KeyEvent) {
        let page = usize::from(self.layout.edit_sql.height).saturating_sub(1).max(1);
        let outcome = match self.edit_sql.as_mut() {
            Some(editor) => editor.handle_key(key, page),
            None => return,
        };
        match outcome {
            crate::edit_sql::Outcome::Close => self.edit_sql = None,
            crate::edit_sql::Outcome::Save => self.save_edit_sql(),
            crate::edit_sql::Outcome::Consumed => {}
        }
    }

    /// Serialize the SQL editor back into the active buffer and save it.
    fn save_edit_sql(&mut self) {
        let Some(text) = self.edit_sql.as_ref().map(crate::edit_sql::Editor::to_text) else {
            return;
        };
        if let Some(tab) = self.editor.active_tab_mut() {
            tab.editor.set_content(&text);
            tab.dirty = true;
        }
        if let Some(editor) = self.edit_sql.as_mut() {
            editor.mark_saved();
        }
        self.run_action("file.save");
    }

    /// Open the structured-value editor (JSON or YAML) on the active buffer.
    /// Warns when there is no buffer or it does not parse in `format`.
    fn open_edit_value(&mut self, format: crate::edit_value::Format) {
        let Some(tab) = self.editor.active_tab() else {
            self.messages.warn(t!("msg.edit_value_no_buffer").to_string());
            return;
        };
        if tab.is_image() {
            self.messages.warn(t!("msg.edit_value_no_buffer").to_string());
            return;
        }
        match crate::edit_value::Tree::from_text(&tab.text(), format) {
            Some(tree) => self.edit_value = Some(tree),
            None => self.messages.warn(t!("msg.edit_value_parse").to_string()),
        }
    }

    /// Route a key to the open structured-value editor and act on its outcome.
    fn edit_value_key(&mut self, key: KeyEvent) {
        let page = usize::from(self.layout.edit_value.height).saturating_sub(1).max(1);
        let outcome = match self.edit_value.as_mut() {
            Some(tree) => tree.handle_key(key, page),
            None => return,
        };
        match outcome {
            crate::edit_value::Outcome::Close => self.edit_value = None,
            crate::edit_value::Outcome::Save => self.save_edit_value(),
            crate::edit_value::Outcome::Consumed => {}
        }
    }

    /// Serialize the structured-value editor back into the active buffer and save.
    fn save_edit_value(&mut self) {
        let Some(text) = self.edit_value.as_ref().map(crate::edit_value::Tree::to_text) else {
            return;
        };
        if let Some(tab) = self.editor.active_tab_mut() {
            tab.editor.set_content(&text);
            tab.dirty = true;
        }
        if let Some(tree) = self.edit_value.as_mut() {
            tree.mark_saved();
        }
        self.run_action("file.save");
    }

    /// Open the byte (hex) editor on the active buffer's bytes. Warns when there
    /// is no editable buffer.
    fn open_edit_bytes(&mut self) {
        let Some(tab) = self.editor.active_tab() else {
            self.messages.warn(t!("msg.edit_bytes_no_buffer").to_string());
            return;
        };
        if tab.is_image() {
            self.messages.warn(t!("msg.edit_bytes_no_buffer").to_string());
            return;
        }
        self.edit_bytes = Some(crate::edit_bytes::Hex::from_bytes(tab.text().into_bytes()));
    }

    /// Route a key to the open byte editor and act on its outcome.
    fn edit_bytes_key(&mut self, key: KeyEvent) {
        let page = usize::from(self.layout.edit_bytes.height).saturating_sub(1).max(1);
        let outcome = match self.edit_bytes.as_mut() {
            Some(hex) => hex.handle_key(key, page),
            None => return,
        };
        match outcome {
            crate::edit_bytes::Outcome::Close => self.edit_bytes = None,
            crate::edit_bytes::Outcome::Save => self.save_edit_bytes(),
            crate::edit_bytes::Outcome::Consumed => {}
        }
    }

    /// Write the byte editor's bytes back into the active buffer and save. Bytes
    /// are decoded lossily to UTF-8 for the text buffer.
    fn save_edit_bytes(&mut self) {
        let Some(bytes) = self.edit_bytes.as_ref().map(|h| h.to_bytes().to_vec()) else {
            return;
        };
        let text = String::from_utf8_lossy(&bytes).into_owned();
        if let Some(tab) = self.editor.active_tab_mut() {
            tab.editor.set_content(&text);
            tab.dirty = true;
        }
        if let Some(hex) = self.edit_bytes.as_mut() {
            hex.mark_saved();
        }
        self.run_action("file.save");
    }

    /// Generate a QR code from the current selection (or the cursor's line) and
    /// show it in a read-only overlay. Warns when there is nothing to encode.
    fn open_qrcode(&mut self) {
        let text = self.editor.active_tab_mut().map(|tab| {
            tab.editor
                .get_selection_text()
                .filter(|s| !s.trim().is_empty())
                .or_else(|| tab.lines().get(tab.editor.cursor_line()).cloned())
                .unwrap_or_default()
                .trim()
                .to_string()
        });
        match text.as_deref().and_then(crate::qr_tool::render) {
            Some(art) => self.qrcode = Some(art),
            None => self.messages.warn(t!("msg.qrcode_empty").to_string()),
        }
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

    // ----- Media-type (MIME) picker ---------------------------------------

    /// Open the media-type picker, pre-selected to the active file's type when
    /// its extension is recognized.
    fn open_media_type_panel(&mut self) {
        let ext = self
            .editor
            .active_tab()
            .and_then(|t| t.path.as_ref())
            .and_then(|p| p.extension())
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.media_type_panel = Some(crate::media_type::Panel::open_for_extension(&ext));
    }

    fn media_type_key(&mut self, key: KeyEvent) {
        let page = (self.layout.media_type_panel.height as usize).max(1);
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.media_type_panel.as_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.media_type_panel.as_mut() {
                    p.down();
                }
            }
            KeyCode::PageUp => {
                if let Some(p) = self.media_type_panel.as_mut() {
                    p.page_up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(p) = self.media_type_panel.as_mut() {
                    p.page_down(page);
                }
            }
            KeyCode::Backspace => {
                if let Some(p) = self.media_type_panel.as_mut() {
                    p.backspace();
                }
            }
            KeyCode::Char(c) => {
                if let Some(p) = self.media_type_panel.as_mut() {
                    p.push(c);
                }
            }
            // Enter inserts and keeps the panel open; Esc closes it.
            KeyCode::Enter => self.insert_selected_media_type(),
            KeyCode::Esc => self.media_type_panel = None,
            _ => {}
        }
    }

    fn media_type_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.media_type_panel;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row_in_view = (mouse.row - r.y) as usize;
        if let Some(p) = self.media_type_panel.as_mut() {
            let idx = p.scroll + row_in_view;
            if p.select_index(idx) {
                self.insert_selected_media_type();
            }
        }
    }

    /// Insert the highlighted media type (e.g. `image/png`) into the active editor
    /// (leaving the panel open). No-op when there is no editable buffer.
    fn insert_selected_media_type(&mut self) {
        let Some(entry) = self.media_type_panel.as_ref().and_then(crate::media_type::Panel::selected_entry)
        else {
            return;
        };
        let media_type = entry.media_type.to_string();
        let area = self.layout.editor;
        if self.editor.insert_str(&media_type, area) {
            self.status = t!("status.media_type_inserted", media_type = media_type).to_string();
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

    /// The selected text, or the whole active buffer when nothing is selected.
    fn selected_or_all_text(&mut self) -> String {
        let selection = self.editor.active_tab_mut().and_then(|t| t.editor.get_selection_text());
        match selection {
            Some(s) if !s.trim().is_empty() => s,
            _ => self.editor.active_tab().map(|t| t.editor.get_content()).unwrap_or_default(),
        }
    }

    /// Summarize the selection (or the whole file when nothing is selected) with
    /// `claude`; the result opens in a new editor tab.
    fn ai_summarize(&mut self) {
        let text = self.selected_or_all_text();
        self.ai_to_new_tab("Summarize this text.", &text, &t!("menu.item.ai.summarize"));
    }

    /// Explain the selection (or the whole file when nothing is selected) with
    /// `claude`; the result opens in a new editor tab.
    fn ai_explain(&mut self) {
        let text = self.selected_or_all_text();
        self.ai_to_new_tab("Explain this text.", &text, &t!("menu.item.ai.explain"));
    }

    /// Define a word with `claude`; the result opens in a new editor tab. The
    /// input is the selection if there is one; otherwise the word under the
    /// cursor, or the next word when the cursor sits between words. Never the
    /// whole buffer.
    fn ai_define(&mut self) {
        let text = self.selected_or_word_text();
        self.ai_to_new_tab("Define this text.", &text, &t!("menu.item.ai.define"));
    }

    /// The selection, else the word at the cursor, else the next word after it.
    /// Returns an empty string when there is no editable buffer or no word ahead.
    fn selected_or_word_text(&mut self) -> String {
        let Some(tab) = self.editor.active_tab_mut() else {
            return String::new();
        };
        if let Some(sel) = tab.editor.get_selection_text()
            && !sel.trim().is_empty() {
                return sel;
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
        let dest = if self.settings.ai_diff_review {
            AiDest::Diff { tab: tab_idx, target }
        } else {
            AiDest::Replace { tab: tab_idx, target }
        };
        if let Some(rx) = self.spawn_ai(prompt, &text) {
            self.ai_replace = Some(AiReplace { rx, dest, label: label.to_string() });
            self.status = t!("status.ai_running", action = label).to_string();
        }
    }

    /// Launch `claude -p <prompt>` over `text`, capturing its full output to open
    /// in a new editor tab when it finishes (Summarize/Explain/Define).
    fn ai_to_new_tab(&mut self, prompt: &str, text: &str, label: &str) {
        if self.ai_replace.is_some() {
            self.status = t!("status.ai_busy").to_string();
            return;
        }
        if let Some(rx) = self.spawn_ai(prompt, text) {
            self.ai_replace = Some(AiReplace { rx, dest: AiDest::NewTab, label: label.to_string() });
            self.status = t!("status.ai_running", action = label).to_string();
        }
    }

    /// Spawn the configured AI command (see [`Settings::ai_command`]) over `text`,
    /// returning a receiver for its captured stdout (or `None` after reporting an
    /// empty input or a spawn failure). The reader thread sends one [`AiMsg`] when
    /// the CLI exits. The command is built from the `ai_command` template so the
    /// AI menu can drive any assistant CLI, not just `claude`.
    fn spawn_ai(&mut self, prompt: &str, text: &str) -> Option<std::sync::mpsc::Receiver<AiMsg>> {
        if text.trim().is_empty() {
            self.status = t!("status.ai_no_input").to_string();
            return None;
        }
        self.spawn_ai_cmd(prompt, text)
    }

    /// The spawn core shared by [`Self::spawn_ai`] and the chat panel: write `text`
    /// to a temp file, expand the `ai_command` template, and run it in the
    /// background. Unlike `spawn_ai` it does **not** reject empty input — a chat
    /// turn may carry no editor context — so callers must guard that themselves.
    fn spawn_ai_cmd(&mut self, prompt: &str, text: &str) -> Option<std::sync::mpsc::Receiver<AiMsg>> {
        let tmp = std::env::temp_dir().join(format!("vix-ai-{}.txt", std::process::id()));
        if std::fs::write(&tmp, text).is_err() {
            self.status = t!("status.ai_no_input").to_string();
            return None;
        }
        let path = tmp.display().to_string();
        let cmd = self.settings.ai_command_line(prompt, &path);
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
                return None;
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
        Some(rx)
    }

    /// Drain a finished AI task and apply its result. Called once per event-loop
    /// iteration; cheap when none is running.
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
                    if let (AiDest::Panel, Some(panel)) = (ar.dest, self.ai_panel.as_mut()) {
                        panel.busy = false;
                        panel.push(crate::ai_panel::Role::Error, t!("status.ai_failed", action = &ar.label));
                    }
                    self.status = t!("status.ai_failed", action = &ar.label).to_string();
                    if !matches!(ar.dest, AiDest::Panel) {
                        self.messages.error(t!("status.ai_failed", action = ar.label));
                    }
                    return;
                }
                match ar.dest {
                    AiDest::Replace { tab, target } => self.apply_ai_replace(tab, target, text),
                    AiDest::NewTab => {
                        self.editor.new_tab_with_content(text);
                        self.focus = Focus::Editor;
                    }
                    AiDest::Panel => {
                        if let Some(panel) = self.ai_panel.as_mut() {
                            panel.busy = false;
                            panel.push(crate::ai_panel::Role::Assistant, text);
                        }
                    }
                    AiDest::Diff { tab, target } => self.open_ai_diff(tab, target, text),
                }
                self.status = t!("status.ai_done", action = &ar.label).to_string();
                if !matches!(ar.dest, AiDest::Panel) {
                    self.messages.info(t!("status.ai_done", action = ar.label));
                }
            }
            AiMsg::Failed => {
                if let (AiDest::Panel, Some(panel)) = (ar.dest, self.ai_panel.as_mut()) {
                    panel.busy = false;
                    panel.push(crate::ai_panel::Role::Error, t!("status.ai_failed", action = &ar.label));
                }
                self.status = t!("status.ai_failed", action = &ar.label).to_string();
                if !matches!(ar.dest, AiDest::Panel) {
                    self.messages.error(t!("status.ai_failed", action = ar.label));
                }
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
        // Keep the caret in range: set_content leaves the old cursor untouched,
        // which can point past the end when the replacement is shorter.
        let caret = tab.editor.get_cursor();
        tab.editor.set_cursor(caret);
        tab.dirty = true;
    }

    /// Whether a background AI transform is in progress.
    #[must_use]
    pub fn ai_replace_running(&self) -> bool {
        self.ai_replace.is_some()
    }

    // ----- AI chat panel --------------------------------------------------

    /// Open the AI chat panel (a persistent conversation with the configured
    /// assistant), or focus it if already open. Seeds the input with the current
    /// selection so "ask about this" is one keystroke away.
    fn open_ai_panel(&mut self) {
        if self.ai_panel.is_none() {
            let mut panel = crate::ai_panel::Panel::open();
            if let Some(sel) = self.editor.active_tab_mut().and_then(|t| t.editor.get_selection_text())
                && !sel.trim().is_empty()
            {
                panel.input = sel;
            }
            self.ai_panel = Some(panel);
        }
    }

    /// Handle a key while the AI chat panel is open. Enter sends the current line;
    /// Esc closes; arrows / `PageUp` / `PageDown` scroll the transcript; Alt+T opens
    /// the last reply in a new tab; Alt+C copies it to the clipboard.
    fn ai_panel_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.ai_panel = None,
            KeyCode::Enter => self.ai_panel_send(),
            KeyCode::Up => {
                if let Some(p) = self.ai_panel.as_mut() {
                    p.scroll_up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.ai_panel.as_mut() {
                    p.scroll_down();
                }
            }
            KeyCode::PageUp => {
                if let Some(p) = self.ai_panel.as_mut() {
                    for _ in 0..10 {
                        p.scroll_up();
                    }
                }
            }
            KeyCode::PageDown => {
                if let Some(p) = self.ai_panel.as_mut() {
                    for _ in 0..10 {
                        p.scroll_down();
                    }
                }
            }
            KeyCode::Char('t') if Self::alt(&key) => self.ai_panel_last_to_tab(),
            KeyCode::Char('c') if Self::alt(&key) => self.ai_panel_copy_last(),
            KeyCode::Backspace => {
                if let Some(p) = self.ai_panel.as_mut() {
                    p.input.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(p) = self.ai_panel.as_mut() {
                    p.input.push(c);
                }
            }
            _ => {}
        }
    }

    /// Send the panel's current input line to the assistant, with the prior
    /// conversation supplied as stdin context, and mark the panel busy until the
    /// reply lands in [`Self::poll_ai_replace`].
    fn ai_panel_send(&mut self) {
        if self.ai_replace.is_some() {
            self.status = t!("status.ai_busy").to_string();
            return;
        }
        let Some(panel) = self.ai_panel.as_mut() else { return };
        let prompt = panel.input.trim().to_string();
        if prompt.is_empty() {
            return;
        }
        let context = panel.context();
        panel.input.clear();
        panel.push(crate::ai_panel::Role::User, prompt.clone());
        panel.busy = true;
        if let Some(rx) = self.spawn_ai_cmd(&prompt, &context) {
            let label = t!("menu.ai").to_string();
            self.ai_replace = Some(AiReplace { rx, dest: AiDest::Panel, label });
        } else if let Some(panel) = self.ai_panel.as_mut() {
            panel.busy = false;
            panel.push(crate::ai_panel::Role::Error, t!("msg.command_failed", error = "spawn").to_string());
        }
    }

    /// Open the panel's most recent assistant reply in a new editor tab.
    fn ai_panel_last_to_tab(&mut self) {
        if let Some(text) = self.ai_panel.as_ref().and_then(|p| p.last_assistant()).map(str::to_string) {
            self.editor.new_tab_with_content(&text);
            self.ai_panel = None;
            self.focus = Focus::Editor;
        }
    }

    /// Copy the panel's most recent assistant reply to the system clipboard.
    fn ai_panel_copy_last(&mut self) {
        let Some(text) = self.ai_panel.as_ref().and_then(|p| p.last_assistant()).map(str::to_string) else {
            return;
        };
        if let Some(tab) = self.editor.active_tab_mut() {
            let _ = tab.editor.set_clipboard(&text);
            self.status = t!("status.ai_copied").to_string();
        }
    }

    // ----- Integrated terminal --------------------------------------------

    /// Toggle the integrated terminal: open a shell on a PTY, or close it if open.
    fn toggle_terminal(&mut self) {
        if self.terminal.is_some() {
            self.terminal = None;
            return;
        }
        let area = self.layout.editor;
        let rows = area.height.max(1);
        let cols = area.width.max(1);
        let shell = std::env::var("SHELL").unwrap_or_else(|_| {
            if cfg!(windows) { "cmd.exe".to_string() } else { "/bin/sh".to_string() }
        });
        match crate::terminal::Terminal::open(&shell, &self.root, rows, cols) {
            Ok(term) => self.terminal = Some(term),
            Err(e) => self.messages.error(t!("msg.command_failed", error = e).to_string()),
        }
    }

    /// Handle a key while the terminal is focused. `Ctrl+]` closes it; every other
    /// key is forwarded to the shell. A dead shell closes on the next key.
    fn terminal_key(&mut self, key: KeyEvent) {
        let close = Self::ctrl(&key) && matches!(key.code, KeyCode::Char(']'));
        if close {
            self.terminal = None;
            return;
        }
        let Some(term) = self.terminal.as_mut() else { return };
        if !term.alive() {
            self.terminal = None;
            return;
        }
        term.send_key(key);
    }

    /// Drain the terminal: close it once the shell has exited. Called each loop.
    pub fn poll_terminal(&mut self) {
        if self.terminal.as_ref().is_some_and(|t| !t.alive()) {
            self.terminal = None;
            self.status = t!("status.terminal_closed").to_string();
        }
    }

    /// Whether the integrated terminal is open (drives the fast poll cadence).
    #[must_use]
    pub fn terminal_running(&self) -> bool {
        self.terminal.is_some()
    }

    // ----- AI diff review (Annotate / Improve) ----------------------------

    /// The open AI diff review, if any (for rendering).
    #[must_use]
    pub fn ai_diff_review(&self) -> Option<&crate::ai_diff::Review> {
        self.ai_diff.as_ref().map(|s| &s.review)
    }

    /// Open an accept/reject diff review for an AI transform whose `new_text`
    /// would replace `target` in tab `tab_idx`. No-ops (with a status note) when
    /// the assistant proposed no change.
    fn open_ai_diff(&mut self, tab_idx: usize, target: AiTarget, new_text: &str) {
        let Some(tab) = self.editor.tabs.get(tab_idx) else { return };
        let old = match target {
            AiTarget::Whole => tab.editor.get_content(),
            AiTarget::Range(start, end) => tab.editor.get_content_slice(start, end),
        };
        match crate::ai_diff::Review::from_texts(&old, new_text) {
            Some(review) => self.ai_diff = Some(AiDiffState { review, tab: tab_idx, target }),
            None => self.status = t!("status.ai_no_change").to_string(),
        }
    }

    /// Handle a key while the AI diff review is open. Enter applies the accepted
    /// hunks; Esc discards the whole proposal; ↑/↓ move between hunks; Space
    /// toggles the highlighted hunk; `a`/`r` accept/reject all.
    fn ai_diff_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.ai_diff = None,
            KeyCode::Enter => self.ai_diff_apply(),
            KeyCode::Up => {
                if let Some(s) = self.ai_diff.as_mut() {
                    s.review.prev();
                }
            }
            KeyCode::Down => {
                if let Some(s) = self.ai_diff.as_mut() {
                    s.review.next();
                }
            }
            KeyCode::Char(' ') => {
                if let Some(s) = self.ai_diff.as_mut() {
                    s.review.toggle();
                }
            }
            KeyCode::Char('a') => {
                if let Some(s) = self.ai_diff.as_mut() {
                    s.review.set_all(true);
                }
            }
            KeyCode::Char('r') => {
                if let Some(s) = self.ai_diff.as_mut() {
                    s.review.set_all(false);
                }
            }
            _ => {}
        }
    }

    /// Apply the reviewed result (accepted hunks applied, rejected ones reverted)
    /// to its target as a single undoable edit, then close the review.
    fn ai_diff_apply(&mut self) {
        let Some(state) = self.ai_diff.take() else { return };
        let text = state.review.result();
        self.apply_ai_replace(state.tab, state.target, &text);
        self.status = t!("status.ai_done", action = t!("menu.ai")).to_string();
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
                    .map(|t| crate::vcard_parser::parse(&t).display_name())
                    .filter(|n| n != "(unnamed)")
                    .unwrap_or_else(|| {
                        path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
                    });
                contacts.push(crate::contact_panel::Contact { name, path });
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
            Ok(text) => self.vcard = Some(VcardPanel::open(crate::vcard_parser::parse(&text))),
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
    fn gather_file_info(&self) -> crate::file_information_panel::FileInfo {
        use crate::file_information_panel::FileInfo;
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
                if let Ok(modified) = meta.modified()
                    && let Ok(d) = modified.duration_since(std::time::UNIX_EPOCH) {
                        info.modified_secs = i64::try_from(d.as_secs()).ok();
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

    // ----- Text Information panel -----------------------------------------

    /// Open the Text Information panel over the selection, or the whole buffer
    /// when nothing is selected.
    fn open_text_info(&mut self) {
        let text = self.selected_or_all_text();
        let stats = crate::text_information_panel::analyze(&text);
        self.text_info = Some(TextInfoPanel::open(&stats));
    }

    fn text_info_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.text_info.as_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.text_info.as_mut() {
                    p.down();
                }
            }
            KeyCode::Enter => self.insert_selected_text_info(),
            KeyCode::Esc => self.text_info = None,
            _ => {}
        }
    }

    fn text_info_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.text_info;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row_in_view = (mouse.row - r.y) as usize;
        if let Some(p) = self.text_info.as_mut()
            && p.select_index(row_in_view) {
                self.insert_selected_text_info();
            }
    }

    /// Insert the highlighted value into the active editor (leaving the panel open).
    fn insert_selected_text_info(&mut self) {
        let Some(p) = self.text_info.as_ref() else {
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

    // ----- Markdown preview -----------------------------------------------

    /// Open a read-only Markdown preview of the active buffer.
    fn open_markdown_preview(&mut self) {
        let Some(text) = self.editor.active_tab().filter(|t| !t.is_image()).map(Tab::text) else {
            return;
        };
        self.markdown_preview = Some(MarkdownPreview::open(&text));
    }

    fn markdown_preview_key(&mut self, key: KeyEvent) {
        let page = (self.layout.editor.height as usize).max(1).saturating_sub(2);
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.markdown_preview.as_mut() {
                    p.up(1);
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.markdown_preview.as_mut() {
                    p.down(1);
                }
            }
            KeyCode::PageUp => {
                if let Some(p) = self.markdown_preview.as_mut() {
                    p.up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(p) = self.markdown_preview.as_mut() {
                    p.down(page);
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => self.markdown_preview = None,
            _ => {}
        }
    }

    // ----- Snippets -------------------------------------------------------

    /// The active buffer's media type (from its file extension), if recognized.
    fn active_media_type(&self) -> Option<String> {
        let ext = self.editor.active_tab()?.path.as_ref()?.extension()?.to_string_lossy().into_owned();
        crate::media_type::for_extension(&ext).map(|m| m.media_type.to_string())
    }

    /// Rebuild the in-scope snippet library (bundled + file scopes) for the active
    /// buffer's media type. Cached by media type; `force` rebuilds regardless.
    fn refresh_snippet_library(&mut self, force: bool) {
        let media = self.active_media_type();
        let key = media.clone().unwrap_or_default();
        if !force && self.snippet_library_key.as_deref() == Some(key.as_str()) {
            return;
        }
        let files = crate::snippets::load_scoped(media.as_deref(), &self.root, &self.settings.project_snippets);
        self.snippet_library = crate::snippets::merge(crate::snippets::bundled(), files);
        self.snippet_library_key = Some(key);
    }

    /// Open the Snippets picker (rebuilding the library for the active buffer).
    fn open_snippets(&mut self) {
        self.refresh_snippet_library(true);
        self.snippets = Some(crate::snippets::Picker::new());
    }

    fn snippets_key(&mut self, key: KeyEvent) {
        let page = (self.layout.snippets.height as usize).max(1);
        let lib = &self.snippet_library;
        match key.code {
            KeyCode::Up => {
                if let Some(p) = self.snippets.as_mut() {
                    p.up(1);
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.snippets.as_mut() {
                    p.down(1, lib);
                }
            }
            KeyCode::PageUp => {
                if let Some(p) = self.snippets.as_mut() {
                    p.up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(p) = self.snippets.as_mut() {
                    p.down(page, lib);
                }
            }
            KeyCode::Backspace => {
                if let Some(p) = self.snippets.as_mut() {
                    p.backspace();
                }
            }
            KeyCode::Char(c) => {
                if let Some(p) = self.snippets.as_mut() {
                    p.push(c);
                }
            }
            KeyCode::Enter => self.insert_selected_snippet(),
            KeyCode::Esc => self.snippets = None,
            _ => {}
        }
    }

    fn snippets_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.snippets;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row = (mouse.row - r.y) as usize;
        let idx = self.snippets.as_ref().map_or(0, |p| p.scroll) + row;
        let hit = self.snippets.as_mut().is_some_and(|p| p.select_index(idx, &self.snippet_library));
        if hit {
            self.insert_selected_snippet();
        }
    }

    /// Insert the highlighted snippet's body at the cursor and close the picker,
    /// starting a tabstop session if the snippet has fields.
    fn insert_selected_snippet(&mut self) {
        let body = self
            .snippets
            .as_ref()
            .and_then(|p| p.selected_library_index(&self.snippet_library))
            .and_then(|i| self.snippet_library.get(i))
            .map(|s| s.body.clone());
        let Some(body) = body else { return };
        self.snippets = None;
        self.insert_snippet_body(&body);
    }

    /// If the word just before the cursor is a snippet prefix, replace it with the
    /// snippet body and arm a tabstop session. Returns `true` if it expanded.
    fn expand_snippet_prefix(&mut self) -> bool {
        self.refresh_snippet_library(false);
        let Some(tab) = self.editor.active_tab() else { return false };
        if tab.is_image() {
            return false;
        }
        let cursor = tab.editor.get_cursor();
        let chars: Vec<char> = tab.editor.get_content().chars().collect();
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        let mut start = cursor;
        while start > 0 && chars.get(start - 1).copied().is_some_and(is_word) {
            start -= 1;
        }
        if start == cursor {
            return false;
        }
        let word: String = chars[start..cursor].iter().collect();
        let Some(snippet) = crate::snippets::find_by_prefix(&self.snippet_library, &word) else {
            return false;
        };
        let body = snippet.body.clone();
        // Select the prefix word so the snippet insertion replaces it.
        let area = self.editor_view();
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_selection_range(start, cursor);
            t.editor.focus(&area);
        }
        self.insert_snippet_body(&body);
        true
    }

    /// Insert a snippet `body` (with `$1`/`${1:…}`/`$0` tabstops) at the cursor.
    /// Places the cursor at the first tabstop and arms a [`SnippetSession`] so Tab
    /// walks the rest; with no tabstops it is a plain insert.
    fn insert_snippet_body(&mut self, body: &str) {
        let parsed = crate::snippet_tool::parse(body);
        let area = self.layout.editor;
        // Insertion replaces any active selection, so the body begins at the
        // selection start (used by prefix expansion); otherwise at the cursor.
        let base = self.editor.active_tab_mut().map_or(0, |t| match t.editor.get_selection() {
            Some(sel) if !sel.is_empty() => sel.sorted().0,
            _ => t.editor.get_cursor(),
        });
        if !self.editor.insert_str(&parsed.text, area) {
            return;
        }
        self.status = t!("status.snippet_inserted").to_string();
        if parsed.stops.is_empty() {
            return;
        }
        let stops: Vec<(usize, usize)> =
            parsed.stops.iter().map(|s| (base + s.start, base + s.end)).collect();
        let single = stops.len() == 1;
        self.snippet_session = Some(SnippetSession { stops, index: 0 });
        self.snippet_goto(0);
        // A lone `$0` is just a caret placement — no session to navigate.
        if single {
            self.snippet_session = None;
        }
    }

    /// Move to tabstop `index`: select its placeholder (or place a bare caret).
    fn snippet_goto(&mut self, index: usize) {
        let Some((start, end)) = self.snippet_session.as_ref().and_then(|s| s.stops.get(index).copied())
        else {
            return;
        };
        let area = self.editor_view();
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_selection_range(start, end);
            t.editor.focus(&area);
        }
    }

    /// Advance to the next snippet tabstop, shifting later stops by the net length
    /// change the user made at the current one. Ends the session after the last.
    fn snippet_tab(&mut self) {
        let cursor = self.editor.active_tab().map_or(0, |t| t.editor.get_cursor());
        let next = {
            let Some(sess) = self.snippet_session.as_mut() else { return };
            let cur_end = sess.stops[sess.index].1;
            // Shift every later stop by the net length change the user made at the
            // current field (cursor vs. the field's original end).
            for (s, e) in &mut sess.stops {
                if *s >= cur_end {
                    if cursor >= cur_end {
                        let add = cursor - cur_end;
                        *s += add;
                        *e += add;
                    } else {
                        let sub = cur_end - cursor;
                        *s = s.saturating_sub(sub);
                        *e = e.saturating_sub(sub);
                    }
                }
            }
            let next = sess.index + 1;
            if next >= sess.stops.len() {
                None
            } else {
                sess.index = next;
                Some(next)
            }
        };
        match next {
            Some(i) => self.snippet_goto(i),
            None => self.snippet_session = None,
        }
    }

    /// Whether a snippet tabstop session is active (so Tab navigates fields).
    fn snippet_active(&self) -> bool {
        self.snippet_session.is_some()
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
            if let Ok(out) = std::process::Command::new("du").arg("-sh").arg(&droot).output()
                && out.status.success() {
                    let text = String::from_utf8_lossy(&out.stdout);
                    if let Some(size) = text.split_whitespace().next() {
                        let _ = dtx.send(DashMsg::Disk(size.to_string()));
                    }
                }
        });

        let ftx = tx.clone();
        let froot = root.clone();
        std::thread::spawn(move || {
            let _ = ftx.send(DashMsg::Files(count_files(&froot)));
        });

        std::thread::spawn(move || {
            let _ = tx.send(DashMsg::Commits(crate::git::commit_count(&root).unwrap_or(0)));
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
        let entries: Vec<crate::outline_panel::Entry> = self
            .editor
            .active_tab()
            .filter(|t| !t.is_image())
            .map(|t| {
                crate::palette::symbols(&t.text())
                    .into_iter()
                    .map(|s| crate::outline_panel::Entry { kind: s.kind, name: s.name, line: s.line })
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
        let Some(line) = self.outline.as_ref().and_then(crate::outline_panel::Outline::selected_line)
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

    /// Toggle the persistent outline sidebar and persist the preference.
    fn toggle_outline_dock(&mut self) {
        self.settings.show_outline_dock = !self.settings.show_outline_dock;
        self.refresh_outline_dock();
    }

    /// Rebuild the outline sidebar's symbol list when the active buffer changes,
    /// and keep its highlight on the symbol nearest the cursor. Cheap between
    /// changes (cached by tab + revision). Called once per event-loop iteration.
    pub fn refresh_outline_dock(&mut self) {
        if !self.settings.show_outline_dock {
            self.outline_dock = None;
            self.outline_dock_key = None;
            return;
        }
        let key = self.editor.active_tab().filter(|t| !t.is_image()).map(|t| (self.editor.active, t.editor.revision()));
        let Some(key) = key else {
            self.outline_dock = None;
            self.outline_dock_key = None;
            return;
        };
        if self.outline_dock_key != Some(key) {
            self.outline_dock_key = Some(key);
            let entries: Vec<crate::outline_panel::Entry> = self
                .editor
                .active_tab()
                .map(|t| {
                    crate::palette::symbols(&t.text())
                        .into_iter()
                        .map(|s| crate::outline_panel::Entry { kind: s.kind, name: s.name, line: s.line })
                        .collect()
                })
                .unwrap_or_default();
            self.outline_dock = if entries.is_empty() { None } else { Some(Outline::new(entries)) };
        }
        if let Some(o) = self.outline_dock.as_mut() {
            let cur = self.editor.active_tab().map_or(1, |t| t.editor.cursor_line() + 1);
            o.select_nearest(cur);
        }
    }

    /// Jump the editor to the outline-sidebar row at viewport index `row`.
    fn outline_dock_click(&mut self, row: usize) {
        let Some(o) = self.outline_dock.as_mut() else { return };
        let idx = o.scroll + row;
        if idx >= o.entries.len() {
            return;
        }
        o.selected = idx;
        let Some(line) = o.selected_line() else { return };
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
                // Score every command: when the query is empty, order by recency
                // (recently-run first) then catalog order; otherwise rank by fuzzy
                // score with recency as a tiebreak.
                let recent_rank = |action: &str| self.command_recents.iter().position(|a| a == action);
                let mut scored: Vec<(i32, usize, Entry)> = Vec::new();
                for (cat_idx, (label_key, action)) in palette::COMMANDS.iter().enumerate() {
                    let label = t!(*label_key).to_string();
                    let entry = Entry {
                        label: format!("> {label}"),
                        action: PAction::RunCommand((*action).to_string()),
                    };
                    if query.is_empty() {
                        // Recents (rank 0..) sort above everything; non-recents keep
                        // catalog order after them.
                        let key = recent_rank(action)
                            .map_or(1000 + i32::try_from(cat_idx).unwrap_or(0), |r| {
                                i32::try_from(r).unwrap_or(0)
                            });
                        scored.push((-key, cat_idx, entry));
                    } else if let Some(score) = palette::fuzzy_score(&label, &query) {
                        let boost = recent_rank(action)
                            .map_or(0, |r| (12 - i32::try_from(r).unwrap_or(12)).max(0));
                        scored.push((score + boost, cat_idx, entry));
                    }
                }
                scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
                entries = scored.into_iter().map(|(_, _, e)| e).collect();
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
                if let Some(tab) = self.editor.active_tab()
                    && !tab.is_image() {
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
            PMode::WorkspaceSymbols => {
                entries = self.workspace_symbol_entries(&query);
            }
        }
        if let Some(p) = self.palette.as_mut() {
            if p.selected >= entries.len() {
                p.selected = entries.len().saturating_sub(1);
            }
            p.entries = entries;
        }
    }

    /// Scan the workspace's indexed files for declaration symbols matching
    /// `query`, returning palette entries that open the file at the symbol's
    /// line. An empty query returns nothing (the workspace is too large to list
    /// every symbol); results and files scanned are capped to stay responsive.
    fn workspace_symbol_entries(&self, query: &str) -> Vec<Entry> {
        const MAX_RESULTS: usize = 200;
        const MAX_FILE_BYTES: u64 = 512 * 1024;
        let mut entries = Vec::new();
        if query.trim().is_empty() {
            return entries;
        }
        for path in &self.file_index {
            if entries.len() >= MAX_RESULTS {
                break;
            }
            // Skip obviously-binary or oversized files cheaply by extension/size.
            if is_image_path(path) {
                continue;
            }
            if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_FILE_BYTES) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            let rel = path.strip_prefix(&self.root).unwrap_or(path).to_string_lossy().into_owned();
            for sym in palette::symbols(&text) {
                if !palette::fuzzy_match(&sym.name, query) {
                    continue;
                }
                entries.push(Entry {
                    label: format!("@ {}  ·  {}:{}", sym.name, rel, sym.line),
                    action: PAction::OpenFile(path.clone(), Some((sym.line, 1))),
                });
                if entries.len() >= MAX_RESULTS {
                    break;
                }
            }
        }
        entries
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

    /// Record `action` as the most-recently-run palette command (deduped,
    /// most-recent first, capped).
    fn record_command_recent(&mut self, action: &str) {
        const MAX_RECENTS: usize = 12;
        self.command_recents.retain(|a| a != action);
        self.command_recents.insert(0, action.to_string());
        self.command_recents.truncate(MAX_RECENTS);
        // Persist so the order survives across sessions.
        self.settings.command_recents.clone_from(&self.command_recents);
        let _ = self.settings.save();
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
            PAction::RunCommand(action) => {
                self.record_command_recent(&action);
                self.run_action(&action);
            }
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
        let (index, total) = self.find_with(&re, forward);
        let msg = Self::match_status(index, total);
        if let Some(s) = self.search.as_mut() {
            s.status = msg;
        } else {
            self.status = msg;
        }
    }

    /// Format a "N of M matches" status (or the no-matches message).
    fn match_status(index: usize, total: usize) -> String {
        if total == 0 {
            t!("status.no_matches").to_string()
        } else {
            t!("status.match_of", index = index, total = total).to_string()
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
        let (index, total) = self.find_with(&re, forward);
        self.status = Self::match_status(index, total);
    }

    /// Mark every match of `re` in the active buffer and move the cursor to the
    /// next/previous one (wrapping at the ends). Returns `(index, total)` where
    /// `index` is the 1-based position of the landed-on match; zero matches
    /// clears the marks and returns `(0, 0)`.
    fn find_with(&mut self, re: &Regex, forward: bool) -> (usize, usize) {
        let area = self.editor_view();
        let Some(t) = self.editor.active_tab_mut() else {
            return (0, 0);
        };
        let content = t.text();
        let matches = crate::find_panel::matches(&content, re);
        if matches.is_empty() {
            t.editor.remove_marks();
            return (0, 0);
        }
        let marks: Vec<(usize, usize, &str)> =
            matches.iter().map(|(s, e)| (*s, *e, SEARCH_MARK)).collect();
        t.editor.set_marks(marks);

        // Pick the next/previous match relative to the cursor, wrapping around
        // the ends (first match after the last, last match before the first).
        let cur = t.editor.get_cursor();
        let target_idx = if forward {
            matches
                .iter()
                .position(|(s, _)| *s > cur)
                .unwrap_or(0) // past the last match: wrap to the first
        } else {
            matches
                .iter()
                .rposition(|(s, _)| *s < cur)
                .unwrap_or(matches.len() - 1) // before the first: wrap to the last
        };
        let target = matches[target_idx];
        t.editor.set_cursor(target.0);
        t.editor.set_selection(Some(Selection::new(target.0, target.1)));
        t.editor.focus(&area);
        (target_idx + 1, matches.len())
    }

    fn clear_marks(&mut self) {
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.remove_marks();
        }
    }

    fn end_search(&mut self) {
        // Sticky highlights stay visible after the Find box closes; otherwise
        // clear them. Either way the search bar itself goes away.
        if !self.settings.sticky_search_highlight {
            self.clear_marks();
        }
        self.search = None;
    }

    /// Toggle the search-match highlights for the active buffer: clear them if
    /// any are shown, otherwise re-highlight the last search term.
    fn toggle_search_highlight(&mut self) {
        let has = self.editor.active_tab().is_some_and(|t| t.editor.has_marks());
        if has {
            self.clear_marks();
            self.status = t!("status.highlights_off").into();
            return;
        }
        let Some(pat) = self.last_search.clone() else {
            self.status = t!("status.no_matches").into();
            return;
        };
        let Ok(re) = Regex::new(&pat) else {
            return;
        };
        let count = self.highlight_all(&re);
        self.status = if count == 0 {
            t!("status.no_matches").into()
        } else {
            t!("status.matches", count = count).to_string()
        };
    }

    /// Highlight every match of `re` in the active buffer without moving the
    /// cursor. Returns the match count.
    fn highlight_all(&mut self, re: &Regex) -> usize {
        let Some(t) = self.editor.active_tab_mut() else {
            return 0;
        };
        let content = t.text();
        let matches = crate::find_panel::matches(&content, re);
        if matches.is_empty() {
            t.editor.remove_marks();
            return 0;
        }
        let marks: Vec<(usize, usize, &str)> =
            matches.iter().map(|(s, e)| (*s, *e, SEARCH_MARK)).collect();
        t.editor.set_marks(marks);
        matches.len()
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

    /// A left click inside the find / replace box: a Case/Word/Regex toggle
    /// button flips that option; an Once/Ask/All button runs that replacement; a
    /// click on a field row focuses that field. Clicks elsewhere are ignored so
    /// the box stays open.
    fn search_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let (col, row) = (mouse.column, mouse.row);
        let hit = |r: Rect| rect_contains(r, col, row);

        // Toggle buttons.
        if hit(self.layout.search_case)
            || hit(self.layout.search_word)
            || hit(self.layout.search_regex)
        {
            if let Some(s) = self.search.as_mut() {
                if hit(self.layout.search_case) {
                    s.case_sensitive = !s.case_sensitive;
                } else if hit(self.layout.search_word) {
                    s.whole_word = !s.whole_word;
                } else {
                    s.regex = !s.regex;
                }
            }
            if self.search.as_ref().is_some_and(|s| !s.interactive) {
                self.find_step(true);
            }
            return;
        }
        // Replace action buttons.
        if hit(self.layout.search_once) {
            self.replace_once();
            return;
        }
        if hit(self.layout.search_ask) {
            self.begin_query_replace();
            return;
        }
        if hit(self.layout.search_all) {
            self.replace_all();
            return;
        }

        // Field rows: Find is row 0; in replace mode Replace is row 2.
        let r = self.layout.search;
        if !rect_contains(r, col, row) {
            return;
        }
        let rel = row - r.y;
        if let Some(s) = self.search.as_mut() {
            if rel == 0 {
                s.field = Field::Query;
            } else if s.replacing && rel == 2 {
                s.field = Field::Replace;
            }
        }
    }

    /// Replace the next match at or after the cursor once, then highlight the
    /// following match (the find box stays open).
    fn replace_once(&mut self) {
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
        let regex = sb.regex;
        let template =
            if regex { crate::find_panel::unescape(&sb.replace) } else { sb.replace.clone() };
        let area = self.editor_view();
        let replaced = {
            let Some(t) = self.editor.active_tab_mut() else {
                return;
            };
            let from = t.editor.get_cursor();
            match next_match_from(t, &re, from) {
                Some(current) => {
                    let resume = do_replace(t, &re, regex, &template, current);
                    t.dirty = true;
                    t.preview = false;
                    if let Some(next) = next_match_from(t, &re, resume) {
                        highlight_match(t, next.0, next.1, area);
                    }
                    true
                }
                None => false,
            }
        };
        if let Some(s) = self.search.as_mut() {
            s.status = if replaced {
                t!("status.replaced", count = 1).to_string()
            } else {
                t!("status.qr_no_matches").to_string()
            };
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
        let (new_text, count) = crate::find_panel::replace_all(&text, &re, use_regex, &replacement);
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
    /// Request LSP formatting of the active document (or selection, when one
    /// exists — via range formatting).
    fn lsp_format(&mut self) {
        let Some(path) = self.active_path() else { return };
        if !self.lsp.handles(&path) {
            self.status = t!("status.lsp_inactive").to_string();
            return;
        }
        let tab_size = u32::try_from(self.settings.tab_width).unwrap_or(4);
        let enc = self.lsp.encoding_for(&path);
        let sel = self.editor.active_tab_mut().and_then(|t| {
            let s = t.editor.get_selection()?;
            if s.is_empty() {
                return None;
            }
            let code = t.editor.code_ref();
            Some((
                char_to_lsp_pos(code, s.start.min(s.end), enc),
                char_to_lsp_pos(code, s.start.max(s.end), enc),
            ))
        });
        match sel {
            Some((start, end)) => self.lsp.request_range_formatting(&path, start, end, tab_size),
            None => self.lsp.request_formatting(&path, tab_size),
        }
    }

    /// Apply LSP text edits to the active buffer (highest position first, so
    /// earlier offsets stay valid), then re-anchor the caret.
    fn apply_lsp_edits(&mut self, edits: &[(crate::lsp_core::Range, String)]) {
        let Some(path) = self.active_path() else { return };
        let enc = self.lsp.encoding_for(&path);
        let Some(t) = self.editor.active_tab_mut() else { return };
        // Resolve every range to char offsets first, then apply tail-to-head.
        let mut resolved: Vec<(usize, usize, String)> = {
            let code = t.editor.code_ref();
            edits
                .iter()
                .map(|(r, text)| {
                    let start = lsp_pos_to_char(code, r.start.line, r.start.character, enc);
                    let end = lsp_pos_to_char(code, r.end.line, r.end.character, enc);
                    (start.min(end), start.max(end), text.clone())
                })
                .collect()
        };
        resolved.sort_by_key(|e| std::cmp::Reverse(e.0));
        let mut content: Vec<char> = t.editor.get_content().chars().collect();
        for (start, end, text) in resolved {
            let (a, b) = (start.min(content.len()), end.min(content.len()));
            if a <= b {
                content.splice(a..b, text.chars());
            }
        }
        let new_content: String = content.into_iter().collect();
        let caret = t.editor.get_cursor().min(new_content.chars().count());
        t.editor.set_content(&new_content);
        t.editor.set_cursor(caret);
        t.dirty = true;
        t.preview = false;
        self.status = t!("status.formatted").to_string();
    }

    /// Request the implementation(s) of the symbol under the cursor (LSP).
    fn goto_implementation(&mut self) {
        if let Some(path) = self.active_path()
            && self.lsp.handles(&path)
        {
            let (line, character) = self.cursor_lsp_position(&path);
            self.lsp.request_implementation(&path, line, character);
        } else {
            self.status = t!("status.lsp_inactive").to_string();
        }
    }

    /// Request the type definition of the symbol under the cursor (LSP).
    fn goto_type_definition(&mut self) {
        if let Some(path) = self.active_path()
            && self.lsp.handles(&path)
        {
            let (line, character) = self.cursor_lsp_position(&path);
            self.lsp.request_type_definition(&path, line, character);
        } else {
            self.status = t!("status.lsp_inactive").to_string();
        }
    }

    /// Request all references to the symbol under the cursor (LSP).
    fn find_references(&mut self) {
        if let Some(path) = self.active_path()
            && self.lsp.handles(&path)
        {
            let (line, character) = self.cursor_lsp_position(&path);
            self.lsp.request_references(&path, line, character);
        } else {
            self.status = t!("status.lsp_inactive").to_string();
        }
    }

    /// Begin an LSP rename: capture the cursor position and prompt for the new
    /// name (seeded with the symbol under the cursor).
    fn begin_lsp_rename(&mut self) {
        let Some(path) = self.active_path() else {
            self.status = t!("status.lsp_inactive").to_string();
            return;
        };
        if !self.lsp.handles(&path) {
            self.status = t!("status.lsp_inactive").to_string();
            return;
        }
        let (line, character) = self.cursor_lsp_position(&path);
        self.rename_at = Some((path, line, character));
        let seed = self.symbol_under_cursor().unwrap_or_default();
        self.prompt = Some(
            Prompt::new(PromptKind::LspRename, t!("prompt.lsp_rename").to_string()).with_input(seed),
        );
    }

    /// Request the linked-editing ranges at the cursor (LSP).
    fn request_linked_editing(&mut self) {
        if let Some(path) = self.active_path()
            && self.lsp.handles(&path)
        {
            let (line, character) = self.cursor_lsp_position(&path);
            self.lsp.request_linked_editing(&path, line, character);
        } else {
            self.status = t!("status.lsp_inactive").to_string();
        }
    }

    /// On a linked-editing response: store the ranges (as char offsets) and
    /// prompt for the shared replacement text, seeded with the current text.
    fn begin_linked_edit(&mut self, ranges: &[crate::lsp_core::Range]) {
        let Some(path) = self.active_path() else { return };
        let enc = self.lsp.encoding_for(&path);
        let Some(t) = self.editor.active_tab() else { return };
        let (offsets, seed) = {
            let code = t.editor.code_ref();
            let offsets: Vec<(usize, usize)> = ranges
                .iter()
                .map(|r| {
                    let s = lsp_pos_to_char(code, r.start.line, r.start.character, enc);
                    let e = lsp_pos_to_char(code, r.end.line, r.end.character, enc);
                    (s.min(e), s.max(e))
                })
                .collect();
            let seed = offsets.first().map(|&(s, e)| code.slice(s, e)).unwrap_or_default();
            (offsets, seed)
        };
        self.linked_ranges = Some(offsets);
        self.prompt =
            Some(Prompt::new(PromptKind::LinkedEdit, t!("prompt.linked_edit").to_string()).with_input(seed));
    }

    /// Replace every captured linked-editing range with `text` (highest offset
    /// first so earlier offsets stay valid).
    fn apply_linked_edit(&mut self, text: &str) {
        let Some(mut ranges) = self.linked_ranges.take() else { return };
        ranges.sort_by_key(|&(s, _)| std::cmp::Reverse(s));
        let Some(t) = self.editor.active_tab_mut() else { return };
        let mut chars: Vec<char> = t.editor.get_content().chars().collect();
        for (s, e) in ranges {
            let (a, b) = (s.min(chars.len()), e.min(chars.len()));
            if a <= b {
                chars.splice(a..b, text.chars());
            }
        }
        let new: String = chars.into_iter().collect();
        let caret = t.editor.get_cursor().min(new.chars().count());
        t.editor.set_content(&new);
        t.editor.set_cursor(caret);
        t.dirty = true;
        t.preview = false;
        self.status = t!("status.linked_edited").to_string();
    }

    /// Apply a rename `WorkspaceEdit`: edit open buffers in place (marking them
    /// dirty) and rewrite closed files on disk.
    fn apply_workspace_edit(&mut self, edits: &[crate::lsp::FileEdits]) {
        let mut files = 0usize;
        for (path, file_edits) in edits {
            if file_edits.is_empty() {
                continue;
            }
            let enc = self.lsp.encoding_for(path);
            if let Some(tab) = self
                .editor
                .tabs
                .iter_mut()
                .find(|t| t.path.as_deref() == Some(path.as_path()))
            {
                let text = tab.editor.get_content();
                let new = apply_edits_to_text(&text, enc, file_edits);
                if new != text {
                    let caret = tab.editor.get_cursor().min(new.chars().count());
                    tab.editor.set_content(&new);
                    tab.editor.set_cursor(caret);
                    tab.dirty = true;
                    tab.preview = false;
                    files += 1;
                }
            } else if let Ok(text) = std::fs::read_to_string(path) {
                let new = apply_edits_to_text(&text, enc, file_edits);
                if new != text && std::fs::write(path, new).is_ok() {
                    files += 1;
                }
            }
        }
        self.refresh_git();
        self.status = t!("status.renamed_in", n = files).to_string();
    }

    /// Request the document symbols (outline) for the active file (LSP).
    fn request_document_symbols(&mut self) {
        if let Some(path) = self.active_path()
            && self.lsp.handles(&path)
        {
            self.lsp.request_document_symbols(&path);
        } else {
            self.status = t!("status.lsp_inactive").to_string();
        }
    }

    /// Request signature help at the cursor (LSP).
    fn lsp_signature_help(&mut self) {
        if let Some(path) = self.active_path()
            && self.lsp.handles(&path)
        {
            let (line, character) = self.cursor_lsp_position(&path);
            self.lsp.request_signature_help(&path, line, character);
        } else {
            self.status = t!("status.lsp_inactive").to_string();
        }
    }

    /// Show document symbols (all in the active file) in the static-results panel.
    fn show_document_symbols(&mut self, syms: &[(u32, u32, String)]) {
        let Some(path) = self.active_path() else { return };
        let mut hits: Vec<Hit> = syms
            .iter()
            .map(|(line, character, name)| {
                let line1 = *line as usize + 1;
                Hit {
                    path: path.clone(),
                    line: line1,
                    col: *character as usize + 1,
                    display: format!("{name}  :{line1}"),
                }
            })
            .collect();
        if hits.is_empty() {
            self.status = t!("status.no_symbols").to_string();
            return;
        }
        hits.sort_by_key(|h| h.line);
        let mut ps = WorkspaceSearch::new(false);
        ps.static_results = true;
        ps.status = t!("status.symbols_n", n = hits.len()).to_string();
        ps.hits = hits;
        self.workspace_search = Some(ps);
    }

    /// Show workspace symbols in the static-results panel (Enter jumps).
    fn show_workspace_symbols(&mut self, syms: &[(PathBuf, u32, u32, String)]) {
        let mut hits: Vec<Hit> = syms
            .iter()
            .map(|(path, line, character, name)| {
                let rel = path
                    .strip_prefix(&self.root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .into_owned();
                let line1 = *line as usize + 1;
                Hit {
                    path: path.clone(),
                    line: line1,
                    col: *character as usize + 1,
                    display: format!("{name}  {rel}:{line1}"),
                }
            })
            .collect();
        if hits.is_empty() {
            self.status = t!("status.no_symbols").to_string();
            return;
        }
        hits.sort_by(|a, b| a.display.cmp(&b.display));
        let mut ps = WorkspaceSearch::new(false);
        ps.static_results = true;
        ps.status = t!("status.symbols_n", n = hits.len()).to_string();
        ps.hits = hits;
        self.workspace_search = Some(ps);
    }

    /// Show LSP reference locations in the static-results panel (Enter jumps).
    fn show_references(&mut self, locs: &[(PathBuf, u32, u32)]) {
        let mut hits: Vec<Hit> = locs
            .iter()
            .map(|(path, line, character)| {
                let rel = path
                    .strip_prefix(&self.root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .into_owned();
                let line1 = *line as usize + 1;
                Hit {
                    path: path.clone(),
                    line: line1,
                    col: *character as usize + 1,
                    display: format!("{rel}:{line1}"),
                }
            })
            .collect();
        if hits.is_empty() {
            self.status = t!("status.no_references").to_string();
            return;
        }
        hits.sort_by(|a, b| a.display.cmp(&b.display));
        let mut ps = WorkspaceSearch::new(false);
        ps.static_results = true;
        ps.status = t!("status.references_n", n = hits.len()).to_string();
        ps.hits = hits;
        self.workspace_search = Some(ps);
    }

    fn goto_definition(&mut self) {
        if let Some(path) = self.active_path()
            && self.lsp.handles(&path) {
                let (line, character) = self.cursor_lsp_position(&path);
                self.lsp.request_definition(&path, line, character);
                return;
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

    /// Open a panel listing every current LSP diagnostic across the workspace;
    /// Enter on a row jumps to it. Reuses the static-results search overlay.
    fn open_diagnostics_panel(&mut self) {
        use crate::lsp_core::Severity;
        let mut hits: Vec<Hit> = Vec::new();
        for (path, diags) in self.lsp.all_diagnostics() {
            let rel = path
                .strip_prefix(&self.root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned();
            for d in diags {
                let sev = match d.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                    Severity::Information => "info",
                    Severity::Hint => "hint",
                };
                let line = d.range.start.line as usize + 1;
                let msg: String = d.message.lines().next().unwrap_or("").chars().take(100).collect();
                hits.push(Hit {
                    path: path.clone(),
                    line,
                    col: d.range.start.character as usize + 1,
                    display: format!("{rel}:{line}: [{sev}] {msg}"),
                });
            }
        }
        if hits.is_empty() {
            self.status = t!("status.no_diagnostics").to_string();
            return;
        }
        hits.sort_by(|a, b| a.display.cmp(&b.display));
        let mut ps = WorkspaceSearch::new(false);
        ps.static_results = true;
        ps.status = t!("status.diagnostics_n", n = hits.len()).to_string();
        ps.hits = hits;
        self.workspace_search = Some(ps);
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

        // Compute (but do not yet write) the replacement for each file, so the
        // user can preview and confirm before anything touches disk.
        let mut plan: Vec<(PathBuf, String)> = Vec::new();
        let mut lines: Vec<String> = Vec::new();
        let mut replaced = 0usize;
        for path in &paths {
            let Some(content) = self.current_text(path) else {
                continue;
            };
            let (new, count) = crate::find_panel::replace_all(&content, &re, use_regex, &replacement);
            if count == 0 || new == content {
                continue;
            }
            let rel = path.strip_prefix(&self.root).unwrap_or(path);
            lines.push(format!("{} ({count})", rel.display()));
            plan.push((path.clone(), new));
            replaced += count;
        }
        if plan.is_empty() {
            if let Some(p) = self.workspace_search.as_mut() {
                p.status = t!("status.replaced_in_files", replaced = 0, files = 0).to_string();
            }
            return;
        }
        self.replace_confirm = Some(ReplaceConfirm { plan, replaced, lines, scroll: 0 });
    }

    /// Apply a confirmed project-wide replace: write every planned file and keep
    /// open buffers in sync, then refresh the search and report the totals.
    fn apply_replace_confirm(&mut self) {
        let Some(rc) = self.replace_confirm.take() else { return };
        let replaced = rc.replaced;
        let mut files = 0usize;
        for (path, new) in &rc.plan {
            if let Err(e) = std::fs::write(path, new) {
                self.messages.error(t!("msg.write_failed", path = path.display(), error = e).to_string());
                continue;
            }
            let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
            for tab in &mut self.editor.tabs {
                if tab.path.as_deref() == Some(canon.as_path()) {
                    tab.editor.set_content(new);
                    tab.dirty = false;
                }
            }
            files += 1;
        }
        self.run_workspace_search();
        let note = t!("status.replaced_in_files", replaced = replaced, files = files).to_string();
        if let Some(p) = self.workspace_search.as_mut() {
            p.status.clone_from(&note);
        }
        self.messages.info(note);
    }

    /// Handle a key in the project-wide replace preview: `y`/Enter applies,
    /// `n`/Esc cancels, arrows scroll the file list.
    fn replace_confirm_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y' | 'Y') | KeyCode::Enter => self.apply_replace_confirm(),
            KeyCode::Char('n' | 'N') | KeyCode::Esc => self.replace_confirm = None,
            KeyCode::Up => {
                if let Some(rc) = self.replace_confirm.as_mut() {
                    rc.scroll = rc.scroll.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if let Some(rc) = self.replace_confirm.as_mut() {
                    rc.scroll = (rc.scroll + 1).min(rc.lines.len().saturating_sub(1));
                }
            }
            _ => {}
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
            crate::find_panel::unescape(&sb.replace)
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
            // Alt+Enter inserts a newline in the multi-line git-commit prompt;
            // plain Enter still submits.
            KeyCode::Enter if Self::alt(&key) => {
                if let Some(p) = self.prompt.as_mut()
                    && matches!(p.kind, PromptKind::GitCommit)
                {
                    p.input.push('\n');
                }
            }
            KeyCode::Enter => self.accept_prompt(),
            KeyCode::Backspace => {
                if let Some(p) = self.prompt.as_mut() {
                    p.input.pop();
                }
            }
            // Alt+C / Alt+R toggle case / regex for the workspace→dock search.
            KeyCode::Char(c) if Self::alt(&key) => {
                if let Some(p) = self.prompt.as_mut()
                    && matches!(p.kind, PromptKind::SearchToDock) {
                        match c.to_ascii_lowercase() {
                            'c' => p.case_sensitive = !p.case_sensitive,
                            'r' => p.regex = !p.regex,
                            _ => {}
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
            PromptKind::GitEditDescription => self.git_edit_description(&prompt.input),
            PromptKind::GitDeleteBranch => self.git_delete_branch(&prompt.input),
            PromptKind::GitGrep => self.git_grep(&prompt.input),
            PromptKind::WorkspaceSymbol => {
                if let Some(path) = self.active_path()
                    && self.lsp.handles(&path)
                {
                    self.lsp.request_workspace_symbols(&path, prompt.input.trim());
                }
            }
            PromptKind::LspRename => {
                let new_name = prompt.input.trim().to_string();
                if let Some((path, line, character)) = self.rename_at.take()
                    && !new_name.is_empty()
                {
                    self.lsp.request_rename(&path, line, character, &new_name);
                }
            }
            PromptKind::LinkedEdit => self.apply_linked_edit(&prompt.input),
            PromptKind::ExplorerInclude => {
                let exclude = self.explorer.exclude_filter.clone();
                self.explorer.set_filter(prompt.input.trim(), &exclude);
            }
            PromptKind::ExplorerExclude => {
                let include = self.explorer.include_filter.clone();
                self.explorer.set_filter(&include, prompt.input.trim());
            }
            PromptKind::CompareFile => self.open_diff_with(prompt.input.trim()),
            PromptKind::SaveMacro => self.save_macro(prompt.input.trim()),
            PromptKind::OrgCapture => {
                let text = prompt.input.trim();
                if !text.is_empty() {
                    self.insert_content(&format!("* TODO {text}\n"));
                }
            }
            PromptKind::DebugRepl => {
                let expr = prompt.input.trim();
                if !expr.is_empty() {
                    self.dap.evaluate(expr);
                }
            }
            PromptKind::DebugWatch => {
                let expr = prompt.input.trim();
                if !expr.is_empty() {
                    self.dap_watches.push((expr.to_string(), String::new()));
                    self.dap.evaluate(expr);
                    self.show_debug_panel = true;
                }
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
        self.running_command = Some(RunningCommand { rx, child, label: cmd.to_string() });
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
                CmdMsg::Line(l) => {
                    if self.test_capture {
                        self.test_buffer.push(l.clone());
                    }
                    self.bottom_dock.push(l);
                }
                CmdMsg::Done(code) => {
                    let code = code.unwrap_or(-1);
                    self.bottom_dock.push(format!("[exit {code}]"));
                    self.status = t!("status.command_done", code = code).to_string();
                    let label = self.running_command.as_ref().map(|rc| rc.label.clone()).unwrap_or_default();
                    let note = t!("msg.command_finished", command = label, code = code).to_string();
                    if code == 0 {
                        self.messages.info(note);
                    } else {
                        self.messages.error(note);
                    }
                    done = true;
                }
            }
        }
        if done {
            self.running_command = None;
            if self.test_capture {
                self.finish_test_run();
            }
            // A finished command may have changed the working tree or HEAD (e.g.
            // git push/pull/checkout); refresh the cached git state.
            self.refresh_git();
        }
    }

    /// Run the configured test command, capturing its output to parse into a
    /// pass/fail tree when it finishes.
    fn run_tests(&mut self) {
        if self.running_command.is_some() {
            self.status = t!("status.command_busy").to_string();
            return;
        }
        self.test_capture = true;
        self.test_buffer.clear();
        self.show_test_panel = true;
        let cmd = self.settings.test_command.clone();
        self.run_command(&cmd);
    }

    /// Parse the captured test output, populate the panel, and report a summary.
    fn finish_test_run(&mut self) {
        self.test_capture = false;
        self.test_results = crate::test_runner::parse(&self.test_buffer.join("\n"));
        self.test_buffer.clear();
        self.test_selected = 0;
        let (pass, fail, ignore) = crate::test_runner::tally(&self.test_results);
        let note = t!("status.tests_done", pass = pass, fail = fail, ignore = ignore).to_string();
        if fail > 0 {
            self.messages.error(note.clone());
        } else {
            self.messages.info(note.clone());
        }
        self.status = note;
    }

    /// Jump to the highlighted failing test's source location, if known.
    fn jump_to_test(&mut self, idx: usize) {
        self.test_selected = idx;
        let Some((file, line)) = self.test_results.get(idx).and_then(|r| r.location.clone()) else {
            return;
        };
        let path = self.resolve(&file);
        if path.is_file() {
            self.with_jump(|s| {
                s.open_path(&path, false);
                let area = s.editor_view();
                s.editor.goto(line, None, area);
                s.focus = Focus::Editor;
            });
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
        self.save_session();
        if let Err(e) = self.settings.save() {
            self.messages
                .push(Level::Warn, t!("msg.settings_save_failed", error = e).to_string());
        }
    }
}

fn rect_contains(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
}

/// The top-level menu index for an `Alt+letter` mnemonic: Vix=0, File=1, Edit=2,
/// View=3 (Alt+I, since "Vix"/"View" both start with V), Tools=4, AI=5, Git=6,
/// Org=7, Debug=8, Help=9. `None` for any other letter.
fn menu_index_for_alt(c: char) -> Option<usize> {
    match c.to_ascii_lowercase() {
        'v' => Some(0),
        'f' => Some(1),
        'e' => Some(2),
        'i' => Some(3),
        't' => Some(4),
        'a' => Some(5),
        'g' => Some(6),
        'o' => Some(7),
        'd' => Some(8),
        'h' => Some(9),
        _ => None,
    }
}

/// The char offset within `code` of LSP position `(line, character)`, where
/// `character` is in the server's `enc` units. Out-of-range positions clamp.
fn lsp_pos_to_char(
    code: &crate::editor_core::code::Code,
    line: u32,
    character: u32,
    enc: crate::lsp_core::Encoding,
) -> usize {
    let line = line as usize;
    if line >= code.len_lines() {
        return code.len();
    }
    let line_start = code.line_to_char(line);
    let line_text = code.slice(line_start, line_start + code.line_len(line));
    line_start + crate::lsp_core::position::col_to_char(&line_text, character, enc)
}

/// Apply LSP text edits to a plain string, resolving each `(line, character)`
/// range to a char offset (encoding-aware) and splicing highest-offset-first so
/// earlier offsets stay valid. Used for rename edits to files that may not be
/// open in an editor tab.
fn apply_edits_to_text(
    text: &str,
    enc: crate::lsp_core::Encoding,
    edits: &[(crate::lsp_core::Range, String)],
) -> String {
    // Char offset at the start of each line (line N → offset), plus the line
    // texts, so a position resolves to `line_start[line] + col_to_char(..)`.
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut line_start = Vec::with_capacity(lines.len() + 1);
    let mut acc = 0usize;
    for l in &lines {
        line_start.push(acc);
        acc += l.chars().count();
    }
    line_start.push(acc); // total length, for a position past the last line
    let resolve = |line: u32, character: u32| -> usize {
        let li = line as usize;
        if li >= lines.len() {
            return acc;
        }
        line_start[li] + crate::lsp_core::position::col_to_char(lines[li], character, enc)
    };
    let mut resolved: Vec<(usize, usize, &str)> = edits
        .iter()
        .map(|(r, t)| {
            let s = resolve(r.start.line, r.start.character);
            let e = resolve(r.end.line, r.end.character);
            (s.min(e), s.max(e), t.as_str())
        })
        .collect();
    resolved.sort_by_key(|e| std::cmp::Reverse(e.0));
    let mut chars: Vec<char> = text.chars().collect();
    for (start, end, new_text) in resolved {
        let (a, b) = (start.min(chars.len()), end.min(chars.len()));
        if a <= b {
            chars.splice(a..b, new_text.chars());
        }
    }
    chars.into_iter().collect()
}

/// Convert a buffer char offset to an LSP `(line, character)` in `enc` units.
fn char_to_lsp_pos(
    code: &crate::editor_core::code::Code,
    char_offset: usize,
    enc: crate::lsp_core::Encoding,
) -> (u32, u32) {
    let offset = char_offset.min(code.len());
    let line = code.char_to_line(offset);
    let line_start = code.line_to_char(line);
    let line_text = code.slice(line_start, line_start + code.line_len(line));
    let character = crate::lsp_core::position::char_to_col(&line_text, offset - line_start, enc);
    (u32::try_from(line).unwrap_or(u32::MAX), character)
}

/// The LSP wire number for a diagnostic severity (Error=1 … Hint=4).
fn severity_number(sev: crate::lsp_core::Severity) -> u8 {
    use crate::lsp_core::Severity;
    match sev {
        Severity::Error => 1,
        Severity::Warning => 2,
        Severity::Information => 3,
        Severity::Hint => 4,
    }
}

/// The underline color for a diagnostic severity.
fn severity_color(sev: crate::lsp_core::Severity) -> ratatui::style::Color {
    use ratatui::style::Color;
    match sev {
        crate::lsp_core::Severity::Error => Color::Red,
        crate::lsp_core::Severity::Warning => Color::Yellow,
        crate::lsp_core::Severity::Information => Color::Cyan,
        crate::lsp_core::Severity::Hint => Color::Blue,
    }
}

/// The text of the HTML-palette row cell at `rel_col` (columns measured from the
/// row's left edge): the glyph, the entity name, or the code point. The column
/// bands track the row format rendered by `ui::draw_html_panel`
/// (`"  {glyph:2}  {name:26}  {code}"`).
fn html_cell_at(e: &crate::html_character_picker::Entity, rel_col: usize) -> String {
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
/// `t`'s buffer. Pure matching lives in [`crate::find_panel::next_match`].
fn next_match_from(t: &Tab, re: &Regex, from_char: usize) -> Option<(usize, usize)> {
    crate::find_panel::next_match(&t.text(), re, from_char)
}

/// Editor adapter: replace the single match at char offset `current.0` in `t`,
/// returning the char offset just past the inserted text (where searching should
/// resume). Pure replacement lives in [`crate::find_panel::replace_one`].
fn do_replace(t: &mut Tab, re: &Regex, regex: bool, template: &str, current: (usize, usize)) -> usize {
    match crate::find_panel::replace_one(&t.text(), re, regex, template, current.0) {
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
fn gutter_hex(mark: crate::git::LineMark) -> &'static str {
    match mark {
        crate::git::LineMark::Added => "#3fb950",
        crate::git::LineMark::Modified => "#d29922",
        crate::git::LineMark::Deleted => "#f85149",
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

    #[test]
    fn apply_edits_to_text_renames_across_lines() {
        use crate::lsp_core::{Encoding, Position, Range};
        let text = "let foo = 1;\nbar(foo);\n";
        // Replace `foo` on line 0 (cols 4..7) and line 1 (cols 4..7) with `baz`.
        let edit = |line: u32, s: u32, e: u32| {
            (Range { start: Position { line, character: s }, end: Position { line, character: e } }, "baz".to_string())
        };
        let out = apply_edits_to_text(text, Encoding::Utf16, &[edit(0, 4, 7), edit(1, 4, 7)]);
        assert_eq!(out, "let baz = 1;\nbar(baz);\n");
    }

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
