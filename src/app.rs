//! Application state and event handling.

#![warn(clippy::pedantic)]

mod command_palette;
mod coverage;
mod git;
mod info_panels;
mod insert_tools;
mod keymap;
mod lsp_dap;
mod org;
mod org_table;
mod picker_panels;
mod roam;
mod scripts;
mod session;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use crate::editor_core::actions::{
    Copy as CopyAction, Cut as CutAction, Paste as PasteAction, Redo as RedoAction, ToggleComment,
    Undo as UndoAction,
};
use crate::editor_core::selection::Selection;
use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
// Only this module's own `#[cfg(test)]` unit tests build `KeyModifiers` chords
// directly now; the keymap-dispatch code that used to need it crate-wide moved
// to `app::keymap` (T141).
#[cfg(test)]
use crossterm::event::KeyModifiers;
use include_dir::{Dir, include_dir};
use ratatui::layout::Rect;
use ratatui_image::picker::Picker;
use regex::Regex;

use crate::editor::{Editor, SEARCH_MARK, SplitDir, Tab, is_image_path};
use crate::explorer::Explorer;
use crate::menu::{Menu, menus};
use crate::messages::{Level, Messages};
use crate::palette::{self, Palette};
use crate::search::{Field, Flags as SearchFlags, Scope, SearchBar};
use crate::settings::Settings;
use crate::workspace_search::{Flags as WorkspaceFlags, Hit, WorkspaceSearch};

/// The repo's `themes/` directory, embedded into the binary so its themes are
/// available in the chooser without the user installing anything.
static BUNDLED_THEMES: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/themes");

/// Parse every bundled `*.json` theme. Malformed files are skipped.
fn bundled_themes() -> Vec<crate::theme::CustomTheme> {
    BUNDLED_THEMES
        .files()
        .filter(|f| f.path().extension().and_then(|e| e.to_str()) == Some("json"))
        .filter_map(|f| f.contents_utf8().and_then(crate::theme_model::parse_theme))
        // Plus the themes generated from base16 palettes (see `crate::base16`).
        .chain(crate::base16::themes())
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
    /// Enter a file path whose contents to insert at the cursor.
    InsertFile,
    /// Enter a coverage report (LCOV or Cobertura XML) path to load for the
    /// coverage gutter (T210). Pre-filled from the `coverage_path` setting.
    LoadCoverageFile,
    /// Confirm/edit a resolved `project.*` lifecycle command before running
    /// it (`App::pending_project_command` carries which slot and, for the
    /// subproject family, which directory).
    ProjectCommand,
    /// Enter a `YYYY-MM-DD` date to set the headline's `SCHEDULED:` entry.
    OrgSchedule,
    /// Enter a `YYYY-MM-DD` date to set the headline's `DEADLINE:` entry.
    OrgDeadline,
    /// Enter text for an Org sparse tree: subtrees not containing it fold.
    OrgSparseMatch,
    /// Enter the target of an Org link to insert (`[[target][description]]`).
    OrgLinkTarget,
    /// Enter the description of the Org link being inserted (empty for a bare
    /// `[[target]]` link).
    OrgLinkDesc,
    /// Enter the headline's tags (colon/comma/space separated; empty clears).
    OrgSetTags,
    /// Enter `NAME VALUE` for a property to set in the headline's drawer.
    OrgSetProperty,
    /// Enter `<column> [a|n|t] [r]` to sort the table at the cursor
    /// (`org-table-sort-lines` / `C-c ^`).
    OrgTableSort,
    /// Enter a property name for a new Column View column, inserted before
    /// the current column (`S-M-Right` inside the Column View overlay).
    OrgColumnsInsertColumn,
    /// Enter the `:id` scope for a new `#+BEGIN: columnview` dynamic block
    /// (`org.columns.insert_dblock`); empty defaults to `local`.
    OrgColumnsInsertDblock,
    /// Enter a name to save the just-recorded keyboard macro under.
    SaveMacro,
    /// Enter an expression to evaluate in the debugger (REPL).
    DebugRepl,
    /// Enter an expression to add as a debugger watch.
    DebugWatch,
    /// Answer one step of an in-progress Org-capture wizard: a `%^{}` field
    /// prompt, or (once every field is answered) the tag prompt.
    OrgCaptureField,
    /// Review/edit a capture's expanded template (multiline; Alt+Enter =
    /// newline) before filing it — skipped when the template sets
    /// `immediate_finish`.
    OrgCaptureReview,
    /// Enter a closing note (multiline; Alt+Enter = newline) while marking the
    /// headline at the cursor DONE (`C-u C-c C-t`).
    OrgCloseNote,
    /// Enter a tags/property match query for the Org agenda match view.
    OrgAgendaMatch,
    /// Enter keywords for the Org agenda text-search view.
    OrgAgendaSearch,
    /// Org-roam: find or create a node by title.
    RoamFind,
    /// Org-roam: insert a link to a node (found or created) by title.
    RoamInsert,
    /// Org-roam: capture a new node by title.
    RoamCapture,
    /// Org-roam: capture an entry into today's daily note.
    RoamDailyCapture,
    /// Org-roam: open the daily note for an entered `YYYY-MM-DD` date.
    RoamDailyDate,
    /// Org-roam: add a `#+filetags:` tag to the current node.
    RoamTag,
    /// Org-roam: add a `:ROAM_ALIASES:` alias to the current node.
    RoamAlias,
    /// Org-roam: add a `:ROAM_REFS:` ref (URL / cite key) to the current node.
    RoamRef,
    /// Org-node: insert a `#+transclude:` directive for a node (found or created).
    NodeTransclusion,
    /// Open a workspace from a `.vix-workspace` file path.
    WorkspaceOpen,
    /// Save the current workspace into a `.vix-workspace` file path.
    WorkspaceSave,
    /// Add a folder (by path) to the current workspace.
    WorkspaceAddFolder,
    /// Jump to the Nth paragraph (Go → Paragraph → Number).
    GotoParagraph,
    /// Jump to the Nth section (Go → Section → Number).
    GotoSection,
    /// Jump to the Nth sentence (Go → Sentence → Number).
    GotoSentence,
    /// Jump to the Nth word (Go → Word → Number).
    GotoWord,
    /// Jump to a percentage through the file (Go → Percent).
    GotoPercent,
    /// Jump to a byte offset (Go → Byte).
    GotoByte,
    /// Jujutsu: enter a repository URL to clone (`jj git clone`).
    JjClone,
    /// Jujutsu: enter a description for the working-copy change (`jj describe`).
    JjDescribe,
    /// Jujutsu: enter a description and start a new change (`jj commit`).
    JjCommit,
    /// Jujutsu: enter the revision to edit (`jj edit`).
    JjEdit,
    /// Jujutsu: enter the destination revision to rebase onto (`jj rebase -d`).
    JjRebase,
    /// Jujutsu: enter a name for a new bookmark (`jj bookmark create`).
    JjBookmarkCreate,
    /// Jujutsu: enter the bookmark to point at the working copy (`jj bookmark set`).
    JjBookmarkSet,
    /// Jujutsu: enter the bookmark to delete (`jj bookmark delete`).
    JjBookmarkDelete,
    /// Answer a script's `prompt(message, on_submit)` request
    /// (`App::pending_script_prompt` carries which script and handler to
    /// re-invoke with the entered text; see `crates/vix-script/spec/index.md`).
    Script,
    /// Enter the new key for the keybinding editor's selected row, as a
    /// `vix-macros` token (e.g. `C-S-k`); `App::pending_rebind_action_id`
    /// carries which action it should rebind to (T204).
    RebindKey,
    /// Enter a name to save the theme editor's draft under (T202); the
    /// draft itself lives in `App::theme_editor`.
    ThemeSaveAs,
    /// Enter the expansion prefix for a new snippet captured from the
    /// current selection (T205); the captured body lives in
    /// `App::pending_snippet_body`.
    SnippetPrefixFromSelection,
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
    /// A read-only preview shown above the input (only `OrgCaptureField`): the
    /// capture template in progress, with answered fields substituted, the
    /// field about to be answered marked `‹Label›`, and later ones `[Label]`.
    pub preview: Option<String>,
}

impl Prompt {
    /// A prompt of `kind` with the given border `title` and empty input.
    fn new(kind: PromptKind, title: String) -> Self {
        Prompt {
            kind,
            title,
            input: String::new(),
            case_sensitive: false,
            regex: false,
            preview: None,
        }
    }

    /// Set the initial input text.
    fn with_input(mut self, input: String) -> Self {
        self.input = input;
        self
    }

    /// Attach a read-only template preview shown above the input.
    fn with_preview(mut self, preview: String) -> Self {
        self.preview = Some(preview);
        self
    }
}

/// The user's choice for the current match in a query-replace session
/// (T151: folded in from the former `vix-query` crate, whose sole consumer
/// was always this file — see `spec/find-and-replace/index.md`).
#[derive(Clone, Copy)]
pub enum Decision {
    /// Replace this match (`y`).
    Replace,
    /// Skip this match (`n`).
    Skip,
    /// Replace this and all remaining matches (`!`).
    ReplaceRest,
    /// End the session (`q`).
    Quit,
}

/// State for an in-progress interactive query-replace session (`Ctrl+Alt+R`):
/// step through matches one at a time, deciding [`Decision::Replace`] /
/// [`Decision::Skip`] / [`Decision::ReplaceRest`] / [`Decision::Quit`] for
/// each. `App` owns the buffer the matches live in and drives the session
/// through its own private `begin_query_replace`/`qr_key`/`qr_apply` methods.
pub struct QueryReplace {
    /// Compiled search pattern.
    pub re: Regex,
    /// Replacement template — already un-escaped when in regex mode, so the
    /// regex engine only has capture groups left to expand.
    pub template: String,
    /// Whether to expand `$1`/`${name}` capture references in the template.
    pub regex: bool,
    /// Character offsets `[start, end)` of the match currently highlighted.
    pub current: (usize, usize),
    /// How many replacements have been applied so far.
    pub replaced: usize,
    /// The original query text, for the prompt label.
    pub label: String,
}

/// State for an in-progress Org-capture: which template, its unanswered
/// `%^{}` field prompts, the answers collected so far, and (if the template
/// uses `%^g`/`%^G`) the tag prompt's answer.
struct PendingCapture {
    /// The template being captured.
    template: vix_org_capture::CaptureTemplate,
    /// Every `%^{}` field prompt in the template, in order.
    prompts: Vec<vix_org_capture::FieldPrompt>,
    /// Answers collected so far, in prompt order.
    answers: Vec<String>,
    /// Index of the next unanswered prompt in `prompts`.
    next: usize,
    /// Whether the template uses `%^g`/`%^G` and so needs a tag prompt.
    wants_tags: bool,
    /// The tag prompt's answer, once given.
    tags: Option<String>,
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
/// Backing state for the interactive Org agenda buffer: enough to map a cursor
/// line back to its source task (so `t` can cycle its TODO state on disk) and to
/// confirm the active buffer really is this agenda before acting on it.
/// Which built-in Org agenda view a buffer shows (Org manual: "Agenda Views"),
/// carrying any query so `t` can rebuild the *same* view after a task changes.
#[derive(Clone)]
enum AgendaKind {
    /// Weekly/daily agenda (`org.agenda`): dated planning lines + unscheduled
    /// TODOs.
    Weekly,
    /// Global TODO list (`org.agenda.todo`).
    Todo,
    /// Stuck projects (`org.agenda.stuck`).
    Stuck,
    /// Tags/property match (`org.agenda.match`) for the held query.
    Match(String),
    /// Text search (`org.agenda.search`) for the held query.
    Search(String),
}

struct AgendaView {
    /// Which view this is, so `t` rebuilds the same one.
    kind: AgendaKind,
    /// Absolute source path + 0-based headline line for each agenda item, indexed
    /// as the view's render map refers to them.
    items: Vec<(PathBuf, usize)>,
    /// Buffer line index → index into `items`, or `None` for title/heading/blank
    /// lines (the map returned by the view's renderer).
    line_map: Vec<Option<usize>>,
    /// The rendered agenda text, compared against the active buffer to verify it
    /// is still the (read-only) agenda before `t` acts.
    rendered: String,
}

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
    /// Apply the reply to the DB workbench's SQL editor (see
    /// [`crate::db::Browser::apply_ai_reply`]).
    Db,
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

/// A pending "trust this workspace's scripts?" decision (T132), shown once
/// per untrusted workspace that has at least one project script.
pub struct ScriptTrustPrompt {
    /// How many `.rhai` files are waiting under `.vix/scripts/`, unloaded
    /// until this prompt is answered.
    pub count: usize,
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

/// The task chooser (Tools → Tasks…, Project → Run Task…): named tasks —
/// either just the workspace's `tasks.toml` entries (Tools → Tasks…) or the
/// full `vix-tasks` merge of user-configured, project-type, and discovered
/// tasks (Project → Run Task…) — and the highlighted row.
/// Choosing one runs its command via the Run pipeline.
pub struct TaskChooser {
    /// Loaded tasks, in file/merge order.
    pub tasks: Vec<crate::tasks::task::NamedTask>,
    /// Index of the highlighted task.
    pub selected: usize,
}

/// One of the six project lifecycle command slots (`crate::tasks::
/// lifecycle::LifecycleCommands`'s fields), addressed generically so the six
/// `project.*` lifecycle actions (and their `project.subproject.*`
/// counterparts) can share one prompt/cache/history code path instead of six
/// near-identical copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectSlot {
    /// `project.configure` / `project.subproject.configure`.
    Configure,
    /// `project.compile` / `project.subproject.compile`.
    Compile,
    /// `project.test` / `project.subproject.test`.
    Test,
    /// `project.install` / `project.subproject.install`.
    Install,
    /// `project.package` / `project.subproject.package`.
    Package,
    /// `project.run` / `project.subproject.run`.
    Run,
}

impl ProjectSlot {
    /// This slot's command out of a resolved [`crate::tasks::lifecycle::LifecycleCommands`].
    fn get(self, c: &crate::tasks::lifecycle::LifecycleCommands) -> Option<String> {
        match self {
            ProjectSlot::Configure => c.configure.clone(),
            ProjectSlot::Compile => c.compile.clone(),
            ProjectSlot::Test => c.test.clone(),
            ProjectSlot::Install => c.install.clone(),
            ProjectSlot::Package => c.package.clone(),
            ProjectSlot::Run => c.run.clone(),
        }
    }

    /// Set this slot's cached command.
    fn set(self, c: &mut crate::tasks::lifecycle::LifecycleCommands, val: Option<String>) {
        match self {
            ProjectSlot::Configure => c.configure = val,
            ProjectSlot::Compile => c.compile = val,
            ProjectSlot::Test => c.test = val,
            ProjectSlot::Install => c.install = val,
            ProjectSlot::Package => c.package = val,
            ProjectSlot::Run => c.run = val,
        }
    }

    /// This slot's command history within `h`.
    fn history_mut(self, h: &mut ProjectHistory) -> &mut Vec<String> {
        match self {
            ProjectSlot::Configure => &mut h.configure,
            ProjectSlot::Compile => &mut h.compile,
            ProjectSlot::Test => &mut h.test,
            ProjectSlot::Install => &mut h.install,
            ProjectSlot::Package => &mut h.package,
            ProjectSlot::Run => &mut h.run,
        }
    }

    /// The locale key for this slot's confirm-prompt title.
    fn prompt_title_key(self) -> &'static str {
        match self {
            ProjectSlot::Configure => "prompt.project_configure",
            ProjectSlot::Compile => "prompt.project_compile",
            ProjectSlot::Test => "prompt.project_test",
            ProjectSlot::Install => "prompt.project_install",
            ProjectSlot::Package => "prompt.project_package",
            ProjectSlot::Run => "prompt.project_run",
        }
    }
}

/// Per-lifecycle-slot command run history for the current workspace root
/// (`project.*`/`project.subproject.*` actions), most-recent last — each
/// command type keeps its own command history. Mirrors
/// `vix_session::WorkspaceSession`'s six `project_history_*` fields (kept as
/// plain fields there so `vix-session` stays free of a dependency on this
/// crate's `vix-tasks`).
#[derive(Debug, Clone, Default)]
struct ProjectHistory {
    /// History for the `configure` slot.
    configure: Vec<String>,
    /// History for the `compile` slot.
    compile: Vec<String>,
    /// History for the `test` slot.
    test: Vec<String>,
    /// History for the `install` slot.
    install: Vec<String>,
    /// History for the `package` slot.
    package: Vec<String>,
    /// History for the `run` slot.
    run: Vec<String>,
}

/// Context for a pending [`PromptKind::ProjectCommand`] prompt: which slot,
/// and (for `project.subproject.*`) the subproject's absolute directory to
/// run the command in instead of the workspace root.
struct PendingProjectCommand {
    /// Which lifecycle slot is being confirmed/edited.
    slot: ProjectSlot,
    /// `None` for the top-level (workspace-root) actions; `Some(dir)` for
    /// `project.subproject.*`.
    dir: Option<PathBuf>,
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

/// Jump-to-line label mode (EasyMotion/leap style): each visible line gets a
/// short label; typing it moves the cursor to that line's start.
pub struct JumpMode {
    /// `(label, 0-based line)` for each visible line.
    pub labels: Vec<(String, usize)>,
    /// Label characters typed so far (matched as a prefix).
    pub typed: String,
}

/// The saved-macro chooser (Edit → Play Saved Macro…): the persisted macros and
/// the highlighted row. Choosing one loads its keys and replays them.
pub struct MacroChooser {
    /// Saved macros, in file order.
    pub macros: Vec<crate::macros::Macro>,
    /// Index of the highlighted macro.
    pub selected: usize,
}

/// The script-command chooser (Tools → Scripts → Run…): every command every
/// loaded script registered, flattened, alongside the script's file stem
/// (its identity, shown so two commands with the same label are still
/// distinguishable). Choosing one runs it (`App::run_selected_script_command`).
pub struct ScriptChooser {
    /// `(script file stem, registered command)` pairs, across every loaded
    /// script, in load then registration order.
    pub commands: Vec<(String, vix_script::Command)>,
    /// Index of the highlighted command.
    pub selected: usize,
}

/// A script's `prompt(message, on_submit)` request awaiting an answer: which
/// loaded script (`App::scripts` index) and which of its `fn`s to re-invoke
/// with the entered text once `App::prompt` (`PromptKind::Script`) is
/// submitted. Cleared on submit or cancel.
struct PendingScriptPrompt {
    /// Index into `App::scripts`.
    script_index: usize,
    /// The script `fn` name to call with the prompt's answer.
    on_submit: String,
}

/// The clipboard-history picker (Edit → Paste from History…): recent copies/cuts,
/// most-recent first. Enter (or a click) pastes the highlighted entry.
pub struct ClipboardChooser {
    /// Recent clipboard entries, most-recent first.
    pub entries: Vec<String>,
    /// Index of the highlighted entry.
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
    /// VS Code shortcuts, e.g. `Ctrl+P` Quick Open, `Ctrl+Shift+P`
    /// Command Palette, `Ctrl+G` Go to Line. macOS and Windows share the same
    /// `Ctrl`-based bindings in the terminal.
    Vscode,
    /// `Ctrl` chords and the `Ctrl+X` prefix, e.g. `Ctrl+X Ctrl+F` to open.
    Emacs,
    /// Modal editing: a Normal mode for motions/commands and an Insert mode.
    Vi,
    /// Vi-style modal editing plus a `Space` leader for menu-like command
    /// sequences (e.g. `SPC f f` find file).
    Spacemacs,
    /// `IntelliJ` IDEA (macOS) shortcuts, with `Ctrl` standing in for `Cmd`.
    IntelliJMacOS,
    /// `IntelliJ` IDEA (Windows/Linux) shortcuts.
    IntelliJWindows,
    /// Eclipse (Windows) shortcuts.
    Eclipse,
    /// Sublime Text shortcuts, with `Ctrl` standing in for `Cmd`.
    Sublime,
}

impl Keymap {
    /// Parse a persisted keymap id, or `None` if it names none of
    /// [`vix_keymap_model::KEYMAPS`] — `App::new` is the only caller that
    /// ever sees `None` in practice (T146): [`App::set_keymap`] only ever
    /// writes an id `vix_keymap_model::by_id` already accepted, so the only
    /// way `settings.keymap` can hold an unrecognized id is a hand-edited
    /// (or otherwise corrupted) `settings.toml` loaded fresh at startup,
    /// which `App::new` reports and corrects once, there, rather than
    /// silently falling back on every call here.
    fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            "apple" => Keymap::Apple,
            // macOS and Windows VS Code share the same Ctrl-based bindings here.
            "vscode-macos" | "vscode-windows" => Keymap::Vscode,
            "emacs" => Keymap::Emacs,
            "vi" => Keymap::Vi,
            "spacemacs" => Keymap::Spacemacs,
            "intellij-macos" => Keymap::IntelliJMacOS,
            "intellij-windows" => Keymap::IntelliJWindows,
            "eclipse" => Keymap::Eclipse,
            "sublime" => Keymap::Sublime,
            _ => return None,
        })
    }
}

/// Convert a saved [`crate::session::PaneNode`] into a live pane tree, clamping
/// leaf tabs to `tab_count`.
fn node_to_pane(node: &crate::session::PaneNode, tab_count: usize) -> crate::pane_tree::Pane {
    match node {
        crate::session::PaneNode::Leaf(i) => {
            crate::pane_tree::Pane::Leaf((*i).min(tab_count.saturating_sub(1)))
        }
        crate::session::PaneNode::Split {
            dir,
            ratio,
            first,
            second,
        } => crate::pane_tree::Pane::Split {
            dir: if dir == "horizontal" {
                crate::editor::SplitDir::Horizontal
            } else {
                crate::editor::SplitDir::Vertical
            },
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
        crate::pane_tree::Pane::Leaf(tab) => {
            Some(crate::session::PaneNode::Leaf((*tab_to_file.get(*tab)?)?))
        }
        crate::pane_tree::Pane::Split {
            dir,
            ratio,
            first,
            second,
        } => Some(crate::session::PaneNode::Split {
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

/// One row of the recent-files chooser: the path plus the on-disk stats shown
/// in its columns (basename, path, size, created at, modified at).
pub struct RecentEntry {
    /// The file's path.
    pub path: PathBuf,
    /// Size in bytes.
    pub size: u64,
    /// Creation time in seconds since the Unix epoch, where recorded.
    pub created: Option<i64>,
    /// Last-modified time in seconds since the Unix epoch, or `None`.
    pub modified: Option<i64>,
}

/// Recent-files chooser overlay state (File -> Open Recent). Lists previously
/// opened files in a multi-column table; Enter (or a click) reopens the
/// highlighted one.
pub struct RecentChooser {
    /// Recent files, most-recent first, with their on-disk stats.
    pub entries: Vec<RecentEntry>,
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

/// Org-capture template chooser overlay state (**Org → Capture → Choose
/// Template…**). Lists every `org_capture_templates` entry; Enter (or a
/// click) starts capturing with the highlighted one.
pub struct CaptureChooser {
    /// Index of the highlighted template in `settings.org_capture_templates`.
    pub selected: usize,
}

/// State of a dedicated source-block edit (Org `C-c '`): where the block came
/// from, so a second `org.edit_src` can write the edited body back.
struct SrcEdit {
    /// Path of the source Org buffer (`None` for an untitled one).
    source: Option<PathBuf>,
    /// Tab index of the source buffer at entry (fallback for untitled ones).
    source_index: usize,
    /// Line of the block's `#+begin_src` fence in the source buffer.
    begin_line: usize,
}

/// Org refile-target chooser overlay state (**Org → Edit Structure → Refile
/// Subtree…**). Lists every headline outside the subtree being moved; Enter
/// (or a click) refiles under the highlighted one.
pub struct RefileChooser {
    /// Candidate targets: `(headline line, indented display label)`.
    pub targets: Vec<(usize, String)>,
    /// Index of the highlighted target.
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
    /// Code-overview minimap rectangle (a column at the right of the editor),
    /// when enabled. Zero-sized when off.
    pub minimap: Rect,
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
    /// Row-list rectangle of the open file browser (File → Open…), so a click
    /// can hit-test which row was picked and paging knows the view height.
    pub file_browser: Rect,
    /// Column-header rectangles (action, keys) of the keyboard-shortcut
    /// overlay, so a click can sort by that column.
    pub help_headers: [Rect; 2],
    /// Body (data-rows) rectangle of the keyboard-shortcut overlay, used to
    /// size paging and clamp the scroll.
    pub help_body: Rect,
    /// Column-header rectangles (action, keys) of the keybinding editor
    /// overlay (Vix → Keybindings…), so a click can sort by that column.
    pub keybinding_editor_headers: [Rect; 2],
    /// Body (data-rows) rectangle of the keybinding editor overlay, used to
    /// size paging, clamp the scroll, and hit-test a row click.
    pub keybinding_editor_body: Rect,
    /// Glyph-grid rectangle of the open Nerd Font palette, so a click can
    /// hit-test which cell was picked.
    pub nerd_palette: Rect,
    /// Row-list rectangle of the open ASCII panel, so a click can hit-test which
    /// row was picked.
    pub ascii_panel: Rect,
    /// Body (data-rows) rectangle of the open table editor, used to size paging
    /// and the scroll window.
    pub edit_table: Rect,
    /// Body (data-rows) rectangle of the open Column View overlay, used to
    /// size paging and the scroll window.
    pub column_view: Rect,
    /// Body rectangle of the open outline editor, used to size paging and the
    /// scroll window.
    pub edit_outline: Rect,
    /// Body rectangle of the open structured-value (JSON/YAML) editor.
    pub edit_value: Rect,
    /// Body rectangle of the open byte (hex) editor.
    pub edit_bytes: Rect,
    /// Body rectangle of the open SQL statement editor.
    pub edit_sql: Rect,
    /// Schema-tree body of the open DB workbench, for paging/scroll sizing.
    pub db_tree: Rect,
    /// Query-editor body of the open DB workbench.
    pub db_editor: Rect,
    /// Results-grid body (data rows) of the open DB workbench.
    pub db_results: Rect,
    /// Row-list rectangle of the open X11 color palette, so a click can hit-test
    /// which row was picked.
    pub x11_panel: Rect,
    /// Row-list rectangle of the open theme editor, for click-to-select.
    pub theme_editor: Rect,
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
    /// Clickable button rectangles in the find box: the Case/Word/Regex toggles,
    /// the Replace-mode and scope options, and, in replace mode, the Once/Ask/All
    /// replace buttons. `Rect::default()` when not shown.
    pub search_case: Rect,
    /// See [`Self::search_case`].
    pub search_smartcase: Rect,
    /// See [`Self::search_case`].
    pub search_word: Rect,
    /// See [`Self::search_case`].
    pub search_regex: Rect,
    /// The Replace option: turns the find box into a find-and-replace in place.
    /// See [`Self::search_case`].
    pub search_replace_toggle: Rect,
    /// The scope option: cycles this buffer → files → workspace, carrying the
    /// query with it. See [`Self::search_case`].
    pub search_scope: Rect,
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
    /// All folders in the current workspace (the first is `root`). The fuzzy file
    /// finder and project search span every folder; folders are added via
    /// File → Add Folder to Workspace and persisted in a `.vix-workspace` file.
    pub workspace_folders: Vec<PathBuf>,
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
    /// Active jump-to-line labels: `(label, 0-based line)` for each visible line,
    /// with the label prefix typed so far. `None` when jump mode is off.
    pub jump: Option<JumpMode>,
    /// Single-line prompt, when open.
    pub prompt: Option<Prompt>,
    /// In-progress paste operation, when active.
    pub paste: Option<PasteOp>,
    /// Pending confirmation, when active.
    pub confirm: Option<Confirm>,
    /// Pending "trust this workspace's scripts?" prompt (T132), when active.
    pub script_trust: Option<ScriptTrustPrompt>,
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
    /// The Rhai scripting engine (`crates/vix-script`), built once at
    /// startup. Loading/reloading scripts rebuilds `scripts`, not this.
    script_runtime: vix_script::Runtime,
    /// Every currently-loaded script (`App::load_scripts`/`reload_scripts`),
    /// in discovery order (global scripts, then project scripts, sorted by
    /// file stem within each — see `vix_script::discover`).
    scripts: Vec<vix_script::LoadedScript>,
    /// Accepted key binding overrides — persisted (`keybindings.toml`,
    /// T104i) and every loaded script's `bind_key` requests (T104j) —
    /// keyed on the `vix-macros` token, value the
    /// `App::run_action`-dispatchable id. Built by
    /// `App::resolve_key_overrides`, consulted by `App::override_key`,
    /// the `on_key` choke point every keymap's own dispatch runs after.
    key_overrides: std::collections::HashMap<String, String>,
    /// Script-command chooser overlay (Tools → Scripts → Run…), when open.
    pub script_chooser: Option<ScriptChooser>,
    /// A script's `prompt(...)` request awaiting an answer, while
    /// `App::prompt` (`PromptKind::Script`) is open.
    pending_script_prompt: Option<PendingScriptPrompt>,
    /// Recent clipboard entries (copies/cuts), most-recent first, for the
    /// paste-from-history picker.
    pub clipboard_ring: Vec<String>,
    /// Clipboard-history picker overlay (Edit → Paste from History…), when open.
    pub clipboard_chooser: Option<ClipboardChooser>,
    /// Recent-projects chooser overlay (File → Switch Project…), when open.
    pub workspace_chooser: Option<WorkspaceChooser>,
    /// Read-only diff overlay (Tools → Compare With File…), when open.
    pub diff_view: Option<DiffViewState>,
    /// File browser overlay (File → Open…), when open.
    pub file_browser: Option<crate::file_browser_panel::Panel>,
    /// Recent-files chooser overlay, when open.
    pub recent_chooser: Option<RecentChooser>,
    /// Recent-locations (jump list) chooser overlay, when open.
    pub location_chooser: Option<LocationChooser>,
    /// Org-capture template chooser overlay, when open.
    pub capture_chooser: Option<CaptureChooser>,
    /// Org refile-target chooser overlay, when open.
    pub refile_chooser: Option<RefileChooser>,
    /// State for an in-progress Org-capture wizard, while its prompts are
    /// being answered.
    pending_capture: Option<PendingCapture>,
    /// The template awaiting the final review-buffer step of a capture, while
    /// that prompt is open.
    capture_review: Option<vix_org_capture::CaptureTemplate>,
    /// Nerd Font palette (character picker) overlay, when open.
    pub nerd_palette: Option<NerdPalette>,
    /// ASCII panel (reference table) overlay, when open.
    pub ascii_panel: Option<AsciiPanel>,
    /// Table editor (CSV/TSV spreadsheet) overlay, when open.
    pub edit_table: Option<crate::edit_table::Grid>,
    /// Interactive Org Column View overlay, when open.
    pub column_view: Option<crate::column_view::ColumnView>,
    /// Outline editor (prose hierarchy) overlay, when open.
    pub edit_outline: Option<crate::edit_outline::Tree>,
    /// Structured-value editor (JSON/YAML tree) overlay, when open.
    pub edit_value: Option<crate::edit_value::Tree>,
    /// Byte editor (hex view) overlay, when open.
    pub edit_bytes: Option<crate::edit_bytes::Hex>,
    /// SQL statement editor overlay, when open.
    pub edit_sql: Option<crate::edit_sql::Editor>,
    /// DB workbench overlay (`spec/db`), when open.
    pub db: Option<crate::db::Browser>,
    /// X11 color palette overlay, when open.
    pub x11_panel: Option<X11Panel>,
    /// Theme editor overlay (T202), when open.
    pub theme_editor: Option<vix_theme_editor_panel::Panel>,
    /// Whether [`App::x11_panel`] was opened *from* the theme editor to pick
    /// a slot's color, rather than from Tools → X11 Colors to insert a hex
    /// value into a buffer — its `Enter` handler branches on this.
    theme_editor_picking: bool,
    /// The theme editor's committed baseline, to revert to if the editor is
    /// closed (`Esc`) without saving. `None` when the editor is closed.
    theme_editor_baseline: Option<String>,
    /// Media-type (MIME) picker overlay, when open.
    pub media_type_panel: Option<crate::media_type::Panel>,
    /// Receiver for an in-flight HTTP request's response (background `curl`).
    http_rx: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
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
    /// The preview's table-of-contents jump list (T206), when open over it --
    /// built from `markdown_preview`'s own `toc`, so it only ever opens while
    /// the preview is also open.
    pub markdown_toc: Option<Outline>,
    /// Snippets picker overlay, when open.
    pub snippets: Option<crate::snippets::Picker>,
    /// The in-scope snippet library (bundled + global + media-type + project),
    /// rebuilt for the active buffer's media type.
    pub snippet_library: Vec<crate::snippets::Snippet>,
    /// The media-type key the `snippet_library` was last built for (cache guard).
    snippet_library_key: Option<String>,
    /// Active snippet tabstop session (Tab navigates the fields), when expanding.
    snippet_session: Option<SnippetSession>,
    /// The selection text captured for `PromptKind::SnippetPrefixFromSelection`
    /// (T205), waiting on the prefix prompt's answer.
    pending_snippet_body: Option<String>,
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
    /// Last-seen on-disk modification time per open file path, for detecting
    /// external changes (auto-reload). Seeded lazily on first poll.
    disk_mtimes: std::collections::HashMap<PathBuf, std::time::SystemTime>,
    /// Throttle anchor for the external-change poll; `None` until the first poll.
    last_disk_poll: Option<std::time::Instant>,
    /// When format-on-save triggered an async LSP format, the path to re-save once
    /// the formatting edits land.
    format_save_pending: Option<PathBuf>,
    /// Throttle anchor for interval auto-save; `None` until the first tick.
    last_auto_save: Option<std::time::Instant>,
    /// `(tab, revision, cursor)` the word-occurrence highlight was last built for.
    word_highlight_key: Option<(usize, u64, usize)>,
    /// When true, the bottom dock shows live backlinks for the active node.
    backlinks_follow: bool,
    /// `(active tab, revision)` the backlinks dock was last built for.
    backlinks_follow_key: Option<(usize, u64)>,
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
    /// Settings file this app persists to. `None` — the normal case — uses the
    /// user's config directory; set via [`App::with_settings_path`] to keep a
    /// run's settings out of it (tests, embedders).
    pub settings_path: Option<PathBuf>,
    /// Session file this app persists to (open files, script trust, …).
    /// `None` — the normal case — uses the user's config directory; set via
    /// [`App::with_session_path`] to keep a run's session out of it (tests,
    /// embedders) — same shape as [`App::settings_path`].
    pub session_path: Option<PathBuf>,
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
    /// The last Org link stored with **Org → Hyperlinks → Store Link** (seeds
    /// the Insert Link… prompt).
    stored_org_link: Option<String>,
    /// Org agenda restriction lock (**Org → Agenda → Set Restriction Lock**):
    /// when set, agenda views scan only this file. Session-only, like Emacs's
    /// `C-c C-x <`.
    agenda_restriction: Option<PathBuf>,
    /// In-progress dedicated source-block edit (Org `C-c '`), if any.
    src_edit: Option<SrcEdit>,
    /// Org table rectangle clipboard (`C-c C-x M-w`/`C-w`/`C-y`), separate from
    /// the normal text clipboard, matching Emacs's own dedicated table
    /// rectangle clipboard. Row-major cell text.
    table_rectangle_clip: Option<Vec<Vec<String>>>,
    /// Pending third key of an Emacs `C-c C-x …` chord.
    emacs_c_x_prefix: bool,
    /// Pending third key of an Emacs `C-c p …` chord (the `project.*` family).
    emacs_c_p_prefix: bool,
    /// Pending fourth key of an Emacs `C-c p c …` chord.
    emacs_c_p_c_prefix: bool,
    /// Pending fifth key of an Emacs `C-c p c m …` chord (the
    /// `project.subproject.*` family).
    emacs_c_p_c_m_prefix: bool,
    /// The link target entered in the first Insert Link… prompt, held while the
    /// description prompt is open.
    pending_link_target: Option<String>,
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
    /// Parsed coverage report (T210: **Tools → Load Coverage File…**), if one
    /// has been loaded. Stays cached across a Toggle Coverage Gutter off/on.
    coverage: Option<vix_coverage::Report>,
    /// Whether the coverage gutter is currently shown. While it is, the git
    /// diff gutter is skipped for the active tab -- both use the same
    /// gutter-sign column, so only one shows at a time.
    coverage_visible: bool,
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
    /// When true, the calendar opens/creates the selected day's Org-roam daily
    /// note on Enter instead of inserting the date string.
    pub calendar_dailies: bool,
    /// Month navigation state for the calendar box.
    pub calendar: crate::calendar::Calendar,
    /// Whether the clock box is shown.
    pub show_clock: bool,
    /// Row-selection state for the clock box.
    pub clock: crate::clock::Clock,
    /// Keyboard-shortcut overlay (Help → Keyboard Shortcuts…, F1), when open:
    /// a filterable, header-sortable action/shortcut table.
    pub help: Option<crate::keyboard_shortcut_panel::Panel>,
    /// Keybinding editor overlay (Vix → Keybindings…), when open: a
    /// filterable, header-sortable, *selectable* action/shortcut table
    /// (T204) that also supports rebind and reset, unlike the read-only
    /// [`App::help`] panel it's a sibling of.
    pub keybinding_editor: Option<vix_keybinding_editor_panel::Panel>,
    /// The `action_id` a `PromptKind::RebindKey` prompt (opened from the
    /// keybinding editor) is currently rebinding, stashed while the prompt
    /// is open — mirrors [`PendingScriptPrompt`]'s "extra context lives in
    /// its own field" convention (`PromptKind` variants carry none).
    pending_rebind_action_id: Option<String>,
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
    /// When set (by `project.subproject.find_file`), restricts the palette's
    /// Files mode to entries under this project-relative directory instead
    /// of the whole workspace. Cleared whenever the palette closes.
    palette_file_scope: Option<String>,
    /// In-memory cache of resolved/edited lifecycle commands for the current
    /// workspace root (`project.*`/`project.subproject.*` actions) — the
    /// highest-precedence layer in `crate::tasks::lifecycle::
    /// effective_lifecycle`. Lazily loaded from the session store by
    /// [`App::ensure_project_session_loaded`].
    project_command_cache: crate::tasks::lifecycle::LifecycleCommands,
    /// Per-slot command run history for the current workspace root, paired
    /// with `project_command_cache`.
    project_history: ProjectHistory,
    /// The most recently run project command of any kind (a lifecycle
    /// command or a named task), for `project.repeat_last_task`.
    project_last_command: Option<String>,
    /// Whether `project_command_cache`/`project_history`/`project_last_command`
    /// have been loaded from the session store yet this run. Guards both the
    /// lazy load (so it happens at most once) and the save on exit (so a run
    /// that never touches a `project.*` action does not overwrite previously
    /// saved project state with empty defaults).
    project_session_loaded: bool,
    /// Context for a pending [`PromptKind::ProjectCommand`] prompt.
    pending_project_command: Option<PendingProjectCommand>,
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
    /// Emacs keymap: a `Ctrl+C` prefix has been pressed (the Org command family,
    /// e.g. `C-c C-t`, `C-c C-c`) and the next key completes the chord.
    emacs_c_prefix: bool,
    /// Emacs keymap: a `Ctrl+U` universal argument is pending, applying to the
    /// next command (used by `C-u C-c C-t` to close a task *with* a note).
    emacs_universal: bool,
    /// The interactive Org agenda view backing the current agenda buffer, if one
    /// is open — maps buffer lines to source tasks so `t` can toggle them.
    agenda: Option<AgendaView>,
    /// Vi / Spacemacs keymaps: true in Insert mode, false in Normal mode.
    /// Meaningless in the non-modal keymaps.
    modal_insert: bool,
    /// Vim keymap: the in-progress `:` command-line text, when the command line
    /// is open.
    vim_cmd: Option<String>,
    /// Vim keymap: the first key of a pending two-key Normal-mode operator
    /// (`g`, `d`, or `y`), awaiting its second key.
    vim_pending: Option<char>,
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
        let mut flags = crate::editor::Flags::empty();
        flags.set(crate::editor::Flags::LINE_NUMBERS, settings.line_numbers);
        flags.set(
            crate::editor::Flags::RELATIVE_LINE_NUMBERS,
            settings.relative_line_numbers,
        );
        flags.set(
            crate::editor::Flags::SHOW_WHITESPACE,
            settings.show_whitespace,
        );
        flags.set(crate::editor::Flags::SOFT_WRAP, settings.soft_wrap);
        let mut editor = Editor::new(flags, settings.indent_string());
        for tab in &mut editor.tabs {
            tab.editor.set_auto_pair(settings.auto_pair);
            tab.editor.set_rainbow_brackets(settings.rainbow_brackets);
            tab.editor
                .set_relative_line_numbers(settings.relative_line_numbers);
        }
        let mut messages = Messages::default();
        messages.advice(t!("msg.welcome").to_string());
        messages.info(t!("msg.welcome_hint").to_string());

        let lsp = crate::lsp::Lsp::new(settings.lsp_enabled, settings.lsp_servers.clone(), root);
        (editor, messages, lsp)
    }

    /// Build an app rooted at `root` using the given `settings`.
    ///
    /// The active locale and theme should already be applied by the caller
    /// (see `main`); the theme is (re)applied here so the first buffer is styled
    /// correctly, and the welcome messages are produced in the current locale.
    #[must_use]
    // The App struct has ~160 fields; this constructor is one irreducible struct
    // literal that rustfmt lays out one field per line, so it necessarily exceeds
    // the pedantic 100-line limit. Splitting it would require grouping fields into
    // sub-structs, rippling `self.field` accesses across the whole crate. A single
    // targeted expect keeps the rest of the pedantic gate intact.
    #[expect(
        clippy::too_many_lines,
        reason = "irreducible ~160-field struct initializer"
    )]
    pub fn new(root: PathBuf, settings: Settings) -> Self {
        // Seed the in-memory palette recents from the persisted list.
        let command_recents = settings.command_recents.clone();
        let (editor, messages, lsp) = Self::build_core(&root, &settings);

        let mut app = App {
            explorer: Explorer::new(root.clone()),
            workspace_folders: vec![root.clone()],
            root,
            editor,
            messages,
            menu: Menu::default(),
            palette: None,
            search: None,
            query_replace: None,
            workspace_search: None,
            jump: None,
            prompt: None,
            paste: None,
            confirm: None,
            script_trust: None,
            replace_confirm: None,
            unsaved: None,
            spell_suggest: None,
            context_menu: None,
            git_panel: None,
            branch_chooser: None,
            task_chooser: None,
            macro_chooser: None,
            script_runtime: vix_script::Runtime::new(),
            scripts: Vec::new(),
            key_overrides: std::collections::HashMap::new(),
            script_chooser: None,
            pending_script_prompt: None,
            clipboard_ring: Vec::new(),
            clipboard_chooser: None,
            workspace_chooser: None,
            diff_view: None,
            file_browser: None,
            recent_chooser: None,
            location_chooser: None,
            capture_chooser: None,
            refile_chooser: None,
            pending_capture: None,
            capture_review: None,
            nerd_palette: None,
            ascii_panel: None,
            edit_table: None,
            column_view: None,
            edit_outline: None,
            edit_value: None,
            edit_bytes: None,
            edit_sql: None,
            db: None,
            qrcode: None,
            x11_panel: None,
            theme_editor: None,
            theme_editor_picking: false,
            theme_editor_baseline: None,
            media_type_panel: None,
            http_rx: None,
            html_panel: None,
            system_info: None,
            dashboard: None,
            dashboard_rx: None,
            outline: None,
            outline_dock: None,
            outline_dock_key: None,
            welcome: None,
            file_info: None,
            text_info: None,
            markdown_preview: None,
            markdown_toc: None,
            snippets: None,
            snippet_library: Vec::new(),
            snippet_library_key: None,
            snippet_session: None,
            pending_snippet_body: None,
            contacts: None,
            vcard: None,
            lsp,
            lsp_synced: std::collections::HashMap::new(),
            blame_cache: None,
            dap: crate::dap::Dap::new(),
            breakpoints: std::collections::HashMap::new(),
            dap_stopped: None,
            dap_stack: Vec::new(),
            dap_variables: Vec::new(),
            dap_watches: Vec::new(),
            show_debug_panel: false,
            test_capture: false,
            test_buffer: Vec::new(),
            test_results: Vec::new(),
            test_selected: 0,
            show_test_panel: false,
            hover: None,
            completion: None,
            dialog: None,
            color_converter: None,
            unit_converter: None,
            calculator: None,
            regex_tester: None,
            code_actions: None,
            code_lens: None,
            pomodoro: None,
            pomodoro_open: false,
            pomodoro_last_tick: None,
            disk_mtimes: std::collections::HashMap::new(),
            last_disk_poll: None,
            format_save_pending: None,
            last_auto_save: None,
            word_highlight_key: None,
            backlinks_follow: false,
            backlinks_follow_key: None,
            clip: Vec::new(),
            clip_cut: false,
            nav_history: Vec::new(),
            bookmarks: Vec::new(),
            nav_idx: 0,
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
            stored_org_link: None,
            agenda_restriction: None,
            src_edit: None,
            table_rectangle_clip: None,
            emacs_c_x_prefix: false,
            emacs_c_p_prefix: false,
            emacs_c_p_c_prefix: false,
            emacs_c_p_c_m_prefix: false,
            pending_link_target: None,
            expand_selection_dir: None,
            show_inlay_hints: true,
            suspend_requested: false,
            linked_ranges: None,
            git_repo: false,
            git_branch: None,
            git_status: Vec::new(),
            git_head_cache: std::collections::HashMap::new(),
            coverage: None,
            coverage_visible: false,
            spellcheck: settings.spellcheck,
            speller: None,
            speller_locale: None,
            show_bottom_dock: settings.show_bottom_dock,
            bottom_dock: crate::bottom_dock::BottomDock::with_scrollback(settings.scrollback),
            bottom_hscroll: 0,
            explorer_hscroll: 0,
            messages_hscroll: 0,
            show_calendar: false,
            calendar_dailies: false,
            calendar: crate::calendar::Calendar::new(),
            show_clock: false,
            clock: crate::clock::Clock::new(),
            help: None,
            keybinding_editor: None,
            pending_rebind_action_id: None,
            focus: Focus::Editor,
            status: t!("status.ready").to_string(),
            should_quit: false,
            layout: Layout::default(),
            settings,
            settings_path: None,
            session_path: None,
            file_index: Vec::new(),
            palette_origin: None,
            palette_file_scope: None,
            project_command_cache: crate::tasks::lifecycle::LifecycleCommands::default(),
            project_history: ProjectHistory::default(),
            project_last_command: None,
            project_session_loaded: false,
            pending_project_command: None,
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
            emacs_c_prefix: false,
            emacs_universal: false,
            agenda: None,
            modal_insert: false,
            vim_cmd: None,
            vim_pending: None,
            spacemacs_leader: None,
        };
        app.validate_keymap();
        app
    }

    /// Report and correct an unrecognized persisted `settings.keymap` once,
    /// at startup (T146) — the only way it can happen at all is a
    /// hand-edited (or otherwise corrupted) `settings.toml`, since
    /// `App::set_keymap` only ever writes an id `vix_keymap_model::by_id`
    /// already accepted. Falls back to Apple, the default keymap, and
    /// leaves the correction unpersisted (a bad on-disk value is worth
    /// reporting, not silently rewriting the user's file for them).
    fn validate_keymap(&mut self) {
        if Keymap::from_id(&self.settings.keymap).is_none() {
            self.messages
                .error(t!("msg.unknown_keymap", id = self.settings.keymap.clone()).to_string());
            self.settings.keymap = "apple".to_string();
        }
    }

    /// On first run, open the welcome screen, then turn the
    /// `show_welcome_dialog` setting off and save immediately so it does not
    /// reappear on the next launch — even if this run never exits cleanly.
    /// Called by `main` after construction; kept out of [`App::new`] so tests
    /// build a clean, overlay-free app.
    pub fn maybe_show_welcome(&mut self) {
        if self.settings.show_welcome_dialog {
            self.welcome = Some(WelcomePanel::open(Self::welcome_lines()));
            self.settings.show_welcome_dialog = false;
            let _ = self.store_settings();
        }
    }

    /// Open a path given on the command line.
    pub fn open_initial(&mut self, path: &Path) {
        self.open_path(path, false);
    }

    /// Open `content` (piped in on stdin) as an unsaved scratch buffer
    /// (T208's `vix -`) — the exact bytes given, no header line, since the
    /// caller may want to save or otherwise act on precisely what it piped.
    pub fn open_stdin_buffer(&mut self, content: &str) {
        self.editor.new_tab_with_content(content);
        self.focus = Focus::Editor;
        self.status = t!("status.stdin_buffer").into();
    }

    /// Open a read-only unified-diff overlay comparing `old` and `new`
    /// directly (T208's `--diff` CLI flag) — unlike the Tools → Compare
    /// With File… overlay (which diffs the active buffer against another
    /// file), this never touches the active buffer, so it works the moment
    /// the app starts, with nothing open yet. This is the shape a `git
    /// difftool` driver invokes with (`$LOCAL $REMOTE`); see
    /// `docs/cli/index.md`.
    pub fn open_diff_files(&mut self, old: &Path, new: &Path) {
        let read = |p: &Path| {
            std::fs::read_to_string(p).map_err(|e| t!("msg.open_failed", error = e).to_string())
        };
        let (old_text, new_text) = match (read(old), read(new)) {
            (Ok(o), Ok(n)) => (o, n),
            (Err(e), _) | (_, Err(e)) => {
                self.messages.error(e);
                return;
            }
        };
        let old_name = old.file_name().map_or_else(
            || old.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let new_name = new.file_name().map_or_else(
            || new.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let lines = crate::diff_view::build(&old_text, &new_text);
        if lines.is_empty() {
            self.status = t!("status.diff_identical").to_string();
            return;
        }
        self.diff_view = Some(DiffViewState {
            title: format!("{old_name} ↔ {new_name}"),
            lines,
            scroll: 0,
        });
    }

    // ----- action dispatch (menu + palette + shortcuts) ------------------

    /// Dispatch a file / workspace / bookmark action. Returns `true` if `action`
    /// was handled. Extracted to keep [`App::run_action`] within the line limit.
    fn run_file_action(&mut self, action: &str) -> bool {
        match action {
            "file.new" => {
                self.editor.new_tab();
                self.focus = Focus::Editor;
                self.status = t!("status.new_buffer").into();
            }
            "file.scratch" => self.new_scratch_buffer(),
            "file.open" => self.open_file_browser(),
            "file.open_path" => {
                self.prompt = Some(Prompt::new(PromptKind::Open, t!("prompt.open").to_string()));
            }
            "file.open_recent" => self.open_recent_chooser(),
            "file.insert_file" => self.open_insert_file_prompt(),
            "file.switch_project" => self.open_workspace_chooser(),
            "workspace.open" => self.prompt_workspace(PromptKind::WorkspaceOpen),
            "workspace.save" => self.prompt_workspace(PromptKind::WorkspaceSave),
            "workspace.add_folder" => self.prompt_workspace(PromptKind::WorkspaceAddFolder),
            "nav.recent_locations" => self.open_location_chooser(),
            "nav.jump" => self.open_jump(),
            "nav.back" => self.nav_back(),
            "nav.forward" => self.nav_forward(),
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
                self.prompt = Some(
                    Prompt::new(PromptKind::SaveAs, t!("prompt.save_as").to_string())
                        .with_input(cur),
                );
            }
            "file.rename" => self.open_rename_prompt(),
            "file.revert" => self.revert_active(),
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
            _ => return false,
        }
        true
    }

    /// Dispatch a named action (shared by the menu bar, command palette, and
    /// keyboard shortcuts).
    pub fn run_action(&mut self, action: &str) {
        // Go-menu navigation and Org → Contacts live in their own dispatchers
        // (keeps this function within the line limit).
        if self.go_action(action) || self.contacts_action(action) {
            return;
        }
        match action {
            a if self.run_file_action(a) => {}
            a if self.run_edit_action(a) => {}
            a if self.run_motion_action(a) => {}
            "explorer.filter_include" => {
                let cur = self.explorer.include_filter.clone();
                self.prompt = Some(
                    Prompt::new(
                        PromptKind::ExplorerInclude,
                        t!("prompt.explorer_include").to_string(),
                    )
                    .with_input(cur),
                );
            }
            "explorer.filter_exclude" => {
                let cur = self.explorer.exclude_filter.clone();
                self.prompt = Some(
                    Prompt::new(
                        PromptKind::ExplorerExclude,
                        t!("prompt.explorer_exclude").to_string(),
                    )
                    .with_input(cur),
                );
            }
            a if a.starts_with("view.theme:") => self.set_theme_by_name(&a["view.theme:".len()..]),
            "view.theme_edit" => self.open_theme_editor(),
            a if a.starts_with("view.locale:") => {
                self.set_locale_by_code(&a["view.locale:".len()..]);
            }
            a if a.starts_with("view.keymap:") => self.set_keymap(&a["view.keymap:".len()..]),
            a if a.starts_with("script:") => self.run_script_command(a),
            "tools.calendar" => {
                self.show_calendar = !self.show_calendar;
                // Always open on the present month; navigation is per-session.
                if self.show_calendar {
                    self.calendar.reset();
                }
            }
            a if self.open_edit_surface(a) => {}
            "tools.clock" => {
                self.show_clock = !self.show_clock;
                if self.show_clock {
                    self.clock.selected = 0;
                }
            }
            a if a.starts_with("view.time_zone:") => {
                self.set_time_zone_by_name(&a["view.time_zone:".len()..]);
            }
            a if self.run_tools_action(a) => {}
            a if self.run_text_tool_action(a) => {}
            a if self.run_vim_action(a) => {}
            other if self.run_view_action(other) => {}
            other if self.run_git_action(other) => {}
            other if self.run_project_action(other) => {}
            other if self.run_named_action(other) => {}
            other => self
                .messages
                .warn(t!("msg.unknown_action", action = other).to_string()),
        }
    }

    /// Dispatch a view/window, command, AI, tab, help, or `vix.*` action.
    /// Returns `true` if `action` was handled. Extracted from
    /// [`App::run_action`] to keep that function within the line limit.
    fn run_view_action(&mut self, action: &str) -> bool {
        match action {
            "tools.run_command" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::RunCommand,
                    t!("prompt.run_command").to_string(),
                ));
            }
            "tools.cancel_command" => self.cancel_command(),
            "tools.tasks" => self.open_tasks(),
            "script.run" => self.open_script_chooser(),
            "script.reload" => self.reload_scripts(),
            "keybindings.reload" => self.reload_key_overrides(),
            "keybindings.editor" => self.open_keybinding_editor(),
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
            "view.toggle_explorer_focus" => self.toggle_focus_explorer_editor(),
            "view.toggle_menu" => self.menu.toggle(),
            "view.line_numbers" | "tools.line_numbers" => self.toggle_editor_line_numbers(),
            "view.relative_line_numbers" => self.toggle_relative_line_numbers(),
            "view.read_only" => self.toggle_read_only(),
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
            "view.format_on_save" => {
                self.settings.format_on_save = !self.settings.format_on_save;
                self.status =
                    t!("status.format_on_save", on = self.settings.format_on_save).to_string();
            }
            "view.auto_save" => {
                self.settings.auto_save = !self.settings.auto_save;
                self.status = t!("status.auto_save", on = self.settings.auto_save).to_string();
            }
            "view.sticky_scroll" => {
                self.settings.sticky_scroll = !self.settings.sticky_scroll;
                self.status =
                    t!("status.sticky_scroll", on = self.settings.sticky_scroll).to_string();
            }
            "view.minimap" => {
                self.settings.show_minimap = !self.settings.show_minimap;
                self.status = t!("status.minimap", on = self.settings.show_minimap).to_string();
            }
            "view.menu_tooltips" => self.toggle_menu_tooltips(),
            "view.highlight_word" => {
                self.settings.highlight_word = !self.settings.highlight_word;
                if !self.settings.highlight_word {
                    self.editor.set_word_marks(Vec::new());
                }
                self.word_highlight_key = None; // force a rebuild on next refresh
                self.status =
                    t!("status.highlight_word", on = self.settings.highlight_word).to_string();
            }
            "view.scrollbar" => self.toggle_scrollbar(),
            "view.spellcheck" => self.toggle_spellcheck(),
            "view.auto_pair" => self.toggle_auto_pair(),
            "view.rainbow_brackets" => self.toggle_rainbow_brackets(),
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
            a if self.db_action(a) => {}
            "view.bottom_dock" => self.toggle_bottom_dock(),
            "tab.next" => self.editor.next_tab(),
            "tab.prev" => self.editor.prev_tab(),
            _ => return self.run_help_action(action),
        }
        true
    }

    /// Dispatch a help / about / `vix.*` action. Returns `true` if `action` was
    /// handled. Extracted to keep [`App::run_view_action`] within the line limit.
    fn run_help_action(&mut self, action: &str) -> bool {
        match action {
            "help.shortcuts" => self.open_help(),
            "help.welcome" => self.open_welcome(),
            "help.license" | "vix.license" => {
                self.welcome = Some(WelcomePanel::open(Self::license_lines()));
            }
            "help.report_issue" => {
                self.welcome = Some(WelcomePanel::open(Self::report_issue_lines()));
            }
            "help.privacy" => self.welcome = Some(WelcomePanel::open(Self::privacy_lines())),
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

    /// Dispatch a `project.*`/`project.subproject.*` action (the
    /// `vix-tasks` wiring). Returns `true` if `action` was handled.
    fn run_project_action(&mut self, action: &str) -> bool {
        match action {
            "project.configure" => self.open_project_command_prompt(ProjectSlot::Configure, None),
            "project.compile" => self.open_project_command_prompt(ProjectSlot::Compile, None),
            "project.test" => self.open_project_command_prompt(ProjectSlot::Test, None),
            "project.install" => self.open_project_command_prompt(ProjectSlot::Install, None),
            "project.package" => self.open_project_command_prompt(ProjectSlot::Package, None),
            "project.run" => self.open_project_command_prompt(ProjectSlot::Run, None),
            "project.test_at_point" => self.project_test_at_point(),
            "project.run_task" => self.open_project_run_task(),
            "project.repeat_last_task" => self.repeat_last_project_task(),
            "project.discard_command_cache" => self.discard_project_command_cache(),
            "project.subproject.find_file" => self.open_subproject_find_file(),
            "project.subproject.configure" => {
                self.open_subproject_command_prompt(ProjectSlot::Configure);
            }
            "project.subproject.compile" => {
                self.open_subproject_command_prompt(ProjectSlot::Compile);
            }
            "project.subproject.test" => self.open_subproject_command_prompt(ProjectSlot::Test),
            "project.subproject.install" => {
                self.open_subproject_command_prompt(ProjectSlot::Install);
            }
            "project.subproject.package" => {
                self.open_subproject_command_prompt(ProjectSlot::Package);
            }
            "project.subproject.run" => self.open_subproject_command_prompt(ProjectSlot::Run),
            _ => return false,
        }
        true
    }

    /// Single-quote `s` for safe use as one `sh -c` word (POSIX escaping).
    fn shell_single_quote(s: &str) -> String {
        format!("'{}'", s.replace('\'', "'\\''"))
    }

    /// Dispatch a buffer-editing action (`edit.undo`/`redo`/`cut`/`copy`/
    /// `paste`/`toggle_comment`, the `edit.case_*` transforms, and the
    /// whole-line operations). Returns `true` if `action` was handled. Extracted
    /// from [`App::run_action`] to keep that function within the line limit.
    fn run_edit_action(&mut self, action: &str) -> bool {
        // Read-only buffers block editing commands, but not copy/navigation/search
        // and pure selection. Scoped to `edit.*` so probing this dispatcher for a
        // non-edit action (it's tried for every action) never consumes it.
        if action.starts_with("edit.")
            && self.active_read_only()
            && Self::edit_action_mutates(action)
        {
            self.status = t!("status.read_only_blocked").to_string();
            return true;
        }
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
            "edit.undo_branch" => {
                let switched = self.editor.switch_undo_branch();
                self.status = if switched {
                    t!("status.undo_branch_switched").to_string()
                } else {
                    t!("status.undo_branch_none").to_string()
                };
            }
            "edit.cut" => {
                let killed = self
                    .editor
                    .active_tab_mut()
                    .and_then(|t| t.editor.get_selection_text());
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.apply(CutAction {});
                    t.dirty = true;
                    t.preview = false;
                }
                self.record_clipboard(killed);
            }
            "edit.copy" => {
                let copied = self
                    .editor
                    .active_tab_mut()
                    .and_then(|t| t.editor.get_selection_text());
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.apply(CopyAction {});
                }
                self.record_clipboard(copied);
            }
            "edit.paste" => {
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.apply(PasteAction {});
                    t.dirty = true;
                    t.preview = false;
                }
            }
            "edit.paste_from_history" => self.open_clipboard_chooser(),
            "edit.toggle_comment" => {
                if let Some(t) = self.editor.active_tab_mut()
                    && !t.is_image()
                {
                    // Comments the cursor line, or every line touched by the
                    // selection; the editor picks the language's token.
                    t.editor.apply(ToggleComment {});
                    t.dirty = true;
                    t.preview = false;
                }
            }
            "edit.emmet_expand" => self.emmet_expand(),
            "edit.toggle_value" => self.smart_toggle(),
            "edit.comment_banner" => self.comment_banner(),
            "edit.transpose_chars" => self.transpose(crate::textops::transpose_chars_at),
            "edit.transpose_words" => self.transpose(crate::textops::transpose_words_at),
            "edit.transpose_lines" => self.transpose(crate::textops::transpose_lines_at),
            "edit.transpose_sentences" => self.transpose(crate::textops::transpose_sentences_at),
            "edit.transpose_paragraphs" => self.transpose(crate::textops::transpose_paragraphs_at),
            "edit.transpose_sections" => self.transpose(crate::textops::transpose_sections_at),
            "edit.delete.character" => self.delete_unit(crate::textops::delete_char_at),
            "edit.delete.word" => self.delete_unit(crate::textops::delete_word_at),
            "edit.delete.sentence" => self.delete_unit(crate::textops::delete_sentence_at),
            "edit.delete.paragraph" => self.delete_unit(crate::textops::delete_paragraph_at),
            "edit.delete.section" => self.delete_unit(crate::textops::delete_section_at),
            "edit.wrap" => self.wrap_text(),
            "edit.increment_number" => self.bump_number(1),
            "edit.decrement_number" => self.bump_number(-1),
            "edit.align.equals" => {
                self.transform_selection_or_buffer(|s| crate::align::on_delimiter(s, '='));
            }
            "edit.align.colon" => {
                self.transform_selection_or_buffer(|s| crate::align::on_delimiter(s, ':'));
            }
            "edit.align.comma" => {
                self.transform_selection_or_buffer(|s| crate::align::on_delimiter(s, ','));
            }
            "edit.align.pipe" => {
                self.transform_selection_or_buffer(|s| crate::align::on_delimiter(s, '|'));
            }
            _ => return self.run_text_transform_action(action),
        }
        true
    }

    /// Dispatch a case-change / line-operation / EOL text transform. Returns
    /// `true` if `action` was handled. Extracted to keep [`App::run_edit_action`]
    /// within the line limit; the read-only guard there runs before this is
    /// reached, so mutation protection is preserved.
    fn run_text_transform_action(&mut self, action: &str) -> bool {
        match action {
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
            "edit.squeeze_blank_lines" => {
                self.transform_selection_or_buffer(crate::textops::squeeze_blank_lines);
            }
            "edit.eol_lf" => self.transform_selection_or_buffer(crate::textops::to_lf),
            "edit.eol_crlf" => self.transform_selection_or_buffer(crate::textops::to_crlf),
            "tools.convert.rot13" => self.transform_selection_or_buffer(crate::textops::rot13),
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
            "nav.switch_buffer" => self.open_palette_seeded("#"),
            "nav.outline" => self.open_outline(),
            // Named wrappers around `editor_motion` so a keybinding table
            // (`vix-keybindings`) can reference them as plain action ids —
            // added for the Emacs keymap's conversion (T104a); nothing here
            // is Emacs-specific, so a later keymap's own conversion should
            // reuse these rather than adding a second set.
            "motion.char_right" => self.editor_motion(KeyCode::Right),
            "motion.char_left" => self.editor_motion(KeyCode::Left),
            "motion.line_down" => self.editor_motion(KeyCode::Down),
            "motion.line_up" => self.editor_motion(KeyCode::Up),
            "motion.home" => self.editor_motion(KeyCode::Home),
            "motion.end" => self.editor_motion(KeyCode::End),
            "motion.page_down" => self.editor_motion(KeyCode::PageDown),
            "motion.page_up" => self.editor_motion(KeyCode::PageUp),
            "motion.delete_forward" => self.editor_motion(KeyCode::Delete),
            "edit.keyboard_quit" => self.status = t!("status.emacs_quit").to_string(),
            _ => return self.run_search_action(action),
        }
        true
    }

    /// Dispatch a find/replace/workspace-search action. Returns `true` if
    /// `action` was handled. Extracted to keep [`App::run_motion_action`] within
    /// the line limit; falls through to [`App::run_lsp_action`].
    fn run_search_action(&mut self, action: &str) -> bool {
        match action {
            "edit.find" => self.start_search(false),
            "edit.find_next" => self.find_step(true),
            "edit.find_prev" => self.find_step(false),
            "edit.replace" => self.start_search(true),
            "edit.query_replace" => {
                self.start_search(true);
                if let Some(s) = self.search.as_mut() {
                    s.flags.insert(SearchFlags::INTERACTIVE);
                }
            }
            "search.workspace" => self.open_workspace_search(false),
            "search.workspace_replace" => self.open_workspace_search(true),
            "search.workspace_dock" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::SearchToDock,
                    t!("prompt.search_dock").to_string(),
                ));
            }
            "search.next_selection" => self.find_selection(true),
            "search.prev_selection" => self.find_selection(false),
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
            "lsp.call_hierarchy" => self.call_hierarchy(),
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
            "tools.todo_finder" => self.open_todo_finder(),
            "tools.http_send" => self.http_send(),
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
            a if self.draw_insert(a) => {}
            a if self.insert_marker(a) => {}
            a if self.surround(a) => {}
            a if self.insert_block(a) => {}
            a if self.org_action(a) => {}
            a if self.insert_dynamic(a) => {}
            "tools.checksum.sha256" => {
                self.transform_selection_or_buffer(crate::checksum_tool::sha256_hex);
            }
            "tools.checksum.sha512" => {
                self.transform_selection_or_buffer(crate::checksum_tool::sha512_hex);
            }
            "tools.checksum.md5" => {
                self.transform_selection_or_buffer(crate::checksum_tool::md5_hex);
            }
            "tools.checksum.crc32" => {
                self.transform_selection_or_buffer(crate::checksum_tool::crc32_hex);
            }
            _ => return self.run_convert_action(action),
        }
        true
    }

    /// Dispatch a `tools.convert.*` / `tools.format.*` text conversion. Returns
    /// `true` if `action` was handled. Extracted to keep
    /// [`App::run_text_tool_action`] within the line limit.
    fn run_convert_action(&mut self, action: &str) -> bool {
        match action {
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
                self.transform_selection_or_buffer_try(
                    crate::convert_from_csv_into_json_tool::convert,
                );
            }
            "tools.convert.csv.tsv" => {
                self.transform_selection_or_buffer_try(
                    crate::convert_from_csv_into_tsv_tool::convert,
                );
            }
            "tools.convert.tsv.csv" => {
                self.transform_selection_or_buffer_try(
                    crate::convert_from_tsv_into_csv_tool::convert,
                );
            }
            "tools.convert.tsv.json" => {
                self.transform_selection_or_buffer_try(
                    crate::convert_from_tsv_into_json_tool::convert,
                );
            }
            "tools.convert.json.csv" => {
                self.transform_selection_or_buffer_try(
                    crate::convert_from_json_into_csv_tool::convert,
                );
            }
            "tools.convert.json.tsv" => {
                self.transform_selection_or_buffer_try(
                    crate::convert_from_json_into_tsv_tool::convert,
                );
            }
            "tools.convert.json.yaml" => {
                self.transform_selection_or_buffer_try(
                    crate::convert_from_json_into_yaml_tool::convert,
                );
            }
            "tools.convert.yaml.json" => {
                self.transform_selection_or_buffer_try(
                    crate::convert_from_yaml_into_json_tool::convert,
                );
            }
            "tools.convert.json.toml" => {
                self.transform_selection_or_buffer_try(
                    crate::convert_from_json_into_toml_tool::convert,
                );
            }
            "tools.convert.toml.json" => {
                self.transform_selection_or_buffer_try(
                    crate::convert_from_toml_into_json_tool::convert,
                );
            }
            "tools.convert.markdown.html" => {
                self.transform_selection_or_buffer_try(
                    crate::convert_from_markdown_into_html_tool::convert,
                );
            }
            "tools.convert.html.markdown" => {
                self.transform_selection_or_buffer_try(
                    crate::convert_from_html_into_markdown_tool::convert,
                );
            }
            "tools.convert.unit" => self.open_unit_converter(),
            "tools.convert.jwt" => self.transform_selection_or_buffer_try(crate::jwt_tool::decode),
            "tools.convert.number.dec" => {
                self.transform_selection_or_buffer_try(crate::base_tool::to_dec);
            }
            "tools.convert.number.hex" => {
                self.transform_selection_or_buffer_try(crate::base_tool::to_hex);
            }
            "tools.convert.number.bin" => {
                self.transform_selection_or_buffer_try(crate::base_tool::to_bin);
            }
            "tools.convert.number.oct" => {
                self.transform_selection_or_buffer_try(crate::base_tool::to_oct);
            }
            _ => return self.run_format_action(action),
        }
        true
    }

    /// Dispatch a `tools.format.*` pretty/minify action. Returns `true` if
    /// `action` was handled. Extracted to keep [`App::run_convert_action`] within
    /// the line limit.
    fn run_format_action(&mut self, action: &str) -> bool {
        match action {
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
            "toggle_help" | "toggle_key_menu" => {
                if self.help.is_some() {
                    self.help = None;
                } else {
                    self.open_help();
                }
            }
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
            "spawn_multi_cursor"
            | "spawn_multi_cursor_select"
            | "skip_multi_cursor"
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
                self.status = t!(if self.overwrite {
                    "status.overwrite_on"
                } else {
                    "status.overwrite_off"
                })
                .to_string();
            }
            "toggle_ruler" => {
                self.show_ruler = !self.show_ruler;
                self.status = t!(if self.show_ruler {
                    "status.ruler_on"
                } else {
                    "status.ruler_off"
                })
                .to_string();
            }
            "macro.record" => {
                self.macro_recording = !self.macro_recording;
                if self.macro_recording {
                    self.macro_keys.clear();
                    self.status = t!("status.macro_recording").to_string();
                } else {
                    self.status =
                        t!("status.macro_recorded", count = self.macro_keys.len()).to_string();
                }
            }
            "macro.play" => self.play_macro(),
            "macro.save" => self.begin_save_macro(),
            "macro.play_saved" => self.open_macro_chooser(),
            "autocomplete" => self.autocomplete(true),
            "cycle_autocomplete_back" => self.autocomplete(false),
            "command_mode" => self.open_palette(),
            "shell_mode" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::RunCommand,
                    t!("prompt.run_command").to_string(),
                ));
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
        if self
            .editor
            .active_tab()
            .and_then(|t| t.path.as_ref())
            .is_none()
        {
            self.run_action("file.save_as");
            return;
        }
        if !self.write_active_to_disk() {
            return;
        }
        // Format on save: re-format via the language server, then re-save once the
        // edits land (see `apply_lsp_edits`). The plain save above already wrote
        // the file, so it is never lost if formatting is slow or unsupported.
        if self.settings.format_on_save
            && let Some(p) = self.active_path()
            && self.lsp.handles(&p)
        {
            self.format_save_pending = Some(p.clone());
            let tab_size = u32::try_from(self.settings.tab_width).unwrap_or(4);
            self.lsp.request_formatting(&p, tab_size);
        }
    }

    /// Write the active buffer to its file (applying the on-save options) and
    /// notify the language server. Returns whether the write succeeded. Does *not*
    /// trigger format-on-save — used both by [`App::save`] and the format-on-save
    /// re-save so the latter can't recurse.
    fn write_active_to_disk(&mut self) -> bool {
        let opts = self.save_options();
        match self.editor.save_active(opts) {
            Ok(p) => {
                self.status = t!("status.saved", path = p.display()).to_string();
                if self.lsp.handles(&p) {
                    let text = self.editor.active_tab().map(Tab::text).unwrap_or_default();
                    self.lsp.did_save(&p, &text);
                }
                // Persist the undo tree alongside the saved content.
                if self.settings.persistent_undo
                    && let Some(tab) = self.editor.active_tab()
                {
                    crate::undo_store::save(&p, &tab.text(), tab.editor.code_ref().history());
                }
                self.refresh_git();
                true
            }
            Err(e) => {
                self.messages
                    .error(t!("msg.save_failed", error = e).to_string());
                false
            }
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
            && let Some(p) = self.editor.active_tab().and_then(|t| t.path.clone())
        {
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
            self.zen_saved = Some((
                self.show_explorer,
                self.show_messages,
                self.show_bottom_dock,
                self.show_status_bar,
            ));
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

    /// The sticky-scroll header line: the source line of the enclosing scope whose
    /// own header has scrolled off the top of the viewport. `None` when sticky
    /// scroll is off, the top is visible, or there is no enclosing scope.
    #[must_use]
    pub fn sticky_header(&self) -> Option<String> {
        if !self.settings.sticky_scroll {
            return None;
        }
        let tab = self.editor.active_tab()?;
        if tab.is_image() {
            return None;
        }
        let top = self.editor.top_visible_line(); // 0-based first visible line
        if top == 0 {
            return None;
        }
        // The enclosing symbol is the last one declared at or above the first
        // visible line; show it only when its own header line is scrolled off.
        crate::palette::symbols(&tab.text())
            .into_iter()
            .rev()
            .find(|s| s.line <= top) // 1-based line <= top means strictly above the first visible (top+1)
            .map(|s| s.text)
    }

    /// The breadcrumb for the active buffer: its file name, then the enclosing
    /// symbol at the cursor (`file ▸ symbol`). Empty when there is no buffer.
    #[must_use]
    pub fn breadcrumb(&self) -> String {
        let Some(tab) = self.editor.active_tab() else {
            return String::new();
        };
        let name = tab.path.as_ref().and_then(|p| p.file_name()).map_or_else(
            || t!("ui.untitled").to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
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

    /// Toggle whether the menu bar shows hover tooltips (help text) for its menus
    /// and items. Persisted via `settings.show_menu_tooltips`.
    fn toggle_menu_tooltips(&mut self) {
        self.settings.show_menu_tooltips = !self.settings.show_menu_tooltips;
        self.status = t!(
            "status.menu_tooltips",
            on = self.settings.show_menu_tooltips
        )
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
    fn transform_selection_or_buffer_try(&mut self, f: impl Fn(&str) -> Result<String, String>) {
        if self.active_read_only() {
            self.status = t!("status.read_only_blocked").into();
            return;
        }
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

    /// Open a fresh throwaway scratch buffer (unsaved, with a header line).
    fn new_scratch_buffer(&mut self) {
        self.editor
            .new_tab_with_content(&format!("{}\n\n", t!("scratch.header")));
        self.focus = Focus::Editor;
        self.status = t!("status.scratch").into();
    }

    /// Rewrite the active buffer with a pure cursor-relative transform
    /// (`(text, cursor) -> Option<(new text, new cursor)>`; see
    /// [`crate::textops`]). On a miss, shows the `miss` status key when given.
    /// Read-only buffers are gated upstream by `run_edit_action`.
    fn rewrite_at_cursor(
        &mut self,
        f: impl Fn(&str, usize) -> Option<(String, usize)>,
        miss: Option<&str>,
    ) {
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        if tab.is_image() {
            return;
        }
        let cursor = tab.editor.get_cursor();
        let text = tab.editor.get_content();
        if let Some((new, pos)) = f(&text, cursor) {
            tab.editor.set_content(&new);
            tab.editor.set_cursor(pos);
            tab.dirty = true;
        } else if let Some(key) = miss {
            self.status = t!(key).to_string();
        }
    }

    /// Increment (or decrement) the integer at/after the cursor by `delta`,
    /// leaving the cursor on the number.
    fn bump_number(&mut self, delta: i64) {
        self.rewrite_at_cursor(
            |text, cursor| crate::textops::bump_number_at(text, cursor, delta),
            Some("status.no_number"),
        );
    }

    /// Transpose text around the cursor with `f` (chars or words), updating the
    /// buffer and cursor. No-op when `f` finds nothing to swap.
    fn transpose(&mut self, f: fn(&str, usize) -> Option<(String, usize)>) {
        self.rewrite_at_cursor(f, None);
    }

    /// Delete the text unit at the cursor with `f` (character, word, sentence,
    /// paragraph or section), updating the buffer and cursor. No-op when `f`
    /// finds no such unit.
    fn delete_unit(&mut self, f: fn(&str, usize) -> Option<(String, usize)>) {
        self.rewrite_at_cursor(f, None);
    }

    /// Hard-wrap (fill) text at the `wrap_column` setting: the selection when
    /// there is one, otherwise the paragraph around the cursor (the run of
    /// non-blank lines holding it). No-op (with a status note) when the text is
    /// already wrapped or the cursor is not in a paragraph.
    fn wrap_text(&mut self) {
        let width = self.settings.wrap_column;
        let selected = self
            .editor
            .active_tab_mut()
            .and_then(|t| t.editor.get_selection())
            .is_some_and(|sel| !sel.is_empty());
        if selected {
            self.transform_selection_or_buffer(|text| crate::textops::wrap(text, width));
        } else {
            self.rewrite_at_cursor(
                |text, cursor| crate::textops::wrap_paragraph_at(text, cursor, width),
                Some("status.no_wrap"),
            );
        }
    }

    /// Toggle the boolean-ish token under the cursor to its opposite (true/false,
    /// yes/no, `&&`/`||`, …). No-op (with a status note) when nothing matches.
    fn smart_toggle(&mut self) {
        self.rewrite_at_cursor(crate::textops::smart_toggle_at, Some("status.no_toggle"));
    }

    // ----- Org structure / dates / links (Emacs Org-menu parity) ----------

    // ----- Org-roam --------------------------------------------------------

    /// Accept the calendar's selected date: open its daily note (dailies mode) or
    /// insert the formatted date at the cursor. Closes the calendar.
    fn calendar_accept(&mut self) {
        self.show_calendar = false;
        if std::mem::take(&mut self.calendar_dailies) {
            let date = self.calendar.selected_formatted("%Y-%m-%d");
            self.roam_open_daily(&date);
        } else {
            let text = self
                .calendar
                .selected_formatted(Self::locale_date_pattern());
            let area = self.editor_view();
            self.editor.insert_str(&text, area);
        }
    }

    /// Today's date as a `(year, month, day)` tuple in the local zone, for
    /// Org column-view `CLOCKSUM_T`/age calculations.
    fn today_ymd() -> (i32, u32, u32) {
        let now = jiff::Zoned::now();
        (
            i32::from(now.year()),
            u32::from(now.month().unsigned_abs()),
            u32::from(now.day().unsigned_abs()),
        )
    }

    // ----- Org-node --------------------------------------------------------

    /// Surround the selection with a bracket/quote pair for an `edit.surround.*`
    /// action (add on first use, remove on repeat — [`toggle_wrap`] handles both).
    /// Returns `true` if `action` was a known surround pair.
    fn surround(&mut self, action: &str) -> bool {
        let (open, close) = match action {
            "edit.surround.paren" => ("(", ")"),
            "edit.surround.bracket" => ("[", "]"),
            "edit.surround.brace" => ("{", "}"),
            "edit.surround.angle" => ("<", ">"),
            "edit.surround.double_quote" => ("\"", "\""),
            "edit.surround.single_quote" => ("'", "'"),
            "edit.surround.backtick" => ("`", "`"),
            _ => return false,
        };
        self.toggle_wrap(open, close);
        true
    }

    /// Toggle `prefix`/`suffix` around the active selection (a conventional wrap;
    /// see [`crate::affix::toggle`]). With no selection, insert the empty pair and
    /// leave the cursor between the two halves.
    fn toggle_wrap(&mut self, prefix: &str, suffix: &str) {
        let area = self.layout.editor;
        let selection = self
            .editor
            .active_tab_mut()
            .and_then(|t| t.editor.get_selection_text());
        match selection {
            Some(sel) if !sel.is_empty() => {
                let wrapped = crate::affix::toggle(&sel, prefix, suffix);
                self.editor.insert_str(&wrapped, area);
            }
            _ => {
                self.editor.insert_str(&format!("{prefix}{suffix}"), area);
                if let Some(t) = self.editor.active_tab_mut() {
                    let cursor = t.editor.get_cursor();
                    t.editor
                        .set_cursor(cursor.saturating_sub(suffix.chars().count()));
                }
            }
        }
    }

    // ----- Color Converter ------------------------------------------------

    // ----- Unit Converter -------------------------------------------------

    // ----- Calculator -----------------------------------------------------

    // ----- Regex tester ---------------------------------------------------

    /// Whether the active tab's path has a (case-insensitive) `.org`
    /// extension. Shared gate for every Org-only, non-`org.*`-prefixed
    /// context check (drawer folding, table editing).
    fn active_is_org(&self) -> bool {
        self.active_path()
            .and_then(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_ascii_lowercase)
            })
            .as_deref()
            == Some("org")
    }

    // ----- Org table editor (crates/vix-org-table) -------------------------
    //
    // Context-sensitive Tab/RET/Meta-arrow/Shift-arrow handling lives in
    // `org_table_key`, gated on the cursor being inside a pipe table so it
    // never affects any other keybinding. The `org.table.*` actions below
    // (menu items, `C-c`-chord bindings) follow the same
    // `(text, cursor) -> Option<new state>` adapter shape as
    // `org_rewrite_line`/`org_move_subtree`, reporting
    // `status.org_table_no_table` on a miss.

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
        let Some(menu) = self.code_lens.take() else {
            return;
        };
        let Some((_, title, command, args)) = menu.lenses.into_iter().nth(menu.selected) else {
            return;
        };
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
        // With a language server, use its selection-range chain (expand + shrink).
        if self.lsp.handles(&path) {
            let (line, character) = self.cursor_lsp_position(&path);
            self.expand_selection_dir = Some(expand);
            self.lsp.request_selection_range(&path, line, character);
            return;
        }
        // Offline fallback: expand to the enclosing Tree-sitter node (expand only).
        if expand {
            self.expand_selection_to_node();
        } else {
            self.status = t!("status.lsp_inactive").to_string();
        }
    }

    /// Jump the cursor to the HTML/XML tag matching the one under the cursor.
    fn goto_matching_tag(&mut self) {
        let Some(t) = self.editor.active_tab() else {
            return;
        };
        let cursor = t.editor.get_cursor();
        match crate::tags::matching_tag(&t.editor.get_content(), cursor) {
            Some(off) => {
                let area = self.editor_view();
                self.editor.goto_offset(off, area);
            }
            None => self.status = t!("status.no_matching_tag").to_string(),
        }
    }

    /// Grow the active selection to the smallest enclosing Tree-sitter node
    /// (offline structural selection). No-op without a parse tree.
    fn expand_selection_to_node(&mut self) {
        let Some(t) = self.editor.active_tab_mut() else {
            return;
        };
        let (s, e) = {
            let cursor = t.editor.get_cursor();
            t.editor.get_selection().map_or((cursor, cursor), |sel| {
                (sel.start.min(sel.end), sel.start.max(sel.end))
            })
        };
        match t.editor.code_ref().expand_to_node(s, e) {
            Some((ns, ne)) => t.editor.set_selection_range(ns, ne),
            None => self.status = t!("status.no_node_selection").to_string(),
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
            let Some(t) = self.editor.active_tab_mut() else {
                return;
            };
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
                (
                    char_to_lsp_pos(code, line_start, enc),
                    char_to_lsp_pos(code, line_end, enc),
                )
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
        self.lsp
            .request_code_action(&path, start, end, &serde_json::Value::Array(diags));
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
        let Some(menu) = self.code_actions.take() else {
            return;
        };
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
    pub(super) fn open_pomodoro(&mut self) {
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
        self.pomodoro
            .as_ref()
            .is_some_and(crate::pomodoro_tool::Timer::is_running)
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
                if matches!(
                    self.pomodoro.as_ref().map(|t| t.phase),
                    Some(Phase::Idle) | None
                ) {
                    self.pomodoro = None;
                }
                self.pomodoro_open = false;
            }
            _ => {}
        }
    }

    // ----- merge conflicts ------------------------------------------------

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
        let Some(path) = Settings::user_dictionary_path() else {
            return Vec::new();
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return Vec::new();
        };
        text.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// Append `word` to the persisted personal word list, creating it if needed
    /// and skipping duplicates.
    fn persist_user_word(word: &str) {
        use std::io::Write;
        let Some(path) = Settings::user_dictionary_path() else {
            return;
        };
        if Self::load_user_words().iter().any(|w| w == word) {
            return;
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
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
        self.spell_suggest = Some(SpellSuggest {
            word,
            span: (start, end),
            suggestions,
            selected: 0,
        });
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
                    && p.selected + 1 < p.suggestions.len()
                {
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
            && row < p.suggestions.len()
        {
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
        let selected = CONTEXT_ITEMS
            .iter()
            .position(|(_, a)| *a != SEP_ACTION)
            .unwrap_or(0);
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
        let Some(cm) = self.context_menu.as_mut() else {
            return;
        };
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
        let action = self
            .context_menu
            .as_ref()
            .map(|cm| CONTEXT_ITEMS[cm.selected].1);
        self.context_menu = None;
        if let Some(action) = action
            && action != SEP_ACTION
        {
            self.run_action(action);
        }
    }

    // ----- git changes panel ----------------------------------------------

    // ----- Debugger (DAP) -------------------------------------------------

    /// Whether a debug session is active (drives the fast poll cadence).
    #[must_use]
    pub fn dap_busy(&self) -> bool {
        self.dap.busy()
    }

    /// Install any completed background reparse (large-file async highlighting).
    pub fn poll_parse(&mut self) {
        self.editor.poll_parse();
    }

    /// Periodically save the active dirty, file-backed buffer when auto-save is
    /// enabled (every few seconds). Uses a plain write (no format-on-save churn).
    pub fn poll_auto_save(&mut self) {
        if !self.settings.auto_save {
            return;
        }
        if self
            .last_auto_save
            .is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(5))
        {
            return;
        }
        self.last_auto_save = Some(std::time::Instant::now());
        let savable = self
            .editor
            .active_tab()
            .is_some_and(|t| t.dirty && t.path.is_some() && t.image.is_none());
        if savable && self.write_active_to_disk() {
            self.status = t!("status.auto_saved").to_string();
        }
    }

    /// When "Highlight Word" is on, mark every occurrence of the identifier under
    /// the cursor in the active buffer (recomputed only when the buffer or cursor
    /// changes). Clears the marks when the cursor is not on a word.
    pub fn refresh_word_highlight(&mut self) {
        if !self.settings.highlight_word {
            return;
        }
        let key = self.editor.active_tab().filter(|t| !t.is_image()).map(|t| {
            (
                self.editor.active,
                t.editor.revision(),
                t.editor.get_cursor(),
            )
        });
        if key == self.word_highlight_key {
            return;
        }
        self.word_highlight_key = key;
        let Some(word) = self.symbol_under_cursor() else {
            self.editor.set_word_marks(Vec::new());
            return;
        };
        let Some(tab) = self.editor.active_tab() else {
            return;
        };
        let content = tab.editor.get_content();
        let chars: Vec<char> = content.chars().collect();
        let wlen = word.chars().count();
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        let mut marks = Vec::new();
        // Whole-word occurrences of `word`, capped to keep the pass cheap.
        let mut i = 0;
        while i + wlen <= chars.len() {
            if chars[i..i + wlen].iter().copied().eq(word.chars())
                && (i == 0 || !is_word(chars[i - 1]))
                && (i + wlen == chars.len() || !is_word(chars[i + wlen]))
            {
                marks.push((i, i + wlen));
                i += wlen;
                if marks.len() >= 500 {
                    break;
                }
            } else {
                i += 1;
            }
        }
        // A single occurrence (the word itself) isn't worth highlighting.
        self.editor
            .set_word_marks(if marks.len() > 1 { marks } else { Vec::new() });
    }

    /// Detect files changed on disk by another process (a formatter, `git`, a
    /// second editor) and react: clean buffers are reloaded; a buffer with unsaved
    /// edits gets a one-time warning. Throttled to once per second.
    pub fn poll_file_changes(&mut self) {
        if self
            .last_disk_poll
            .is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(1))
        {
            return;
        }
        self.last_disk_poll = Some(std::time::Instant::now());

        let mut reload_clean = false;
        let mut dirty_changed: Vec<PathBuf> = Vec::new();
        for tab in &self.editor.tabs {
            let (Some(path), false) = (tab.path.as_ref(), tab.image.is_some()) else {
                continue;
            };
            let Ok(mtime) = std::fs::metadata(path).and_then(|m| m.modified()) else {
                continue;
            };
            let previous = self.disk_mtimes.insert(path.clone(), mtime);
            // Only act on a *change* (the first sighting just records the mtime).
            if previous.is_some_and(|p| p != mtime) {
                if tab.dirty {
                    dirty_changed.push(path.clone());
                } else {
                    reload_clean = true;
                }
            }
        }
        if reload_clean {
            let n = self.editor.reload_clean_from_disk();
            if n > 0 {
                self.status = t!("status.reloaded_external", count = n).to_string();
            }
        }
        for path in dirty_changed {
            self.messages
                .warn(t!("msg.external_change_unsaved", path = path.display()).to_string());
        }
    }

    /// Whether a background reparse is in flight (keeps the event loop ticking
    /// fast so the new highlights appear promptly).
    #[must_use]
    pub fn parse_busy(&self) -> bool {
        self.editor.parse_pending()
    }

    /// Drain debug-adapter events and apply them. Called each event-loop iteration.
    pub fn poll_dap(&mut self) {
        if !self.dap.is_active() {
            return;
        }
        for event in self.dap.poll() {
            match event {
                crate::dap::DapEvent::Running => {
                    self.status = t!("status.debug_running").to_string();
                }
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

    // ----- git branch switcher --------------------------------------------

    // ----- Tasks (tasks.toml runner) --------------------------------------

    /// Open the task chooser from the workspace's `tasks.toml`, or report when no
    /// tasks are defined.
    fn open_tasks(&mut self) {
        let tasks = crate::tasks::task::from_vix_tasks(&crate::tasks::load(&self.root));
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
    /// Also records it in the project `run` slot's history and as the most
    /// recent project command of any kind (`project.repeat_last_task`),
    /// whichever chooser it came from (Tools → Tasks… or Project → Run
    /// Task…) — one code path, so the two never drift.
    fn run_selected_task(&mut self) {
        let Some(c) = self.task_chooser.take() else {
            return;
        };
        if let Some(task) = c.tasks.get(c.selected) {
            let command = task.command.clone();
            self.run_command(&command);
            self.ensure_project_session_loaded();
            crate::tasks::history::push_history(
                &mut self.project_history.run,
                command.clone(),
                crate::tasks::history::DuplicatePolicy::IgnoreConsecutive,
            );
            self.project_last_command = Some(command);
        }
    }

    // ----- Scripting (vix-script wiring) ------------------------------------
    //
    // `vix-script` provides the pure engine (Rhai compile/invoke, resource
    // limits, discovery); everything host-side — reading the real
    // buffer/selection/cursor into a `HostState` snapshot before a call and
    // applying its effects after, the palette/menu/prompt/message
    // integration, and error routing — lives here
    // (`crates/vix-script/spec/index.md`, improvement plan T103). Key
    // bindings a script requested via `bind_key` are loaded (`LoadedScript::
    // bindings`) but not yet wired to real key dispatch — that's T104.

    // ----- script trust (T132) ---------------------------------------------
    //
    // vix-script auto-loads and runs every `.rhai` file under a workspace's
    // `.vix/scripts/` at startup with no confirmation today — sandboxed (no
    // file/network access, § crates/vix-script/spec/index.md), but still
    // able to read/rewrite the open buffer, spam messages, or plant a fake
    // `prompt()` on first open. Cloning an untrusted repo and opening it in
    // Vix would otherwise run its scripts silently. Global scripts
    // (`Settings::scripts_dir()`) need no such gate — the user put them
    // there directly, not some repo's own author.

    /// Resolve every current override request — persisted
    /// (`keybindings.toml`) and every currently-loaded script's
    /// `bind_key` requests (T104j) — against the active keymap's
    /// built-ins and against each other, in one combined batch, via
    /// `apply_key_overrides`. Combining both sources into a single call
    /// matters: the "a token claimed twice is a conflict, regardless of
    /// source" rule (§ `crates/vix-keybindings/spec/index.md` "Conflict
    /// handling") only holds if a persisted override and a script's
    /// override are resolved together, not in two separate calls that
    /// would each see the other's token as simply unclaimed. A script
    /// binding's `command_id` isn't yet a dispatchable action id on its
    /// own — it becomes one the same way the command palette's
    /// `script:`-prefixed entries do, `format!("script:{stem}:{command_id}")`
    /// (`App::run_script_command` already parses exactly that shape).
    /// Called at startup, and by `script.reload`/`keybindings.reload`.
    pub fn resolve_key_overrides(&mut self) {
        let mut requests: Vec<vix_keybindings::Override> = Settings::keybindings_path()
            .map(|path| vix_keybindings::user_bindings::load(&path))
            .unwrap_or_default()
            .into_iter()
            .map(|b| vix_keybindings::Override {
                key_token: b.key_token,
                action_id: b.action_id,
                source: vix_keybindings::Source::User,
            })
            .collect();
        for script in &self.scripts {
            for binding in &script.bindings {
                requests.push(vix_keybindings::Override {
                    key_token: binding.key_token.clone(),
                    action_id: format!("script:{}:{}", script.stem, binding.command_id),
                    source: vix_keybindings::Source::Script(script.stem.clone()),
                });
            }
        }
        self.apply_key_overrides(requests);
    }

    /// Resolve `requests` against the active keymap's built-ins (T104i),
    /// replacing `self.key_overrides` with the winners. Split out from
    /// `resolve_key_overrides` so tests can drive the choke point
    /// directly with hand-built requests, without touching the real
    /// `keybindings.toml` path or a real loaded script. A token two or
    /// more requests claim is rejected outright (all of them dropped)
    /// and reported as an error naming every source; a request that wins
    /// but also claims a token a built-in already owns is reported once,
    /// informationally — it still wins, the built-in just won't fire for
    /// that key any more.
    pub fn apply_key_overrides(&mut self, requests: Vec<vix_keybindings::Override>) {
        let resolved = vix_keybindings::resolve(requests, &self.settings.keymap);
        for conflict in &resolved.conflicts {
            let sources = conflict
                .sources
                .iter()
                .map(vix_keybindings::Source::describe)
                .collect::<Vec<_>>()
                .join(", ");
            self.messages.error(
                t!(
                    "msg.keybinding_conflict",
                    token = conflict.key_token.clone(),
                    sources = sources
                )
                .to_string(),
            );
        }
        for shadow in &resolved.shadows {
            self.messages.info(
                t!(
                    "msg.keybinding_shadows_builtin",
                    token = shadow.key_token.clone(),
                    action = shadow.shadowed_action_id
                )
                .to_string(),
            );
        }
        self.key_overrides = resolved
            .accepted
            .into_iter()
            .map(|o| (o.key_token, o.action_id))
            .collect();
    }

    /// `keybindings.reload`: re-run `resolve_key_overrides` and report
    /// how many ended up active, so a hand-edited `keybindings.toml` can
    /// be picked up without restarting — mirrors `script.reload`'s shape.
    fn reload_key_overrides(&mut self) {
        self.resolve_key_overrides();
        self.messages
            .info(t!("msg.keybindings_reloaded", count = self.key_overrides.len()).to_string());
    }

    // ----- keybinding editor (Vix → Keybindings…, T204) -------------------

    /// `keybindings.editor`: open the editable keybinding overlay over the
    /// active keymap's current bindings.
    fn open_keybinding_editor(&mut self) {
        self.keybinding_editor = Some(vix_keybinding_editor_panel::Panel::open(
            self.build_keybinding_rows(),
        ));
    }

    /// Assemble every rebindable row: the active keymap's *top-level*
    /// bindings (chorded contexts are excluded — overrides never resolve
    /// against them, see `crates/vix-keybinding-editor-panel/spec/index.md`
    /// "Contents"), the keymap-agnostic [`vix_keybindings::SHARED`]
    /// bindings, and any token already accepted into
    /// [`App::key_overrides`] (covers a user/script token bound to a key
    /// with no built-in at all). Each token's `Source` is `User` when
    /// `keybindings.toml` names it, `Script` when a loaded script's
    /// `bindings` do, else `BuiltIn`.
    fn build_keybinding_rows(&self) -> Vec<vix_keybinding_editor_panel::Row> {
        use vix_keybinding_editor_panel::{Row, Source};
        let user_overrides = Settings::keybindings_path()
            .map(|path| vix_keybindings::user_bindings::load(&path))
            .unwrap_or_default();
        let overridden_tokens: std::collections::HashSet<&str> = user_overrides
            .iter()
            .map(|b| b.key_token.as_str())
            .collect();

        let mut tokens: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for table in vix_keybindings::TABLES
            .iter()
            .filter(|t| t.keymap_id == self.settings.keymap)
        {
            for ctx in table.contexts.iter().filter(|c| c.name.is_empty()) {
                for b in ctx.bindings {
                    tokens.insert(b.key_token.to_string());
                }
            }
        }
        for b in vix_keybindings::SHARED {
            tokens.insert(b.key_token.to_string());
        }
        tokens.extend(self.key_overrides.keys().cloned());

        tokens
            .into_iter()
            .filter_map(|key_token| {
                let action_id = self
                    .key_overrides
                    .get(&key_token)
                    .cloned()
                    .or_else(|| {
                        vix_keybindings::lookup(&self.settings.keymap, "", &key_token)
                            .map(str::to_string)
                    })
                    .or_else(|| vix_keybindings::lookup_shared(&key_token).map(str::to_string))?;
                let source = if overridden_tokens.contains(key_token.as_str()) {
                    Source::User
                } else if let Some(script) = self
                    .scripts
                    .iter()
                    .find(|s| s.bindings.iter().any(|b| b.key_token == key_token))
                {
                    Source::Script(script.stem.clone())
                } else {
                    Source::BuiltIn
                };
                Some(Row {
                    key_display: modifier_token_display(&key_token),
                    key_token,
                    action_title: Self::action_title(&action_id),
                    action_id,
                    source,
                })
            })
            .collect()
    }

    /// Rebuild the open keybinding editor's rows in place after a rebind or
    /// a reset, keeping the query/sort/scroll and clamping the selection if
    /// the row count shrank.
    fn refresh_keybinding_editor(&mut self) {
        let rows = self.build_keybinding_rows();
        if let Some(p) = self.keybinding_editor.as_mut() {
            p.rows = rows;
            let len = p.len();
            if p.selected >= len {
                p.selected = len.saturating_sub(1);
            }
        }
    }

    /// Key handling for the keybinding editor overlay: mirrors the F1
    /// keyboard-shortcut panel's filter/scroll keys, plus Enter to rebind
    /// and Delete to reset the selected row.
    fn keybinding_editor_key(&mut self, key: KeyEvent) {
        if self.keybinding_editor.is_none() {
            return;
        }
        let page = (self.layout.keybinding_editor_body.height as usize).max(1);
        match key.code {
            KeyCode::Esc => self.keybinding_editor = None,
            KeyCode::Backspace => {
                if let Some(p) = self.keybinding_editor.as_mut() {
                    p.backspace();
                }
            }
            KeyCode::Up => {
                if let Some(p) = self.keybinding_editor.as_mut() {
                    p.select_up(1);
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.keybinding_editor.as_mut() {
                    p.select_down(1);
                }
            }
            KeyCode::PageUp => {
                if let Some(p) = self.keybinding_editor.as_mut() {
                    p.select_up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(p) = self.keybinding_editor.as_mut() {
                    p.select_down(page);
                }
            }
            KeyCode::Enter => self.begin_rebind_selected_keybinding(),
            KeyCode::Delete => self.reset_selected_keybinding(),
            KeyCode::Char(c) if !Self::ctrl(&key) && !Self::alt(&key) => {
                if let Some(p) = self.keybinding_editor.as_mut() {
                    p.push(c);
                }
            }
            _ => {}
        }
    }

    /// Mouse handling for the keybinding editor overlay: the wheel scrolls
    /// the selection; a click on a column header sorts by it.
    fn keybinding_editor_mouse(&mut self, mouse: MouseEvent) {
        use vix_keybinding_editor_panel::Column;
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                if let Some(p) = self.keybinding_editor.as_mut() {
                    p.select_up(3);
                }
            }
            MouseEventKind::ScrollDown => {
                if let Some(p) = self.keybinding_editor.as_mut() {
                    p.select_down(3);
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let headers = self.layout.keybinding_editor_headers;
                let col = if rect_contains(headers[0], mouse.column, mouse.row) {
                    Some(Column::Action)
                } else if rect_contains(headers[1], mouse.column, mouse.row) {
                    Some(Column::Keys)
                } else {
                    None
                };
                if let (Some(col), Some(p)) = (col, self.keybinding_editor.as_mut()) {
                    p.toggle_sort(col);
                }
            }
            _ => {}
        }
    }

    /// Enter (or a click, T204 roadmap) on the selected row: stash its
    /// action id and open a prompt for the replacement key, typed as a
    /// `vix-macros` token — see [`App::accept_rebind_key`].
    fn begin_rebind_selected_keybinding(&mut self) {
        let Some((action_id, action_title)) = self
            .keybinding_editor
            .as_ref()
            .and_then(vix_keybinding_editor_panel::Panel::selected_row)
            .map(|r| (r.action_id.clone(), r.action_title.clone()))
        else {
            return;
        };
        self.pending_rebind_action_id = Some(action_id);
        self.prompt = Some(Prompt::new(
            PromptKind::RebindKey,
            t!("prompt.rebind_key", action = action_title).to_string(),
        ));
    }

    /// `PromptKind::RebindKey`'s accept handler: validate the typed token
    /// via `vix_macros::decode_key` (rejecting anything it can't parse),
    /// then re-encode the decoded key through `vix_macros::encode_key` so
    /// the persisted token is always in canonical form regardless of how
    /// it was typed. Persists via `vix_keybindings::user_bindings::upsert`
    /// and re-resolves — a rebind that collides with another override is
    /// reported the same way `keybindings.reload` reports any conflict,
    /// not specially here.
    fn accept_rebind_key(&mut self, input: &str) {
        let Some(action_id) = self.pending_rebind_action_id.take() else {
            return;
        };
        if input.is_empty() {
            return;
        }
        let Some(key_event) = vix_macros::decode_key(input) else {
            self.messages
                .error(t!("msg.rebind_invalid_token", token = input.to_string()).to_string());
            return;
        };
        let key_token = vix_macros::encode_key(key_event);
        let Some(path) = Settings::keybindings_path() else {
            return;
        };
        if let Err(e) = vix_keybindings::user_bindings::upsert(
            &path,
            vix_keybindings::UserBinding {
                key_token: key_token.clone(),
                action_id,
            },
        ) {
            self.messages
                .error(t!("msg.keybinding_save_failed", error = e.to_string()).to_string());
            return;
        }
        self.resolve_key_overrides();
        self.refresh_keybinding_editor();
        self.messages
            .info(t!("msg.keybinding_rebound", token = key_token).to_string());
    }

    /// Delete on the selected row: only acts when it's resettable (a user
    /// override — `Row::resettable`), removing it from `keybindings.toml`
    /// and re-resolving so the built-in (or script) binding takes back
    /// over.
    fn reset_selected_keybinding(&mut self) {
        let Some(key_token) = self
            .keybinding_editor
            .as_ref()
            .and_then(vix_keybinding_editor_panel::Panel::selected_row)
            .filter(|r| r.resettable())
            .map(|r| r.key_token.clone())
        else {
            return;
        };
        let Some(path) = Settings::keybindings_path() else {
            return;
        };
        if let Err(e) = vix_keybindings::user_bindings::remove(&path, &key_token) {
            self.messages
                .error(t!("msg.keybinding_save_failed", error = e.to_string()).to_string());
            return;
        }
        self.resolve_key_overrides();
        self.refresh_keybinding_editor();
        self.messages
            .info(t!("msg.keybinding_reset", token = key_token).to_string());
    }

    /// The persisted/script key-binding override choke point (T104i),
    /// run between `org_table_key` and every keymap's own dispatch — the
    /// *only* new call site, covering all 10 keymaps at once (see
    /// `crates/vix-keybindings/spec/index.md`'s "One choke point"/
    /// "Override layer"). Builds the incoming key's plain `vix-macros`
    /// token — the shared grammar every override source is authored in,
    /// not any single keymap's own Shift-bit-explicit convention — and
    /// looks it up in `self.key_overrides`. Returns true if consumed.
    fn override_key(&mut self, key: KeyEvent) -> bool {
        let token = crate::macros::encode_key(key);
        if token.is_empty() {
            return false;
        }
        if let Some(action) = self.key_overrides.get(&token).cloned() {
            self.run_action(&action);
            return true;
        }
        false
    }

    // ----- Project (vix-tasks wiring) --------------------------------------
    //
    // `vix-tasks` provides the pure logic (project-type detection, lifecycle
    // command resolution, task discovery/merging, subprojects, history
    // policy, test-at-point); everything host-side — filesystem
    // reads, the confirm prompt, running commands, and persistence — lives
    // here. Two-tier persistence: the shareable `<root>/.vix/project.toml`
    // override (`App::load_project_override`), and the private per-user
    // cache/history in the session store (`App::project_command_cache`/
    // `project_history`, lazily loaded by `ensure_project_session_loaded`
    // and saved in `save_session`).

    /// Copy `prior`'s persisted project cache/history/last-command into `ws`.
    /// Called by `save_session` before any in-memory overlay so a run that
    /// never touches a `project.*` action carries the previous save forward
    /// instead of overwriting it with empty defaults.
    fn carry_forward_project_fields(
        ws: &mut crate::session::WorkspaceSession,
        prior: &crate::session::WorkspaceSession,
    ) {
        ws.project_cmd_configure
            .clone_from(&prior.project_cmd_configure);
        ws.project_cmd_compile
            .clone_from(&prior.project_cmd_compile);
        ws.project_cmd_test.clone_from(&prior.project_cmd_test);
        ws.project_cmd_install
            .clone_from(&prior.project_cmd_install);
        ws.project_cmd_package
            .clone_from(&prior.project_cmd_package);
        ws.project_cmd_run.clone_from(&prior.project_cmd_run);
        ws.project_history_configure
            .clone_from(&prior.project_history_configure);
        ws.project_history_compile
            .clone_from(&prior.project_history_compile);
        ws.project_history_test
            .clone_from(&prior.project_history_test);
        ws.project_history_install
            .clone_from(&prior.project_history_install);
        ws.project_history_package
            .clone_from(&prior.project_history_package);
        ws.project_history_run
            .clone_from(&prior.project_history_run);
        ws.project_last_command
            .clone_from(&prior.project_last_command);
    }

    /// Load the shareable per-project lifecycle command override from
    /// `<root>/.vix/project.toml` (a per-project override file — meant to be
    /// checked into the repo and shared with a team). Missing file or
    /// unparseable TOML both fall through to an all-`None` override,
    /// matching `crate::tasks::load`'s "missing/bad file → empty" convention.
    fn load_project_override(&self) -> crate::tasks::lifecycle::LifecycleCommands {
        #[derive(serde::Deserialize, Default)]
        struct ProjectOverrideFile {
            configure: Option<String>,
            compile: Option<String>,
            test: Option<String>,
            install: Option<String>,
            package: Option<String>,
            run: Option<String>,
        }
        let path = self.root.join(".vix").join("project.toml");
        let Ok(text) = std::fs::read_to_string(&path) else {
            return crate::tasks::lifecycle::LifecycleCommands::default();
        };
        let parsed: ProjectOverrideFile = toml::from_str(&text).unwrap_or_default();
        crate::tasks::lifecycle::LifecycleCommands {
            configure: parsed.configure,
            compile: parsed.compile,
            test: parsed.test,
            install: parsed.install,
            package: parsed.package,
            run: parsed.run,
        }
    }

    /// The names of every entry directly inside `dir`, or empty on any error
    /// (missing directory, permissions, …).
    fn dir_entry_names(dir: &Path) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .collect()
    }

    /// Detect the registered `vix-tasks` project type(s) for `dir` by
    /// marker-file presence, additionally inspecting `pyproject.toml`'s
    /// content (when present) to tell `python-poetry` apart from a generic
    /// PEP 621/Flit/Hatch project.
    fn detect_project_types_at(
        dir: &Path,
    ) -> Vec<&'static crate::tasks::project_type::ProjectType> {
        let names = Self::dir_entry_names(dir);
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let pyproject = if refs.contains(&"pyproject.toml") {
            std::fs::read_to_string(dir.join("pyproject.toml")).ok()
        } else {
            None
        };
        let contents: Vec<(&str, &str)> = pyproject
            .as_deref()
            .map(|body| ("pyproject.toml", body))
            .into_iter()
            .collect();
        crate::tasks::project_type::detect_project_types_with_content(&refs, &contents)
    }

    /// Resolve the effective lifecycle command for `slot` at `dir`: detect
    /// `dir`'s project type(s), merge their defaults, then apply the
    /// `.vix/project.toml` override and the session's resolved-command
    /// cache — `crate::tasks::lifecycle`'s own documented precedence.
    /// The override and cache are always the workspace root's (this crate
    /// keeps one flat override/cache per workspace, not one per
    /// subproject), so `project.subproject.*` actions only vary the
    /// *default* by directory.
    fn resolve_lifecycle(&mut self, slot: ProjectSlot, dir: &Path) -> Option<String> {
        self.ensure_project_session_loaded();
        let types = Self::detect_project_types_at(dir);
        let default = crate::tasks::lifecycle::default_lifecycle(&types);
        let over = self.load_project_override();
        let effective = crate::tasks::lifecycle::effective_lifecycle(
            &default,
            &over,
            &self.project_command_cache,
        );
        slot.get(&effective)
    }

    /// Open the confirm/edit prompt for `slot`, resolved at `dir` (the
    /// workspace root for the top-level `project.*` actions, or a
    /// subproject's directory for `project.subproject.*`). Reports and
    /// stops when the slot has no command for this project type.
    fn open_project_command_prompt(&mut self, slot: ProjectSlot, dir: Option<PathBuf>) {
        let detect_dir = dir.clone().unwrap_or_else(|| self.root.clone());
        let Some(cmd) = self.resolve_lifecycle(slot, &detect_dir) else {
            self.status = t!("status.project_no_command").to_string();
            return;
        };
        self.pending_project_command = Some(PendingProjectCommand { slot, dir });
        self.prompt = Some(
            Prompt::new(
                PromptKind::ProjectCommand,
                t!(slot.prompt_title_key()).to_string(),
            )
            .with_input(cmd),
        );
    }

    /// Handle the completed `project.*`/`project.subproject.*` confirm
    /// prompt: run the (possibly edited) command, cache it as this slot's
    /// new resolved command, and push it onto the slot's history and the
    /// project's last-command-of-any-kind.
    fn accept_project_command_prompt(&mut self, input: &str) {
        let Some(pending) = self.pending_project_command.take() else {
            return;
        };
        let cmd = input.trim();
        if cmd.is_empty() {
            return;
        }
        match &pending.dir {
            Some(dir) => self.run_command_in(dir, cmd),
            None => self.run_command(cmd),
        }
        pending
            .slot
            .set(&mut self.project_command_cache, Some(cmd.to_string()));
        crate::tasks::history::push_history(
            pending.slot.history_mut(&mut self.project_history),
            cmd.to_string(),
            crate::tasks::history::DuplicatePolicy::IgnoreConsecutive,
        );
        self.project_last_command = Some(cmd.to_string());
    }

    /// Re-run the most recently run project command of any kind (a lifecycle
    /// command or a named task) — `project.repeat_last_task`.
    fn repeat_last_project_task(&mut self) {
        self.ensure_project_session_loaded();
        let Some(cmd) = self.project_last_command.clone() else {
            self.status = t!("status.project_no_last_command").to_string();
            return;
        };
        self.run_command(&cmd);
    }

    /// Clear the session's cached lifecycle commands for this workspace root
    /// — `project.discard_command_cache`. Leaves history and the
    /// `.vix/project.toml` override untouched.
    fn discard_project_command_cache(&mut self) {
        self.ensure_project_session_loaded();
        self.project_command_cache = crate::tasks::lifecycle::LifecycleCommands::default();
        self.status = t!("status.project_cache_discarded").to_string();
    }

    /// Read whichever discovered-task manifests are present at `dir` and
    /// return the merged `Discovered`-tier tasks (all seven sources
    /// `vix-tasks` supports). Cheap existence checks before each read.
    fn discovered_tasks_at(dir: &Path) -> Vec<crate::tasks::task::NamedTask> {
        let read = |name: &str| std::fs::read_to_string(dir.join(name)).ok();
        let mut out = Vec::new();
        if let Some(body) = read("package.json") {
            let names = Self::dir_entry_names(dir);
            let refs: Vec<&str> = names.iter().map(String::as_str).collect();
            let manager = crate::tasks::project_type::detect_package_manager(&refs);
            out.extend(crate::tasks::discover_npm::discover_npm_scripts(
                &body, manager,
            ));
        }
        if let Some(body) = read("deno.json").or_else(|| read("deno.jsonc")) {
            out.extend(crate::tasks::discover_deno::discover_deno_tasks(&body));
        }
        if let Some(body) = read("composer.json") {
            out.extend(crate::tasks::discover_composer::discover_composer_scripts(
                &body,
            ));
        }
        if let Some(body) = read("justfile")
            .or_else(|| read(".justfile"))
            .or_else(|| read("Justfile"))
        {
            out.extend(crate::tasks::discover_just::discover_just_recipes(&body));
        }
        if let Some(body) = read("Taskfile.yml").or_else(|| read("Taskfile.yaml")) {
            out.extend(crate::tasks::discover_taskfile::discover_taskfile_tasks(
                &body,
            ));
        }
        // Rake: the top-level `Rakefile` plus any `*.rake` files directly at
        // the root or under `rakelib/` — a practical subset of "the Rakefile
        // plus any .rake files", not an exhaustive recursive scan.
        let mut rakefiles: Vec<(String, String)> = Vec::new();
        if let Some(body) = read("Rakefile") {
            rakefiles.push(("Rakefile".to_string(), body));
        }
        let is_rake_file = |name: &str| {
            Path::new(name)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("rake"))
        };
        for name in Self::dir_entry_names(dir) {
            if is_rake_file(&name)
                && let Some(body) = read(&name)
            {
                rakefiles.push((name, body));
            }
        }
        let rakelib = dir.join("rakelib");
        if rakelib.is_dir() {
            for name in Self::dir_entry_names(&rakelib) {
                if is_rake_file(&name)
                    && let Ok(body) = std::fs::read_to_string(rakelib.join(&name))
                {
                    rakefiles.push((format!("rakelib/{name}"), body));
                }
            }
        }
        if !rakefiles.is_empty() {
            let use_bundler = dir.join("Gemfile").is_file();
            let refs: Vec<(&str, &str)> = rakefiles
                .iter()
                .map(|(n, b)| (n.as_str(), b.as_str()))
                .collect();
            out.extend(crate::tasks::discover_rake::discover_rake_tasks(
                &refs,
                use_bundler,
            ));
        }
        if let Some(body) = read("Makefile")
            .or_else(|| read("makefile"))
            .or_else(|| read("GNUmakefile"))
        {
            out.extend(crate::tasks::discover_make::discover_make_targets(&body));
        }
        out
    }

    /// Open the merged task chooser (`project.run_task`): user-configured
    /// tasks (`tasks.toml`) plus discovered tasks from any present
    /// manifests, merged with `crate::tasks::task::merge_tasks`. The
    /// project-type tier is always empty — `crate::tasks::ProjectType`
    /// carries no named tasks of its own, only the six lifecycle slots (see
    /// `crates/vix-tasks/spec/index.md`).
    fn open_project_run_task(&mut self) {
        let user = crate::tasks::task::from_vix_tasks(&crate::tasks::load(&self.root));
        let discovered = Self::discovered_tasks_at(&self.root);
        let merged = crate::tasks::task::merge_tasks(&[], &user, &discovered);
        if merged.is_empty() {
            self.status = t!("status.no_tasks").to_string();
            return;
        }
        self.task_chooser = Some(TaskChooser {
            tasks: merged,
            selected: 0,
        });
    }

    /// Resolve the test at the active tab's cursor line
    /// (`crate::tasks::test_at_point`) and run it immediately, with no
    /// confirm prompt — matching the manual's own "runs it right away"
    /// behavior for this one command, since re-confirming would defeat the
    /// point of a fast "run the test under my cursor" binding. Does not
    /// touch the project command cache or history — this command
    /// deliberately keeps its own note.
    fn project_test_at_point(&mut self) {
        let Some(tab) = self.editor.active_tab() else {
            return;
        };
        let Some(path) = tab.path.clone() else {
            self.status = t!("status.project_test_no_file").to_string();
            return;
        };
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_string();
        let rel = path
            .strip_prefix(&self.root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let text = tab.text();
        let lines: Vec<&str> = text.lines().collect();
        let cursor_line = tab.editor.cursor_line();
        match crate::tasks::test_at_point::test_at_point(
            &ext,
            &rel,
            &lines,
            cursor_line,
            crate::tasks::test_at_point::TestFrameworkHint::Unknown,
        ) {
            Some(m) => self.run_command(&m.command),
            None => self.status = t!("status.project_no_test_at_point").to_string(),
        }
    }

    /// Every subproject below the workspace root (monorepo support), from
    /// the already gitignore-aware, vendor-pruned file index.
    fn subprojects(&self) -> Vec<crate::tasks::subproject::Subproject> {
        let relative_paths: Vec<String> = self
            .file_index
            .iter()
            .filter_map(|p| p.strip_prefix(&self.root).ok())
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect();
        let refs: Vec<&str> = relative_paths.iter().map(String::as_str).collect();
        crate::tasks::subproject::find_subprojects(&refs)
    }

    /// The nearest enclosing subproject for the active tab's file, or `None`
    /// if it has none (no path, or no subproject manifest between it and the
    /// workspace root).
    fn nearest_subproject_for_active_file(
        &mut self,
    ) -> Option<crate::tasks::subproject::Subproject> {
        self.build_file_index();
        let path = self.editor.active_tab().and_then(|t| t.path.clone())?;
        let rel = path
            .strip_prefix(&self.root)
            .ok()?
            .to_string_lossy()
            .replace('\\', "/");
        let subs = self.subprojects();
        crate::tasks::subproject::nearest_subproject(&subs, &rel).cloned()
    }

    /// `project.subproject.find_file`: open the palette's fuzzy file finder
    /// scoped to the nearest subproject's files, reusing the existing
    /// chooser rather than building a second one.
    fn open_subproject_find_file(&mut self) {
        let Some(sub) = self.nearest_subproject_for_active_file() else {
            self.status = t!("status.project_no_subproject").to_string();
            return;
        };
        self.palette_file_scope = Some(sub.relative_root);
        self.open_palette_seeded("");
    }

    /// `project.subproject.{configure,compile,test,install,package,run}`:
    /// same flow as the top-level lifecycle actions, but project-type
    /// detection and the run's working directory are the nearest enclosing
    /// subproject's, not the workspace root's.
    fn open_subproject_command_prompt(&mut self, slot: ProjectSlot) {
        let Some(sub) = self.nearest_subproject_for_active_file() else {
            self.status = t!("status.project_no_subproject").to_string();
            return;
        };
        let dir = self.root.join(&sub.relative_root);
        self.open_project_command_prompt(slot, Some(dir));
    }

    // ----- Compare With File (diff view) ----------------------------------

    /// Prompt for a file path to compare the active buffer against.
    fn open_compare_prompt(&mut self) {
        if self.editor.active_tab().is_none() {
            return;
        }
        self.prompt = Some(Prompt::new(
            PromptKind::CompareFile,
            t!("prompt.compare_file").to_string(),
        ));
    }

    // ----- Insert File / Revert -------------------------------------------

    /// Prompt for a file whose contents to insert at the cursor.
    fn open_insert_file_prompt(&mut self) {
        if self.editor.active_tab().is_none() {
            return;
        }
        self.prompt = Some(Prompt::new(
            PromptKind::InsertFile,
            t!("prompt.insert_file").to_string(),
        ));
    }

    /// Insert the contents of the file at `input` (resolved relative to the
    /// workspace root) into the active buffer at the cursor.
    fn insert_file_at_cursor(&mut self, input: &str) {
        if input.is_empty() {
            return;
        }
        let path = self.resolve(input);
        let Ok(text) = std::fs::read_to_string(&path) else {
            self.messages
                .error(t!("msg.open_failed", error = path.display()).to_string());
            return;
        };
        let area = self.layout.editor;
        if self.editor.insert_str(&text, area) {
            self.status = t!("status.inserted_file", path = path.display()).to_string();
        }
    }

    /// Revert the active buffer to its on-disk contents, discarding unsaved
    /// edits. The reload is a single undo step, so a mistaken revert can be
    /// undone; the cursor stays where it still fits.
    fn revert_active(&mut self) {
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        if tab.is_image() {
            return;
        }
        let Some(path) = tab.path.clone() else {
            self.status = t!("status.revert_no_file").to_string();
            return;
        };
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                let cursor = tab.editor.get_cursor();
                tab.editor.set_content(&content);
                tab.editor.set_cursor(cursor.min(content.chars().count()));
                tab.dirty = false;
                self.status = t!("status.reverted", path = path.display()).to_string();
            }
            Err(e) => self
                .messages
                .error(t!("msg.open_failed", error = e).to_string()),
        }
    }

    /// Toggle the editor's line-number gutter, driven by the editor panel's own
    /// `toggle_line_numbers`, then mirrored across every tab and persisted.
    fn toggle_editor_line_numbers(&mut self) {
        let fallback = !self
            .editor
            .flags
            .contains(crate::editor::Flags::LINE_NUMBERS);
        let on = self
            .editor
            .active_tab_mut()
            .map_or(fallback, |t| t.editor.toggle_line_numbers());
        self.editor
            .flags
            .set(crate::editor::Flags::LINE_NUMBERS, on);
        self.editor.refresh_line_numbers();
        self.settings.line_numbers = on;
        self.status = if on {
            t!("status.line_numbers_on")
        } else {
            t!("status.line_numbers_off")
        }
        .to_string();
    }

    /// Turn the current line's text into a comment banner: the title bordered
    /// above and below by a rule of `=`, each prefixed with the language's line
    /// comment. An empty line uses a placeholder title.
    fn comment_banner(&mut self) {
        if self.active_read_only() {
            self.status = t!("status.read_only_blocked").to_string();
            return;
        }
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        if tab.is_image() {
            return;
        }
        let prefix = tab.editor.comment_prefix();
        let prefix = if prefix.is_empty() {
            "#".to_string()
        } else {
            prefix
        };
        let line = tab.editor.cursor_line();
        let mut lines: Vec<String> = tab
            .editor
            .get_content()
            .split('\n')
            .map(str::to_string)
            .collect();
        let Some(cur) = lines.get(line) else { return };
        let indent: String = cur.chars().take_while(|c| c.is_whitespace()).collect();
        let title = cur.trim();
        let title = if title.is_empty() { "Section" } else { title };
        let rule = "=".repeat(title.chars().count().clamp(8, 60));
        let banner =
            format!("{indent}{prefix} {rule}\n{indent}{prefix} {title}\n{indent}{prefix} {rule}");
        lines[line] = banner;
        tab.editor.set_content(&lines.join("\n"));
        tab.editor.set_cursor_line(line + 1);
        tab.dirty = true;
    }

    /// Whether the active buffer is locked against edits.
    fn active_read_only(&self) -> bool {
        self.editor.active_tab().is_some_and(|t| t.read_only)
    }

    /// Toggle the active buffer's read-only lock.
    fn toggle_read_only(&mut self) {
        let on = if let Some(t) = self.editor.active_tab_mut() {
            t.read_only = !t.read_only;
            t.read_only
        } else {
            return;
        };
        self.status = t!("status.read_only", on = on).to_string();
    }

    /// Toggle relative (hybrid) line numbering, mirrored across every tab and
    /// persisted.
    fn toggle_relative_line_numbers(&mut self) {
        let on = !self.settings.relative_line_numbers;
        self.settings.relative_line_numbers = on;
        self.editor
            .flags
            .set(crate::editor::Flags::RELATIVE_LINE_NUMBERS, on);
        self.editor.refresh_line_numbers();
        self.status = t!("status.relative_line_numbers", on = on).to_string();
    }

    /// Toggle the editor's visible-whitespace glyphs, mirrored across every tab
    /// and persisted.
    fn toggle_editor_whitespace(&mut self) {
        let fallback = !self
            .editor
            .flags
            .contains(crate::editor::Flags::SHOW_WHITESPACE);
        let on = self
            .editor
            .active_tab_mut()
            .map_or(fallback, |t| t.editor.toggle_whitespace());
        self.editor
            .flags
            .set(crate::editor::Flags::SHOW_WHITESPACE, on);
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
        let fallback = !self.editor.flags.contains(crate::editor::Flags::SOFT_WRAP);
        let on = self
            .editor
            .active_tab_mut()
            .map_or(fallback, |t| t.editor.toggle_soft_wrap());
        self.editor.flags.set(crate::editor::Flags::SOFT_WRAP, on);
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
        // Org view-only keys (drawer fold on Tab, agenda `t`) run ahead of the
        // read-only guard because they never edit the buffer text.
        if self.org_view_key(key) {
            return;
        }
        // Read-only buffers accept navigation but not edits.
        if self.active_read_only() && (Self::is_edit_key(&key) || key.code == KeyCode::Delete) {
            self.status = t!("status.read_only_blocked").to_string();
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
        // A Tab or Alt+Tab (M-Tab) right after typing `[[mailto:`/`[[contact:` in
        // an Org buffer opens Org-contacts email/name completion.
        if matches!(key.code, KeyCode::Tab)
            && !Self::ctrl(&key)
            && !Self::shift(&key)
            && self.maybe_complete_org_contact_link()
        {
            return;
        }
        // A plain Tab after a snippet prefix word expands that snippet.
        if matches!(key.code, KeyCode::Tab)
            && !Self::ctrl(&key)
            && !Self::alt(&key)
            && !Self::shift(&key)
            && self.expand_snippet_prefix()
        {
            return;
        }
        let area = self.editor_view();
        match key.code {
            KeyCode::Home => return self.editor.cursor_line_home(),
            KeyCode::End => return self.editor.cursor_line_end(),
            KeyCode::Delete => {
                let multi = self
                    .editor
                    .active_tab()
                    .is_some_and(|t| t.editor.has_multi_carets());
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
                self.editor
                    .page_up(area.height.saturating_sub(2).max(1) as usize);
                return;
            }
            KeyCode::PageDown => {
                self.editor
                    .page_down(area.height.saturating_sub(2).max(1) as usize);
                return;
            }
            _ => {}
        }

        // Overwrite mode: a plain character types over the one under the cursor
        // (delete it first, then the normal insert below replaces it). Skipped at
        // end-of-line, with a selection, or with multiple carets.
        if self.overwrite
            && matches!(key.code, KeyCode::Char(_))
            && !Self::ctrl(&key)
            && !Self::alt(&key)
        {
            let over_char = self.editor.active_tab_mut().is_some_and(|t| {
                if t.editor.has_multi_carets()
                    || t.editor.get_selection().is_some_and(|s| !s.is_empty())
                {
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
        self.prompt = Some(Prompt::new(
            PromptKind::SaveMacro,
            t!("prompt.save_macro").to_string(),
        ));
    }

    /// Persist the recorded macro under `name` to `macros.toml`.
    fn save_macro(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() || self.macro_keys.is_empty() {
            return;
        }
        let Some(path) = Settings::macros_path() else {
            return;
        };
        let mac = crate::macros::Macro {
            name: name.to_string(),
            keys: crate::macros::encode(&self.macro_keys),
        };
        match crate::macros::upsert(&path, mac) {
            Ok(()) => self.status = t!("status.macro_saved", name = name).to_string(),
            Err(e) => self
                .messages
                .error(t!("msg.save_failed", error = e).to_string()),
        }
    }

    /// Open the saved-macro chooser, or report when none are saved.
    /// Push `text` onto the clipboard history (most-recent first, de-duplicated,
    /// capped). Empty/`None` entries are ignored.
    fn record_clipboard(&mut self, text: Option<String>) {
        let Some(text) = text.filter(|s| !s.is_empty()) else {
            return;
        };
        self.clipboard_ring.retain(|e| *e != text);
        self.clipboard_ring.insert(0, text);
        self.clipboard_ring.truncate(30);
    }

    /// Open the clipboard-history picker (Edit → Paste from History…).
    fn open_clipboard_chooser(&mut self) {
        if self.clipboard_ring.is_empty() {
            self.status = t!("status.clipboard_empty").to_string();
            return;
        }
        self.clipboard_chooser = Some(ClipboardChooser {
            entries: self.clipboard_ring.clone(),
            selected: 0,
        });
    }

    fn clipboard_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(c) = self.clipboard_chooser.as_mut() {
                    let n = c.entries.len();
                    c.selected = (c.selected + n - 1) % n;
                }
            }
            KeyCode::Down => {
                if let Some(c) = self.clipboard_chooser.as_mut() {
                    c.selected = (c.selected + 1) % c.entries.len();
                }
            }
            KeyCode::Enter => self.paste_selected_clipboard(),
            KeyCode::Esc => self.clipboard_chooser = None,
            _ => {}
        }
    }

    fn clipboard_mouse(&mut self, mouse: MouseEvent) {
        if let Some(idx) = self.chooser_row(mouse)
            && let Some(c) = self.clipboard_chooser.as_mut()
            && idx < c.entries.len()
        {
            c.selected = idx;
            self.paste_selected_clipboard();
        }
    }

    /// Insert the highlighted clipboard-history entry at the cursor and close the
    /// picker (also promoting it to the front of the ring).
    fn paste_selected_clipboard(&mut self) {
        let Some(c) = self.clipboard_chooser.take() else {
            return;
        };
        let Some(text) = c.entries.get(c.selected).cloned() else {
            return;
        };
        let area = self.layout.editor;
        if self.editor.insert_str(&text, area) {
            self.mark_active_dirty();
        }
        self.record_clipboard(Some(text));
        self.focus = Focus::Editor;
    }

    fn open_macro_chooser(&mut self) {
        let macros = Settings::macros_path()
            .map(|p| crate::macros::load(&p))
            .unwrap_or_default();
        if macros.is_empty() {
            self.status = t!("status.no_macros").to_string();
            return;
        }
        self.macro_chooser = Some(MacroChooser {
            macros,
            selected: 0,
        });
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
        let Some(c) = self.macro_chooser.take() else {
            return;
        };
        if let Some(mac) = c.macros.get(c.selected) {
            self.macro_keys = crate::macros::decode(&mac.keys);
            self.focus = Focus::Editor;
            self.play_macro();
        }
    }

    // ----- Recent-projects switcher ---------------------------------------

    /// Switch to the highlighted recent project.
    fn switch_to_selected_workspace(&mut self) {
        let Some(c) = self.workspace_chooser.take() else {
            return;
        };
        let Some(root) = c.roots.get(c.selected).cloned() else {
            return;
        };
        let path = PathBuf::from(&root);
        if !path.is_dir() {
            self.status = t!("status.project_missing", path = root).to_string();
            return;
        }
        self.switch_workspace(&path);
    }

    // ----- Workspaces (multi-folder, saved to a file) ----------------------

    /// Open the prompt for a workspace action (Save seeds a default path).
    fn prompt_workspace(&mut self, kind: PromptKind) {
        let (msg, seed) = match kind {
            PromptKind::WorkspaceOpen => (t!("prompt.workspace_open").to_string(), None),
            PromptKind::WorkspaceSave => {
                let p = self
                    .root
                    .join(format!("workspace.{}", crate::workspace::EXTENSION));
                (
                    t!("prompt.workspace_save").to_string(),
                    Some(p.to_string_lossy().into_owned()),
                )
            }
            _ => (t!("prompt.workspace_add_folder").to_string(), None),
        };
        let mut prompt = Prompt::new(kind, msg);
        if let Some(seed) = seed {
            prompt = prompt.with_input(seed);
        }
        self.prompt = Some(prompt);
    }

    /// Save the current workspace (all folders + open files) into `path`.
    fn workspace_save(&mut self, path: &str) {
        let path = self.resolve(path.trim());
        let ws = crate::workspace::Workspace {
            folders: self
                .workspace_folders
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            files: self
                .editor
                .tabs
                .iter()
                .filter_map(|t| t.path.as_ref())
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
        };
        match ws.save(&path) {
            Ok(()) => self.status = t!("status.workspace_saved", path = path.display()).to_string(),
            Err(e) => self
                .messages
                .error(t!("msg.save_failed", error = e).to_string()),
        }
    }

    /// Open a workspace file: re-root at its first folder, register the rest, and
    /// reopen its files.
    fn workspace_open(&mut self, path: &str) {
        let path = self.resolve(path.trim());
        let ws = match crate::workspace::Workspace::load(&path) {
            Ok(ws) => ws,
            Err(e) => {
                self.messages
                    .error(t!("msg.open_failed", error = e).to_string());
                return;
            }
        };
        // A workspace file is portable and may be attacker-supplied. Refuse one
        // that would re-root the file index at a filesystem root (`/`), which
        // would recursively walk the whole disk.
        if ws.has_root_or_empty_folder() {
            self.messages
                .error(t!("msg.workspace_unsafe_root").to_string());
            return;
        }
        let folders: Vec<PathBuf> = ws.folders.iter().map(PathBuf::from).collect();
        if let Some(primary) = folders.first() {
            self.switch_workspace(&primary.clone());
        }
        // switch_workspace reset workspace_folders to just the primary; register
        // the full set so the finder/search span them all.
        self.workspace_folders = if folders.is_empty() {
            vec![self.root.clone()]
        } else {
            folders
        };
        self.build_file_index();
        // Only auto-open files contained within the workspace's own folders, so a
        // crafted workspace can't silently open arbitrary system paths (e.g.
        // `/etc/passwd`, `~/.ssh/id_rsa`) into editor tabs.
        let skipped = ws.external_files().len();
        for f in &ws.files {
            if !ws.file_within_folders(f) {
                continue;
            }
            let p = PathBuf::from(f);
            if p.exists() {
                self.open_path(&p, false);
            }
        }
        self.status = if skipped > 0 {
            t!(
                "status.workspace_opened_skipped",
                path = path.display(),
                count = skipped
            )
            .to_string()
        } else {
            t!("status.workspace_opened", path = path.display()).to_string()
        };
    }

    /// Add a folder to the current workspace so the finder/search span it too.
    fn workspace_add_folder(&mut self, path: &str) {
        let folder = self.resolve(path.trim());
        if !folder.is_dir() {
            self.status = t!("status.workspace_not_a_folder").to_string();
            return;
        }
        if !self.workspace_folders.contains(&folder) {
            self.workspace_folders.push(folder.clone());
            self.build_file_index();
        }
        self.status = t!("status.workspace_folder_added", path = folder.display()).to_string();
    }

    /// Expand the Emmet abbreviation ending at the cursor (the contiguous
    /// non-whitespace run before it) into HTML, replacing it. No-op when the run
    /// doesn't parse as Emmet.
    fn emmet_expand(&mut self) {
        let Some(tab) = self.editor.active_tab() else {
            return;
        };
        let cur = tab.editor.get_cursor();
        let chars: Vec<char> = tab.editor.get_content().chars().collect();
        let mut start = cur.min(chars.len());
        while start > 0 && !chars[start - 1].is_whitespace() {
            start -= 1;
        }
        let abbr: String = chars[start..cur.min(chars.len())].iter().collect();
        let Some(html) = crate::emmet::expand(&abbr) else {
            self.status = t!("status.emmet_none").to_string();
            return;
        };
        let html = html.trim_end_matches('\n').to_string();
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_selection_range(start, cur);
        }
        let area = self.editor_view();
        self.editor.insert_str(&html, area);
        self.status = t!("status.emmet_expanded").to_string();
    }

    /// Buffer-word autocomplete: complete the word before the cursor from other
    /// words in the buffer, cycling on repeated calls (`forward` chooses the
    /// direction). Like classic "dynamic abbreviation" expansion.
    fn autocomplete(&mut self, forward: bool) {
        let Some(cur) = self
            .editor
            .active_tab()
            .filter(|t| !t.is_image())
            .map(|t| t.editor.get_cursor())
        else {
            return;
        };
        // Continue an active cycle when the cursor still sits at the last insert.
        let cycling = self.complete_session.as_ref().is_some_and(|s| s.end == cur);
        if cycling {
            let s = self.complete_session.as_mut().unwrap();
            let n = s.candidates.len();
            s.index = if forward {
                (s.index + 1) % n
            } else {
                (s.index + n - 1) % n
            };
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
            return matches!(key.code, KeyCode::Char('v' | 'x' | 'z' | 'Z' | 'k' | 'd'));
        }
        matches!(
            key.code,
            KeyCode::Char(_)
                | KeyCode::Enter
                | KeyCode::Backspace
                | KeyCode::Tab
                | KeyCode::BackTab
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
        let Some(t) = self.editor.active_tab() else {
            return (0, 0);
        };
        let code = t.editor.code_ref();
        let cur = t.editor.get_cursor();
        let line = code.char_to_line(cur);
        let line_start = code.line_to_char(line);
        let line_text = code.slice(line_start, line_start + code.line_len(line));
        let character = crate::lsp_core::position::char_to_col(&line_text, cur - line_start, enc);
        (u32::try_from(line).unwrap_or(0), character)
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
                crate::lsp::LspEvent::Definition {
                    path,
                    line,
                    character,
                } => {
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
                crate::lsp::LspEvent::CallHierarchyPrepared(item) => {
                    if let Some(path) = self.active_path()
                        && self.lsp.handles(&path)
                    {
                        self.lsp.request_incoming_calls(&path, item);
                    }
                }
                crate::lsp::LspEvent::Edits(edits) => self.apply_lsp_edits(&edits),
                crate::lsp::LspEvent::DocumentSymbols(syms) => self.show_document_symbols(&syms),
                crate::lsp::LspEvent::WorkspaceSymbols(syms) => self.show_workspace_symbols(&syms),
                crate::lsp::LspEvent::WorkspaceEdit(edits) => self.apply_workspace_edit(&edits),
                crate::lsp::LspEvent::CodeActions(actions) => {
                    self.code_actions = Some(CodeActionMenu {
                        actions,
                        selected: 0,
                    });
                }
                crate::lsp::LspEvent::CodeLenses(lenses) => {
                    self.code_lens = Some(CodeLensMenu {
                        lenses,
                        selected: 0,
                    });
                }
                crate::lsp::LspEvent::SelectionRanges(ranges) => {
                    self.apply_selection_range(&ranges);
                }
                crate::lsp::LspEvent::Highlights(ranges) => self.apply_document_highlights(&ranges),
                crate::lsp::LspEvent::InlayHints(hints) => self.apply_inlay_hints(&hints),
                crate::lsp::LspEvent::LinkedRanges(ranges) => self.begin_linked_edit(&ranges),
                crate::lsp::LspEvent::FoldingRanges(ranges) => {
                    let ranges: Vec<(usize, usize)> = ranges
                        .iter()
                        .map(|&(s, e)| (s as usize, e as usize))
                        .collect();
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

    /// The identifier characters immediately before the cursor (the typed prefix
    /// a completion should extend).
    fn word_prefix_before_cursor(&self) -> String {
        let Some(t) = self.editor.active_tab() else {
            return String::new();
        };
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
    /// When the cursor sits just after a freshly typed `[[` in an `.org` file,
    /// open the node-title completion popup (Org-roam/Node wiki-link completion).
    fn maybe_complete_wiki_link(&mut self) {
        if self.completion.is_some() {
            return;
        }
        let Some(path) = self.active_path() else {
            return;
        };
        if path.extension().and_then(|e| e.to_str()) != Some("org") {
            return;
        }
        let Some(tab) = self.editor.active_tab() else {
            return;
        };
        let cur = tab.editor.get_cursor();
        if cur < 2 || tab.editor.code_ref().char_slice(cur - 2, cur) != "[[" {
            return;
        }
        self.open_node_link_completion();
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
        self.status = (if cut {
            t!("status.cut_n", n = n)
        } else {
            t!("status.copied_n", n = n)
        })
        .to_string();
    }

    fn explorer_paste(&mut self) {
        if self.clip.is_empty() {
            return;
        }
        let target = match self.explorer.selected_node() {
            Some(n) if n.is_dir => n.path.clone(),
            Some(n) => n
                .path
                .parent()
                .map_or_else(|| self.root.clone(), Path::to_path_buf),
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
                    && op.cut
                {
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
                    op.queue
                        .front()
                        .map(|s| (s.clone(), op.target.clone(), op.cut))
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
        self.editor
            .tabs
            .iter()
            .position(|t| t.dirty && !t.is_image())
    }

    /// Close the active tab, prompting first if it has unsaved changes.
    fn request_close_active(&mut self) {
        if self
            .editor
            .active_tab()
            .is_some_and(|t| t.dirty && !t.is_image())
        {
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

    /// Whether Delete should move to the OS trash rather than remove
    /// outright (T209): the `explorer_delete` setting, defaulting to the
    /// safer trash behavior for any value other than the literal `"hard"`
    /// (including an old config file that predates this setting, or a typo).
    fn explorer_delete_uses_trash(&self) -> bool {
        self.settings.explorer_delete != "hard"
    }

    fn explorer_delete_request(&mut self) {
        let paths = self.explorer.selected_paths();
        if paths.is_empty() {
            return;
        }
        let key = if self.explorer_delete_uses_trash() {
            "confirm.delete"
        } else {
            "confirm.delete_hard"
        };
        self.confirm = Some(Confirm {
            message: t!(key, n = paths.len()).to_string(),
            paths,
        });
    }

    fn confirm_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y' | 'Y') => {
                if let Some(c) = self.confirm.take() {
                    let use_trash = self.explorer_delete_uses_trash();
                    let mut removed = 0;
                    for path in &c.paths {
                        // Canonicalize before removing so buffer paths still match.
                        let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
                        let result = if use_trash {
                            crate::fileops::trash_path(path)
                        } else {
                            crate::fileops::remove_path(path)
                        };
                        match result {
                            Ok(()) => {
                                self.close_buffers_under(&canon);
                                removed += 1;
                            }
                            Err(e) => self
                                .messages
                                .error(t!("msg.delete_failed", error = e).to_string()),
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
            && !node.is_dir
            && !is_image_path(&node.path)
        {
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
        let before = self.editor.tabs.len();
        match self.editor.open(path, preview) {
            Ok(()) => {
                let opened_new = self.editor.tabs.len() > before;
                if !preview {
                    self.editor.promote_active();
                    self.record_recent(path);
                    // Restore the persisted undo tree for a freshly opened file.
                    if opened_new && self.settings.persistent_undo {
                        self.restore_persistent_undo(path);
                    }
                }
                self.apply_editorconfig_indent(path);
                self.status = t!("status.opened", path = path.display()).to_string();
            }
            Err(e) => self
                .messages
                .error(t!("msg.open_failed", error = e).to_string()),
        }
    }

    /// Restore the active buffer's persisted undo tree, if one was saved for this
    /// file and its content still matches.
    fn restore_persistent_undo(&mut self, path: &Path) {
        let Some(content) = self.editor.active_tab().map(crate::editor::Tab::text) else {
            return;
        };
        if let Some(history) = crate::undo_store::load(path, &content)
            && let Some(tab) = self.editor.active_tab_mut()
        {
            tab.editor.code_mut().set_history(history);
        }
    }

    /// Apply the `.editorconfig` indent (style/size) for `path` to the active tab,
    /// when `EditorConfig` support is enabled and the file's config specifies one.
    fn apply_editorconfig_indent(&mut self, path: &Path) {
        let auto_pair = self.settings.auto_pair;
        let rainbow = self.settings.rainbow_brackets;
        let relative = self.settings.relative_line_numbers;
        if let Some(tab) = self.editor.active_tab_mut() {
            tab.editor.set_auto_pair(auto_pair);
            tab.editor.set_rainbow_brackets(rainbow);
            tab.editor.set_relative_line_numbers(relative);
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
        self.status = t!(if on {
            "status.auto_pair_on"
        } else {
            "status.auto_pair_off"
        })
        .to_string();
    }

    /// Toggle rainbow (depth-colored) brackets for every open buffer and persist it.
    fn toggle_rainbow_brackets(&mut self) {
        self.settings.rainbow_brackets = !self.settings.rainbow_brackets;
        let on = self.settings.rainbow_brackets;
        for tab in &mut self.editor.tabs {
            tab.editor.set_rainbow_brackets(on);
        }
        self.status = t!("status.rainbow_brackets", on = on).to_string();
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
        if let Some(i) = self
            .bookmarks
            .iter()
            .position(|b| b.path == loc.path && b.line == loc.line)
        {
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
        self.location_chooser = Some(LocationChooser {
            entries,
            selected: 0,
        });
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
            && self.nav_history.last() != Some(&o)
        {
            self.nav_history.push(o);
        }
        if let Some(d) = dest
            && self.nav_history.last() != Some(&d)
        {
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
        self.editor
            .goto(loc.line, Some(loc.col), self.editor_view());
        self.focus = Focus::Editor;
        self.status = format!("{}:{}", loc.path.display(), loc.line);
    }

    fn open_image(&mut self, path: &Path) {
        let Some(picker) = self.picker.as_ref() else {
            self.messages.warn(t!("msg.image_needs_terminal"));
            return;
        };
        match decode_image(path) {
            Ok(img) => {
                let proto = picker.new_resize_protocol(img);
                self.editor.open_image(path, proto);
                self.focus = Focus::Editor;
                self.status = t!("status.opened_image", path = path.display()).to_string();
            }
            Err(e) => self
                .messages
                .error(t!("msg.image_open_failed", error = e).to_string()),
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
                    if rect_contains(
                        self.layout.color_converter_rows[field.index()],
                        mouse.column,
                        mouse.row,
                    ) {
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
                for (i, focus) in [Focus::Value, Focus::From, Focus::To]
                    .into_iter()
                    .enumerate()
                {
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
    /// Mouse handling for the modal overlays (welcome, context menu and the
    /// right-click that opens it, the info dialog). Split out of
    /// [`App::try_overlay_mouse`] to keep it within the line limit.
    fn try_modal_mouse(&mut self, mouse: MouseEvent) -> bool {
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
        false
    }

    fn try_overlay_mouse(&mut self, mouse: MouseEvent) -> bool {
        if self.try_modal_mouse(mouse) {
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
        panel!(help, help_mouse);
        panel!(keybinding_editor, keybinding_editor_mouse);
        panel!(file_browser, file_browser_mouse);
        panel!(recent_chooser, recent_mouse);
        panel!(location_chooser, location_mouse);
        panel!(capture_chooser, capture_chooser_mouse);
        panel!(refile_chooser, refile_chooser_mouse);
        panel!(nerd_palette, nerd_mouse);
        panel!(ascii_panel, ascii_mouse);
        panel!(x11_panel, x11_mouse);
        panel!(theme_editor, theme_editor_mouse);
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
        panel!(script_chooser, script_chooser_mouse);
        panel!(clipboard_chooser, clipboard_mouse);
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
                && rect_contains(self.layout.pomodoro_button, mouse.column, mouse.row)
            {
                self.pomodoro_primary();
            }
            return true;
        }
        // Keyboard-only modal overlays swallow all mouse input rather than
        // letting a click fall through to the editor/explorer underneath.
        self.palette.is_some()
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
                if rect_contains(self.layout.minimap, col, row) =>
            {
                self.minimap_click(row);
                return true;
            }
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
        // Edges only exist once a render has recorded the dock's rectangle; a
        // zeroed (never-drawn) rect would otherwise claim row/column 0.
        let left_edge = (self.show_explorer && self.layout.explorer.width > 0)
            .then(|| self.layout.explorer.right().saturating_sub(1));
        let right_edge = (self.show_messages && self.layout.messages.width > 0)
            .then_some(self.layout.messages.x);
        // The bottom dock's top edge (its top border row), draggable to resize.
        let bottom_edge = (self.show_bottom_dock && self.layout.bottom_dock.height > 0)
            .then_some(self.layout.bottom_dock.y);
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
                    && self
                        .editor
                        .resize_split_at(self.layout.editor_region, col, row) =>
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
            && self
                .editor
                .focus_pane_at(self.layout.editor_region, col, row)
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
            let sb_rect = Rect {
                x: sb_col,
                y: a.y + 1,
                width: 1,
                height: a.height - 1,
            };
            self.bottom_dock.scroll = crate::ui::scrollbar_pos_from_row(
                sb_rect,
                mouse.row,
                total.saturating_sub(viewport),
            );
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

    /// Enter jump-to-line mode: assign a short label to each visible line.
    fn open_jump(&mut self) {
        let height = self.layout.editor.height as usize;
        if height == 0 {
            return;
        }
        let top = self.editor.top_visible_line();
        let total = self
            .editor
            .active_tab()
            .map_or(0, |t| t.text().lines().count().max(1));
        let labels: Vec<(String, usize)> = (0..height)
            .map(|r| top + r)
            .take_while(|&line| line < total)
            .enumerate()
            .map(|(i, line)| (jump_label(i), line))
            .collect();
        if labels.is_empty() {
            return;
        }
        self.jump = Some(JumpMode {
            labels,
            typed: String::new(),
        });
        self.focus = Focus::Editor;
    }

    /// Handle a key while jump-to-line mode is active. Returns `true` if consumed.
    /// Esc or any non-character key cancels; a character extends the typed prefix,
    /// jumping when it matches a label and staying only while a match is possible.
    fn jump_key(&mut self, key: KeyEvent) -> bool {
        let Some(mut jm) = self.jump.take() else {
            return false;
        };
        let KeyCode::Char(c) = key.code else {
            return true;
        }; // Esc/other: cancelled
        jm.typed.push(c);
        if let Some((_, line)) = jm.labels.iter().find(|(label, _)| *label == jm.typed) {
            let line = *line;
            let area = self.editor_view();
            self.editor.goto(line + 1, None, area); // goto is 1-based
            self.focus = Focus::Editor;
        } else if jm
            .labels
            .iter()
            .any(|(label, _)| label.starts_with(&jm.typed))
        {
            self.jump = Some(jm); // keep waiting: the prefix can still match
        }
        true
    }

    /// Jump to the source line corresponding to a click at minimap `row`.
    fn minimap_click(&mut self, row: u16) {
        let mm = self.layout.minimap;
        if mm.height == 0 {
            return;
        }
        let rel = row.saturating_sub(mm.y) as usize;
        let total = self
            .editor
            .active_tab()
            .map_or(1, |t| t.text().lines().count().max(1));
        let line = (rel * total / mm.height as usize).min(total.saturating_sub(1));
        let area = self.editor_view();
        self.editor.goto(line + 1, None, area); // goto is 1-based
        self.focus = Focus::Editor;
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
                let other = if self.show_messages {
                    self.settings.messages_width
                } else {
                    0
                };
                let max = full.saturating_sub(MIN_EDITOR + other).max(MIN_DOCK);
                let w = (col.saturating_sub(self.layout.explorer.x) + 1).clamp(MIN_DOCK, max);
                self.settings.explorer_width = w;
            }
            Some(DockResize::Right) => {
                let other = if self.show_explorer {
                    self.settings.explorer_width
                } else {
                    0
                };
                let max = full.saturating_sub(MIN_EDITOR + other).max(MIN_DOCK);
                let w = self
                    .layout
                    .messages
                    .right()
                    .saturating_sub(col)
                    .clamp(MIN_DOCK, max);
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
        let last = self
            .editor
            .split_layout(self.layout.editor_region)
            .len()
            .saturating_sub(1);
        self.editor.focus_leaf(idx.min(last));
        self.focus = Focus::Editor;
    }

    fn editor_mouse(&mut self, mouse: MouseEvent) {
        self.focus = Focus::Editor;
        let area = self.layout.editor;
        let alt = mouse
            .modifiers
            .contains(crossterm::event::KeyModifiers::ALT);
        if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
            // Alt+click adds an extra caret; a plain click collapses to one.
            if alt {
                if let Some(t) = self.editor.active_tab_mut()
                    && let Some(pos) = t.editor.cursor_from_mouse(mouse.column, mouse.row, &area)
                {
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
            let sb_rect = Rect {
                x: sb_col,
                y: inner_top,
                width: 1,
                height: a.height - 1,
            };
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
            let sb_rect = Rect {
                x: sb_col,
                y: inner_top,
                width: 1,
                height: area.height - 1,
            };
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
                    let offset = crate::ui::dropdown_scroll(
                        self.menu.subsub,
                        sd.height.saturating_sub(2) as usize,
                        items.len(),
                    );
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
                    let offset = crate::ui::dropdown_scroll(
                        self.menu.sub,
                        sd.height.saturating_sub(2) as usize,
                        items.len(),
                    );
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
                let offset = crate::ui::dropdown_scroll(
                    self.menu.item,
                    dd.height.saturating_sub(2) as usize,
                    items.len(),
                );
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
            self.toggle_menu(i);
        }
    }

    /// Open top-level menu `i`, or close the menu bar when that menu is already
    /// the open one. Menu names toggle: naming the open menu again — by click or
    /// by `Alt+<letter>` mnemonic — dismisses it, while naming another switches.
    fn toggle_menu(&mut self, i: usize) {
        if self.menu.open == Some(i) {
            self.close_menu();
        } else {
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
                && self.menu.open != Some(i)
            {
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
                    let offset = crate::ui::dropdown_scroll(
                        self.menu.subsub,
                        sd.height.saturating_sub(2) as usize,
                        items.len(),
                    );
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
                    let offset = crate::ui::dropdown_scroll(
                        self.menu.sub,
                        sd.height.saturating_sub(2) as usize,
                        items.len(),
                    );
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
                let offset = crate::ui::dropdown_scroll(
                    self.menu.item,
                    dd.height.saturating_sub(2) as usize,
                    items.len(),
                );
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
        if let Some(theme) = Self::available_custom_themes()
            .into_iter()
            .find(|t| t.name == name)
        {
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
            // Menu-bar mnemonics keep working while a dropdown is open: the open
            // menu's own letter closes it, another menu's letter switches to it.
            KeyCode::Char(c) if Self::alt(&key) => {
                if let Some(i) = menu_index_for_alt(c) {
                    self.toggle_menu(i);
                }
            }
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
            && let Some(it) = items.get(sidx)
        {
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
        let Some(theme) = Self::available_custom_themes()
            .into_iter()
            .find(|t| t.name == name)
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
        self.status = t!("status.locale", language = loc.code).to_string();
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
        self.modal_insert = false;
        self.vim_cmd = None;
        self.spacemacs_leader = None;
        self.vim_pending = None;
    }

    // ----- recent-files chooser -------------------------------------------

    /// Open the recent-files chooser, listing the saved recent paths that still
    /// exist. Does nothing (just a status note) when there are none.
    /// Open the keyboard-shortcut overlay (Help → Keyboard Shortcuts…, F1)
    /// over every active shortcut.
    fn open_help(&mut self) {
        self.help = Some(crate::keyboard_shortcut_panel::Panel::open(
            self.shortcut_rows(),
        ));
    }

    /// Every active keyboard shortcut as an action-name/key-combo row: a
    /// few purely-informational rows with no action id of their own
    /// (`ROWS`), every menu-item accelerator, and the active keymap's
    /// chord tables (the Spacemacs leader, the Emacs `Ctrl X` map). Titles
    /// come from [`Self::action_title`] — a menu label, else the
    /// `vix_action_catalog` entry for ids with no menu leaf (T147).
    /// Deduplicated on (action, keys), first source wins.
    fn shortcut_rows(&self) -> Vec<crate::keyboard_shortcut_panel::Shortcut> {
        use crate::keyboard_shortcut_panel::Shortcut;
        let mut out: Vec<Shortcut> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut add = |action: String, keys: String| {
            if !keys.is_empty() && seen.insert((action.to_lowercase(), keys.to_lowercase())) {
                out.push(Shortcut { action, keys });
            }
        };
        for r in crate::keyboard_shortcut_panel::ROWS {
            add(t!(r.desc).to_string(), r.keys.to_string());
        }
        for menu in crate::menu::menus() {
            collect_menu_shortcuts(menu.items, &mut add);
        }
        // Every keymap dispatches through global_shared_key identically
        // (T104g), so its bindings are shown regardless of the active
        // keymap, unlike the per-keymap match below.
        for b in vix_keybindings::SHARED {
            add(
                Self::action_title(b.action_id),
                modifier_token_display(b.key_token),
            );
        }
        match self.settings.keymap.as_str() {
            "spacemacs" => {
                for b in Self::spacemacs_leader_bindings() {
                    let keys = std::iter::once("SPC".to_string())
                        .chain(b.key_token.chars().map(|c| display_key(&c.to_string())))
                        .collect::<Vec<_>>()
                        .join(" ");
                    add(Self::action_title(b.action_id), keys);
                }
            }
            // Walks every context of every id below generically — Emacs is
            // the only one with more than one context (its chord tables,
            // T104a); the rest have only ever had one ("", T104c–g) — so a
            // later context/binding needs no matching change here. One
            // arm since T145 merged what used to be Emacs's own (needing
            // a chord-prefix string) with everyone else's: `ctx.name` is
            // always `""` for the single-context keymaps, so building a
            // prefix from it is a no-op for them, not special-cased away.
            id @ ("emacs" | "vscode-macos" | "vscode-windows" | "intellij-macos"
            | "intellij-windows" | "eclipse" | "sublime" | "apple") => {
                for table in vix_keybindings::TABLES.iter().filter(|t| t.keymap_id == id) {
                    for ctx in table.contexts {
                        let prefix: String = ctx
                            .name
                            .split(' ')
                            .filter(|s| !s.is_empty())
                            .map(modifier_token_display)
                            .collect::<Vec<_>>()
                            .join(" ");
                        for b in ctx.bindings {
                            let key_display = modifier_token_display(b.key_token);
                            let keys = if prefix.is_empty() {
                                key_display
                            } else {
                                format!("{prefix} {key_display}")
                            };
                            add(Self::action_title(b.action_id), keys);
                        }
                    }
                }
            }
            _ => {}
        }
        out
    }

    /// The translated title for an action id: its menu label when a menu
    /// item runs it, else its [`vix_action_catalog`] title (T147), else the
    /// raw id itself (matching what the which-key popup shows).
    fn action_title(action: &str) -> String {
        fn find(items: &[crate::menu::Item], action: &str) -> Option<String> {
            for it in items {
                if let Some(sub) = it.submenu {
                    if let Some(label) = find(sub, action) {
                        return Some(label);
                    }
                } else if it.action == action {
                    return Some(it.label());
                }
            }
            None
        }
        crate::menu::menus()
            .iter()
            .find_map(|m| find(m.items, action))
            .or_else(|| vix_action_catalog::title_key(action).map(|key| t!(key).to_string()))
            .unwrap_or_else(|| action.to_string())
    }

    /// Mouse handling for the keyboard-shortcut overlay: the wheel scrolls;
    /// a click on a column header sorts by it (again to flip the direction).
    fn help_mouse(&mut self, mouse: MouseEvent) {
        use crate::keyboard_shortcut_panel::Column;
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                if let Some(h) = self.help.as_mut() {
                    h.scroll_up(3);
                }
            }
            MouseEventKind::ScrollDown => {
                if let Some(h) = self.help.as_mut() {
                    h.scroll_down(3);
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let headers = self.layout.help_headers;
                let col = if rect_contains(headers[0], mouse.column, mouse.row) {
                    Some(Column::Action)
                } else if rect_contains(headers[1], mouse.column, mouse.row) {
                    Some(Column::Keys)
                } else {
                    None
                };
                if let (Some(col), Some(h)) = (col, self.help.as_mut()) {
                    h.toggle_sort(col);
                }
            }
            _ => {}
        }
    }

    /// Open the file browser (File → Open…) rooted at the workspace root.
    fn open_file_browser(&mut self) {
        self.file_browser = Some(crate::file_browser_panel::Panel::open(&self.root));
    }

    /// Keys for the file-browser overlay. Plain typing edits the filter;
    /// Ctrl S / Ctrl R / Alt H drive sort, direction, and hidden files;
    /// Ctrl O falls back to the classic path prompt (`file.open_path`).
    fn file_browser_key(&mut self, key: KeyEvent) {
        let page = (self.layout.file_browser.height as usize).max(1);
        match key.code {
            KeyCode::Up => {
                if let Some(fb) = self.file_browser.as_mut() {
                    fb.up();
                }
            }
            KeyCode::Down => {
                if let Some(fb) = self.file_browser.as_mut() {
                    fb.down();
                }
            }
            KeyCode::PageUp => {
                if let Some(fb) = self.file_browser.as_mut() {
                    fb.page_up(page);
                }
            }
            KeyCode::PageDown => {
                if let Some(fb) = self.file_browser.as_mut() {
                    fb.page_down(page);
                }
            }
            KeyCode::Home => {
                if let Some(fb) = self.file_browser.as_mut() {
                    fb.selected = 0;
                    fb.scroll = 0;
                }
            }
            KeyCode::End => {
                if let Some(fb) = self.file_browser.as_mut() {
                    fb.selected = fb.len().saturating_sub(1);
                }
            }
            // Left walks up to the parent; Right enters a highlighted directory.
            KeyCode::Left => {
                if let Some(fb) = self.file_browser.as_mut() {
                    fb.parent();
                }
            }
            KeyCode::Right => {
                if let Some(fb) = self.file_browser.as_mut()
                    && fb.selected_entry().is_some_and(|e| e.is_dir)
                {
                    fb.activate();
                }
            }
            KeyCode::Backspace => {
                if let Some(fb) = self.file_browser.as_mut() {
                    fb.backspace();
                }
            }
            KeyCode::Char('s') if Self::ctrl(&key) => {
                if let Some(fb) = self.file_browser.as_mut() {
                    fb.cycle_sort();
                }
            }
            KeyCode::Char('r') if Self::ctrl(&key) => {
                if let Some(fb) = self.file_browser.as_mut() {
                    fb.toggle_order();
                }
            }
            KeyCode::Char('h') if Self::alt(&key) => {
                if let Some(fb) = self.file_browser.as_mut() {
                    fb.toggle_hidden();
                }
            }
            KeyCode::Char('o') if Self::ctrl(&key) => {
                self.file_browser = None;
                self.run_action("file.open_path");
            }
            KeyCode::Char(c) if !Self::ctrl(&key) && !Self::alt(&key) => {
                if let Some(fb) = self.file_browser.as_mut() {
                    fb.push(c);
                }
            }
            KeyCode::Enter => self.file_browser_open_selected(),
            KeyCode::Esc => self.file_browser = None,
            _ => {}
        }
    }

    /// Act on the file browser's highlighted row: open a file in the editor
    /// (closing the browser), or re-root the browser into a directory.
    fn file_browser_open_selected(&mut self) {
        let Some(fb) = self.file_browser.as_mut() else {
            return;
        };
        // A ":line[:col]" query suffix jumps after opening, like the Open prompt.
        let target = fb.target();
        if let Some(path) = fb.activate() {
            self.file_browser = None;
            self.with_jump(|s| {
                s.open_path(&path, false);
                if let Some((line, col)) = target {
                    let area = s.editor_view();
                    s.editor.goto(line, Some(col), area);
                }
                s.focus = Focus::Editor;
            });
        }
    }

    fn file_browser_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                if let Some(fb) = self.file_browser.as_mut() {
                    fb.up();
                }
            }
            MouseEventKind::ScrollDown => {
                if let Some(fb) = self.file_browser.as_mut() {
                    fb.down();
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let r = self.layout.file_browser;
                if !rect_contains(r, mouse.column, mouse.row) {
                    return;
                }
                let mut hit = false;
                if let Some(fb) = self.file_browser.as_mut() {
                    let idx = fb.scroll + (mouse.row - r.y) as usize;
                    if idx < fb.len() {
                        fb.selected = idx;
                        hit = true;
                    }
                }
                // A click selects the row and opens it, like the recent chooser.
                if hit {
                    self.file_browser_open_selected();
                }
            }
            _ => {}
        }
    }

    fn open_recent_chooser(&mut self) {
        let entries: Vec<RecentEntry> = self
            .settings
            .recent_files
            .iter()
            .map(PathBuf::from)
            .filter(|p| p.is_file())
            .map(|path| {
                use crate::file_browser_panel::unix_secs;
                let meta = std::fs::metadata(&path).ok();
                RecentEntry {
                    size: meta.as_ref().map_or(0, std::fs::Metadata::len),
                    created: meta.as_ref().and_then(|m| unix_secs(m.created().ok())),
                    modified: meta.as_ref().and_then(|m| unix_secs(m.modified().ok())),
                    path,
                }
            })
            .collect();
        if entries.is_empty() {
            self.status = t!("status.no_recent").to_string();
            return;
        }
        self.recent_chooser = Some(RecentChooser {
            entries,
            selected: 0,
        });
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
            && let Some(path) = rc.entries.get(rc.selected).map(|e| e.path.clone())
        {
            self.with_jump(|s| {
                s.open_path(&path, false);
                s.focus = Focus::Editor;
            });
        }
    }

    fn recent_mouse(&mut self, mouse: MouseEvent) {
        if let Some(idx) = self.chooser_row(mouse)
            && let Some(rc) = self.recent_chooser.as_mut()
            && idx < rc.entries.len()
        {
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
        self.location_chooser = Some(LocationChooser {
            entries,
            selected: 0,
        });
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
            && let Some(loc) = lc.entries.get(lc.selected).cloned()
        {
            self.navigate_to(&loc);
        }
    }

    fn location_mouse(&mut self, mouse: MouseEvent) {
        if let Some(idx) = self.chooser_row(mouse)
            && let Some(lc) = self.location_chooser.as_mut()
            && idx < lc.entries.len()
        {
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

    // ----- ASCII panel ----------------------------------------------------

    /// Open the table editor on the active buffer, parsed as CSV or TSV (per the
    /// file extension; CSV by default). Warns when there is no editable buffer.
    fn open_edit_table(&mut self) {
        let Some(tab) = self.editor.active_tab() else {
            self.messages
                .warn(t!("msg.edit_table_no_buffer").to_string());
            return;
        };
        if tab.is_image() {
            self.messages
                .warn(t!("msg.edit_table_no_buffer").to_string());
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
        let page = usize::from(self.layout.edit_table.height)
            .saturating_sub(1)
            .max(1);
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
        let Some(text) = self
            .edit_table
            .as_ref()
            .map(crate::edit_table::Grid::to_text)
        else {
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
            self.messages
                .warn(t!("msg.edit_outline_no_buffer").to_string());
            return;
        };
        if tab.is_image() {
            self.messages
                .warn(t!("msg.edit_outline_no_buffer").to_string());
            return;
        }
        let text = tab.text();
        self.edit_outline = Some(crate::edit_outline::Tree::from_text(&text));
    }

    /// Route a key to the open outline editor and act on its outcome.
    fn edit_outline_key(&mut self, key: KeyEvent) {
        let page = usize::from(self.layout.edit_outline.height)
            .saturating_sub(1)
            .max(1);
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
        let Some(text) = self
            .edit_outline
            .as_ref()
            .map(crate::edit_outline::Tree::to_text)
        else {
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
        let page = usize::from(self.layout.edit_sql.height)
            .saturating_sub(1)
            .max(1);
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

    /// Dispatch a `db.*` action (the DB menu / palette). Returns `true` if
    /// handled. Actions that need an open workbench open the overlay first.
    fn db_action(&mut self, action: &str) -> bool {
        match action {
            "db.connections" => {
                self.open_db();
                if let Some(b) = self.db.as_mut() {
                    b.view = crate::db::View::Connections;
                }
            }
            "db.query" => {
                self.open_db();
                if let Some(b) = self.db.as_mut()
                    && b.conn.is_some()
                {
                    b.view = crate::db::View::Workbench;
                }
            }
            "db.execute" => self.with_db(crate::db::Browser::execute),
            "db.execute_all" => self.with_db(crate::db::Browser::execute_all),
            "db.explain" => self.with_db(|b| b.explain(false)),
            "db.explain_analyze" => self.with_db(|b| b.explain(true)),
            "db.format" => self.with_db(crate::db::Browser::format_at_cursor),
            "db.history" => self.with_db(crate::db::Browser::open_history),
            "db.saved" => self.with_db(crate::db::Browser::open_saved),
            "db.save_query" => self.with_db(crate::db::Browser::open_save_name),
            "db.export" => self.with_db(crate::db::Browser::open_export),
            "db.begin" => self.with_db(crate::db::Browser::begin_tx),
            "db.commit" => self.with_db(crate::db::Browser::commit_tx),
            "db.rollback" => self.with_db(crate::db::Browser::rollback_tx),
            "db.refresh" => self.with_db(crate::db::Browser::refresh_catalog),
            "db.disconnect" => self.with_db(crate::db::Browser::disconnect),
            _ => return false,
        }
        true
    }

    /// Open the DB overlay (if it is not already), keeping any live session.
    /// A fresh overlay also loads the persisted query history and saved
    /// queries.
    fn open_db(&mut self) {
        if self.db.is_none() {
            let mut browser = crate::db::Browser::new(self.settings.db_connections.clone());
            browser.history = crate::db::store::load_history();
            browser.saved = crate::db::store::load_saved();
            self.db = Some(browser);
        }
    }

    /// Apply `f` to the open DB workbench, opening the overlay when needed.
    fn with_db(&mut self, f: impl FnOnce(&mut crate::db::Browser)) {
        self.open_db();
        if let Some(b) = self.db.as_mut() {
            f(b);
        }
    }

    /// Route a key to the open DB workbench, persist any connection-list
    /// change, and act on the outcome.
    fn db_key(&mut self, key: KeyEvent) {
        let pages = crate::db::Pages {
            tree: usize::from(self.layout.db_tree.height).max(1),
            editor: usize::from(self.layout.db_editor.height).max(1),
            results: usize::from(self.layout.db_results.height).max(1),
        };
        let outcome = match self.db.as_mut() {
            Some(b) => b.handle_key(key, pages),
            None => return,
        };
        if let Some(conns) = self
            .db
            .as_mut()
            .and_then(crate::db::Browser::take_dirty_connections)
        {
            self.settings.db_connections = conns;
            let _ = self.store_settings();
        }
        if let Some(history) = self
            .db
            .as_mut()
            .and_then(crate::db::Browser::take_dirty_history)
        {
            let _ = crate::db::store::save_history(&history);
        }
        if let Some(saved) = self
            .db
            .as_mut()
            .and_then(crate::db::Browser::take_dirty_saved)
        {
            let _ = crate::db::store::save_saved(&saved);
        }
        // A queued natural-language → SQL request: spawn the assistant CLI and
        // route its reply back into the workbench editor.
        if let Some(req) = self
            .db
            .as_mut()
            .and_then(crate::db::Browser::take_ai_request)
        {
            if self.ai_replace.is_some() {
                if let Some(b) = self.db.as_mut() {
                    b.ai_failed();
                }
            } else if let Some(rx) = self.spawn_ai_cmd(&req.prompt, &req.context) {
                let label = t!("menu.ai").to_string();
                self.ai_replace = Some(AiReplace {
                    rx,
                    dest: AiDest::Db,
                    label,
                });
            } else if let Some(b) = self.db.as_mut() {
                b.ai_failed();
            }
        }
        if outcome == crate::db::Outcome::Close {
            self.db = None;
        }
    }

    /// Open the structured-value editor (JSON or YAML) on the active buffer.
    /// Warns when there is no buffer or it does not parse in `format`.
    fn open_edit_value(&mut self, format: crate::edit_value::Format) {
        let Some(tab) = self.editor.active_tab() else {
            self.messages
                .warn(t!("msg.edit_value_no_buffer").to_string());
            return;
        };
        if tab.is_image() {
            self.messages
                .warn(t!("msg.edit_value_no_buffer").to_string());
            return;
        }
        match crate::edit_value::Tree::from_text(&tab.text(), format) {
            Some(tree) => self.edit_value = Some(tree),
            None => self.messages.warn(t!("msg.edit_value_parse").to_string()),
        }
    }

    /// Route a key to the open structured-value editor and act on its outcome.
    fn edit_value_key(&mut self, key: KeyEvent) {
        let page = usize::from(self.layout.edit_value.height)
            .saturating_sub(1)
            .max(1);
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
        let Some(text) = self
            .edit_value
            .as_ref()
            .map(crate::edit_value::Tree::to_text)
        else {
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
            self.messages
                .warn(t!("msg.edit_bytes_no_buffer").to_string());
            return;
        };
        if tab.is_image() {
            self.messages
                .warn(t!("msg.edit_bytes_no_buffer").to_string());
            return;
        }
        self.edit_bytes = Some(crate::edit_bytes::Hex::from_bytes(tab.text().into_bytes()));
    }

    /// Route a key to the open byte editor and act on its outcome.
    fn edit_bytes_key(&mut self, key: KeyEvent) {
        let page = usize::from(self.layout.edit_bytes.height)
            .saturating_sub(1)
            .max(1);
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

    // ----- X11 color palette ----------------------------------------------

    // ----- Media-type (MIME) picker ---------------------------------------

    // ----- HTML character palette -----------------------------------------

    pub(super) fn open_html_panel(&mut self) {
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
            p.select_index(idx)
                .then(|| html_cell_at(p.selected_entity(), rel_col))
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

    /// License/about information sourced from the crate metadata (`Cargo.toml`,
    /// via the `CARGO_PKG_*` build-time environment variables).
    fn license_lines() -> Vec<String> {
        let authors = env!("CARGO_PKG_AUTHORS").replace(':', ", ");
        vec![
            format!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
            String::new(),
            env!("CARGO_PKG_DESCRIPTION").to_string(),
            String::new(),
            format!(
                "{}: {}",
                t!("help.license.field_license"),
                env!("CARGO_PKG_LICENSE")
            ),
            format!(
                "{}: {}",
                t!("help.license.field_repository"),
                env!("CARGO_PKG_REPOSITORY")
            ),
            format!("{}: {}", t!("help.license.field_authors"), authors),
            String::new(),
            t!("help.license.trademarks").to_string(),
        ]
    }

    /// "Report an issue" help text plus the issue-tracker URL.
    fn report_issue_lines() -> Vec<String> {
        let mut lines: Vec<String> = t!("help.report_issue.body")
            .lines()
            .map(str::to_string)
            .collect();
        lines.push(String::new());
        lines.push(format!("{}/issues", env!("CARGO_PKG_REPOSITORY")));
        lines
    }

    /// The privacy statement from the i18n catalog, split into lines.
    fn privacy_lines() -> Vec<String> {
        t!("help.privacy.body")
            .lines()
            .map(str::to_string)
            .collect()
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
        let selection = self
            .editor
            .active_tab_mut()
            .and_then(|t| t.editor.get_selection_text());
        match selection {
            Some(s) if !s.trim().is_empty() => s,
            _ => self
                .editor
                .active_tab()
                .map(|t| t.editor.get_content())
                .unwrap_or_default(),
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
            && !sel.trim().is_empty()
        {
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
            let Some(tab) = self.editor.active_tab_mut() else {
                return;
            };
            if tab.is_image() {
                self.status = t!("status.ai_no_input").to_string();
                return;
            }
            match tab.editor.get_selection() {
                Some(sel) if !sel.is_empty() => (
                    tab.editor.get_content_slice(sel.start, sel.end),
                    AiTarget::Range(sel.start, sel.end),
                ),
                _ => (tab.editor.get_content(), AiTarget::Whole),
            }
        };
        let dest = if self.settings.ai_diff_review {
            AiDest::Diff {
                tab: tab_idx,
                target,
            }
        } else {
            AiDest::Replace {
                tab: tab_idx,
                target,
            }
        };
        if let Some(rx) = self.spawn_ai(prompt, &text) {
            self.ai_replace = Some(AiReplace {
                rx,
                dest,
                label: label.to_string(),
            });
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
            self.ai_replace = Some(AiReplace {
                rx,
                dest: AiDest::NewTab,
                label: label.to_string(),
            });
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
    fn spawn_ai_cmd(
        &mut self,
        prompt: &str,
        text: &str,
    ) -> Option<std::sync::mpsc::Receiver<AiMsg>> {
        // A private, unpredictable temp file (0600, O_EXCL) so a local attacker
        // can't pre-plant a symlink at the name and redirect the write, nor read
        // the buffer contents left behind in a shared /tmp.
        let Ok(tmp) = crate::fileops::write_private_temp("vix-ai", text.as_bytes()) else {
            self.status = t!("status.ai_no_input").to_string();
            return None;
        };
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
                self.messages
                    .error(t!("msg.command_failed", error = e).to_string());
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
            let ok = std::io::BufReader::new(stdout)
                .read_to_string(&mut out)
                .is_ok();
            let status = reader_child
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .wait()
                .ok();
            // The input file is no longer needed once the CLI has exited.
            let _ = std::fs::remove_file(&tmp);
            let success = ok && status.and_then(|s| s.code()) == Some(0);
            let _ = tx.send(if success {
                AiMsg::Done(out)
            } else {
                AiMsg::Failed
            });
        });
        Some(rx)
    }

    // ----- HTTP client ----------------------------------------------------

    /// Send the HTTP request described by the active buffer (a `.http`-style
    /// document) on a background thread; the response opens in a new tab.
    fn http_send(&mut self) {
        let Some(text) = self.editor.active_tab().map(crate::editor::Tab::text) else {
            return;
        };
        let Some(req) = crate::http_client::parse_request(&text) else {
            self.status = t!("status.http_no_request").to_string();
            return;
        };
        self.status = t!("status.http_sending", url = req.url.clone()).to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(crate::http_client::send(&req));
        });
        self.http_rx = Some(rx);
    }

    /// Whether an HTTP request is in flight (keeps the event loop polling fast).
    #[must_use]
    pub fn http_running(&self) -> bool {
        self.http_rx.is_some()
    }

    /// Drain a finished HTTP response into a new tab. Called each event-loop
    /// iteration; cheap when none is running.
    pub fn poll_http(&mut self) {
        let result = {
            let Some(rx) = self.http_rx.as_ref() else {
                return;
            };
            match rx.try_recv() {
                Ok(r) => r,
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Err(t!("status.http_failed").to_string())
                }
            }
        };
        self.http_rx = None;
        match result {
            Ok(body) => {
                self.editor.new_tab_with_content(&body);
                self.focus = Focus::Editor;
                self.status = t!("status.http_done").to_string();
            }
            Err(e) => self
                .messages
                .error(t!("msg.http_error", error = e).to_string()),
        }
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
                        panel.push(
                            crate::ai_panel::Role::Error,
                            t!("status.ai_failed", action = &ar.label),
                        );
                    }
                    if let (AiDest::Db, Some(b)) = (ar.dest, self.db.as_mut()) {
                        b.ai_failed();
                    }
                    self.status = t!("status.ai_failed", action = &ar.label).to_string();
                    if !matches!(ar.dest, AiDest::Panel | AiDest::Db) {
                        self.messages
                            .error(t!("status.ai_failed", action = ar.label));
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
                    AiDest::Db => {
                        if let Some(b) = self.db.as_mut() {
                            b.apply_ai_reply(text);
                        }
                    }
                }
                self.status = t!("status.ai_done", action = &ar.label).to_string();
                if !matches!(ar.dest, AiDest::Panel | AiDest::Db) {
                    self.messages.info(t!("status.ai_done", action = ar.label));
                }
            }
            AiMsg::Failed => {
                if let (AiDest::Panel, Some(panel)) = (ar.dest, self.ai_panel.as_mut()) {
                    panel.busy = false;
                    panel.push(
                        crate::ai_panel::Role::Error,
                        t!("status.ai_failed", action = &ar.label),
                    );
                }
                if let (AiDest::Db, Some(b)) = (ar.dest, self.db.as_mut()) {
                    b.ai_failed();
                }
                self.status = t!("status.ai_failed", action = &ar.label).to_string();
                if !matches!(ar.dest, AiDest::Panel | AiDest::Db) {
                    self.messages
                        .error(t!("status.ai_failed", action = ar.label));
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

    /// Drain a finished asynchronous DB query into the workbench. Called each
    /// event-loop tick; cheap when nothing is running.
    pub fn poll_db_query(&mut self) {
        if let Some(b) = self.db.as_mut() {
            b.poll_query();
        }
    }

    /// Whether a DB query is running asynchronously (keeps the loop polling).
    #[must_use]
    pub fn db_query_running(&self) -> bool {
        self.db
            .as_ref()
            .is_some_and(crate::db::Browser::query_running)
    }

    // ----- AI chat panel --------------------------------------------------

    /// Open the AI chat panel (a persistent conversation with the configured
    /// assistant), or focus it if already open. Seeds the input with the current
    /// selection so "ask about this" is one keystroke away.
    fn open_ai_panel(&mut self) {
        if self.ai_panel.is_none() {
            let mut panel = crate::ai_panel::Panel::open();
            if let Some(sel) = self
                .editor
                .active_tab_mut()
                .and_then(|t| t.editor.get_selection_text())
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
        let Some(panel) = self.ai_panel.as_mut() else {
            return;
        };
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
            self.ai_replace = Some(AiReplace {
                rx,
                dest: AiDest::Panel,
                label,
            });
        } else if let Some(panel) = self.ai_panel.as_mut() {
            panel.busy = false;
            panel.push(
                crate::ai_panel::Role::Error,
                t!("msg.command_failed", error = "spawn").to_string(),
            );
        }
    }

    /// Open the panel's most recent assistant reply in a new editor tab.
    fn ai_panel_last_to_tab(&mut self) {
        if let Some(text) = self
            .ai_panel
            .as_ref()
            .and_then(|p| p.last_assistant())
            .map(str::to_string)
        {
            self.editor.new_tab_with_content(&text);
            self.ai_panel = None;
            self.focus = Focus::Editor;
        }
    }

    /// Copy the panel's most recent assistant reply to the system clipboard.
    fn ai_panel_copy_last(&mut self) {
        let Some(text) = self
            .ai_panel
            .as_ref()
            .and_then(|p| p.last_assistant())
            .map(str::to_string)
        else {
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
            if cfg!(windows) {
                "cmd.exe".to_string()
            } else {
                "/bin/sh".to_string()
            }
        });
        match crate::terminal::Terminal::open(&shell, &self.root, rows, cols) {
            Ok(term) => self.terminal = Some(term),
            Err(e) => self
                .messages
                .error(t!("msg.command_failed", error = e).to_string()),
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
        let Some(term) = self.terminal.as_mut() else {
            return;
        };
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
        let Some(tab) = self.editor.tabs.get(tab_idx) else {
            return;
        };
        let old = match target {
            AiTarget::Whole => tab.editor.get_content(),
            AiTarget::Range(start, end) => tab.editor.get_content_slice(start, end),
        };
        match crate::ai_diff::Review::from_texts(&old, new_text) {
            Some(review) => {
                self.ai_diff = Some(AiDiffState {
                    review,
                    tab: tab_idx,
                    target,
                });
            }
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
        let Some(state) = self.ai_diff.take() else {
            return;
        };
        let text = state.review.result();
        self.apply_ai_replace(state.tab, state.target, &text);
        self.status = t!("status.ai_done", action = t!("menu.ai")).to_string();
    }

    // ----- Contacts (vCard browser) ---------------------------------------

    /// Open the contact browser over the configured vCard directory (or the
    /// workspace root), parsing each `.vcf`'s display name.
    pub(super) fn open_contacts(&mut self) {
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
                        path.file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default()
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

    // ----- File Information panel -----------------------------------------

    // ----- Text Information panel -----------------------------------------

    // ----- Markdown preview -----------------------------------------------

    // ----- Snippets -------------------------------------------------------

    // ----- System Information panel ---------------------------------------

    // ----- workspace dashboard ----------------------------------------------

    /// Open the Workspace Dashboard and kick off the background metric computations
    /// (disk usage via `du`, a recursive file count, and the git commit count).
    pub(super) fn open_dashboard(&mut self) {
        // Idempotent while already open: re-invoking the action must not spawn
        // another batch of metric threads (and another `du` scan) on top of the
        // in-flight one. Reopening after a close (`dashboard_rx` cleared) does
        // recompute.
        if self.dashboard.is_some() && self.dashboard_rx.is_some() {
            return;
        }
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
            if let Ok(out) = std::process::Command::new("du")
                .arg("-sh")
                .arg(&droot)
                .output()
                && out.status.success()
            {
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
            let _ = tx.send(DashMsg::Commits(
                crate::git::commit_count(&root).unwrap_or(0),
            ));
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
                    .map(|s| crate::outline_panel::Entry {
                        kind: s.kind,
                        name: s.name,
                        line: s.line,
                    })
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
        let Some(line) = self
            .outline
            .as_ref()
            .and_then(crate::outline_panel::Outline::selected_line)
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
        let key = self
            .editor
            .active_tab()
            .filter(|t| !t.is_image())
            .map(|t| (self.editor.active, t.editor.revision()));
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
                        .map(|s| crate::outline_panel::Entry {
                            kind: s.kind,
                            name: s.name,
                            line: s.line,
                        })
                        .collect()
                })
                .unwrap_or_default();
            self.outline_dock = if entries.is_empty() {
                None
            } else {
                Some(Outline::new(entries))
            };
        }
        if let Some(o) = self.outline_dock.as_mut() {
            let cur = self
                .editor
                .active_tab()
                .map_or(1, |t| t.editor.cursor_line() + 1);
            o.select_nearest(cur);
        }
    }

    /// Jump the editor to the outline-sidebar row at viewport index `row`.
    fn outline_dock_click(&mut self, row: usize) {
        let Some(o) = self.outline_dock.as_mut() else {
            return;
        };
        let idx = o.scroll + row;
        if idx >= o.entries.len() {
            return;
        }
        o.selected = idx;
        let Some(line) = o.selected_line() else {
            return;
        };
        self.with_jump(|s| {
            let area = s.editor_view();
            s.editor.goto(line, None, area);
            s.focus = Focus::Editor;
        });
    }

    // ----- command palette ------------------------------------------------

    // ----- search / replace ----------------------------------------------

    fn start_search(&mut self, replacing: bool) {
        self.search = Some(SearchBar::new(replacing));
    }

    /// Move the find box to its next scope (buffer → files → workspace → buffer)
    /// and act on it.
    ///
    /// This is what replaced the separate **Find in Files…** and **Find in
    /// Workspace…** menu items: one dialog, with the place to look as an option
    /// inside it. Widening carries the query, the replacement, and the
    /// case/regex toggles to the surface that can show that many results — the
    /// workspace panel for **Files**, the bottom dock for **Workspace** — so
    /// nothing is retyped. Cycling back to **Buffer** returns to the find box
    /// with the query it had.
    fn widen_search_scope(&mut self) {
        let Some(bar) = self.search.as_mut() else {
            return;
        };
        let scope = bar.cycle_scope();
        let (query, replace, replacing, case_sensitive, regex) = (
            bar.query.clone(),
            bar.replace.clone(),
            bar.flags.contains(SearchFlags::REPLACING),
            bar.flags.contains(SearchFlags::CASE_SENSITIVE),
            bar.flags.contains(SearchFlags::REGEX),
        );
        match scope {
            Scope::Buffer => {}
            Scope::Files => {
                self.search = None;
                self.build_file_index();
                let mut panel = WorkspaceSearch::new(replacing);
                panel.query = query;
                panel.replace = replace;
                panel
                    .flags
                    .set(WorkspaceFlags::CASE_SENSITIVE, case_sensitive);
                panel.flags.set(WorkspaceFlags::REGEX, regex);
                self.workspace_search = Some(panel);
                self.run_workspace_search();
            }
            Scope::Workspace => {
                self.search = None;
                if query.is_empty() {
                    // Nothing to list yet: ask for the pattern, as the dock
                    // search does when invoked on its own.
                    self.prompt = Some(Prompt::new(
                        PromptKind::SearchToDock,
                        t!("prompt.search_dock").to_string(),
                    ));
                } else {
                    self.search_workspace_to_dock(&query, case_sensitive, regex);
                }
            }
        }
    }

    /// Whether typing in the search box should live-preview the next match.
    /// Interactive (query-replace) mode keeps the cursor put until it begins.
    fn search_should_preview(&self) -> bool {
        self.search
            .as_ref()
            .is_some_and(|s| !s.flags.contains(SearchFlags::INTERACTIVE) && s.field == Field::Query)
    }

    /// Recompute and apply search-highlight marks for the active buffer, then
    /// move the cursor to the next/previous match.
    fn find_step(&mut self, forward: bool) {
        // While the find box is open, use its (possibly empty) query; once closed,
        // repeat the last completed search so Find Next / Previous keep working.
        let pat = if self.search.is_some() {
            self.search
                .as_ref()
                .and_then(super::search::SearchBar::pattern)
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
            matches.iter().position(|(s, _)| *s > cur).unwrap_or(0) // past the last match: wrap to the first
        } else {
            matches
                .iter()
                .rposition(|(s, _)| *s < cur)
                .unwrap_or(matches.len() - 1) // before the first: wrap to the last
        };
        let target = matches[target_idx];
        t.editor.set_cursor(target.0);
        t.editor
            .set_selection(Some(Selection::new(target.0, target.1)));
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
        let has = self
            .editor
            .active_tab()
            .is_some_and(|t| t.editor.has_marks());
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
                let interactive = self
                    .search
                    .as_ref()
                    .is_some_and(|s| s.flags.contains(SearchFlags::INTERACTIVE));
                let replacing = self
                    .search
                    .as_ref()
                    .is_some_and(|s| s.flags.contains(SearchFlags::REPLACING));
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
            // `Alt+I` widens the search instead of the user closing the box and
            // finding another menu item; it takes the query, the replacement,
            // and the toggles with it.
            KeyCode::Char('i' | 'I') if Self::alt(&key) => self.widen_search_scope(),
            KeyCode::Char(c) if Self::alt(&key) => {
                if let Some(s) = self.search.as_mut() {
                    match c.to_ascii_lowercase() {
                        'c' => s.flags.toggle(SearchFlags::CASE_SENSITIVE),
                        's' => s.flags.toggle(SearchFlags::SMART_CASE),
                        'w' => s.flags.toggle(SearchFlags::WHOLE_WORD),
                        'r' => s.flags.toggle(SearchFlags::REGEX),
                        // Replace is a mode of this dialog, not a separate one.
                        // `h` as in the `Ctrl+H` every other editor uses; `Alt+P`
                        // is taken by `search.prev_selection`.
                        'h' => s.toggle_replace(),
                        _ => {}
                    }
                }
                // Toggles never move the cursor while in interactive mode.
                if self
                    .search
                    .as_ref()
                    .is_some_and(|s| !s.flags.contains(SearchFlags::INTERACTIVE))
                {
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

        // The Replace and scope options: the same two moves as `Alt+P` / `Alt+I`.
        if hit(self.layout.search_replace_toggle) {
            if let Some(s) = self.search.as_mut() {
                s.toggle_replace();
            }
            return;
        }
        if hit(self.layout.search_scope) {
            self.widen_search_scope();
            return;
        }
        // Toggle buttons.
        if hit(self.layout.search_case)
            || hit(self.layout.search_smartcase)
            || hit(self.layout.search_word)
            || hit(self.layout.search_regex)
        {
            if let Some(s) = self.search.as_mut() {
                if hit(self.layout.search_case) {
                    s.flags.toggle(SearchFlags::CASE_SENSITIVE);
                } else if hit(self.layout.search_smartcase) {
                    s.flags.toggle(SearchFlags::SMART_CASE);
                } else if hit(self.layout.search_word) {
                    s.flags.toggle(SearchFlags::WHOLE_WORD);
                } else {
                    s.flags.toggle(SearchFlags::REGEX);
                }
            }
            if self
                .search
                .as_ref()
                .is_some_and(|s| !s.flags.contains(SearchFlags::INTERACTIVE))
            {
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
            } else if s.flags.contains(SearchFlags::REPLACING) && rel == 2 {
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
        let regex = sb.flags.contains(SearchFlags::REGEX);
        let template = if regex {
            crate::find_panel::unescape(&sb.replace)
        } else {
            sb.replace.clone()
        };
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
        let use_regex = sb.flags.contains(SearchFlags::REGEX);
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

    /// Org → Contacts dispatcher. Returns `true` if it handled `action`.
    fn contacts_action(&mut self, action: &str) -> bool {
        if let Some(key) = action.strip_prefix("org.contacts.field.") {
            self.insert_content(&crate::org_contacts::field_line(key));
            return true;
        }
        match action {
            "org.capture.contact" => self.start_capture_by_key("c"),
            "org.contacts.find" => self.contacts_buffer(crate::org_contacts::directory),
            "org.contacts.birthdays" => self.contacts_buffer(crate::org_contacts::birthdays),
            "org.contacts.vcard" => self.contacts_buffer(crate::org_contacts::to_vcard),
            "org.contacts.link_complete" => {
                if !self.maybe_complete_org_contact_link() {
                    self.status = t!("status.org_contacts_link_no_context").to_string();
                }
            }
            _ => return false,
        }
        true
    }

    /// Compile a cross-file contacts view with `f` (over every project `.org`
    /// file) into a new buffer.
    fn contacts_buffer(&mut self, f: fn(&[(String, String)]) -> String) {
        let files = self.roam_node_files();
        let buffer = f(&files);
        self.editor.new_tab_with_content(&buffer);
    }

    /// Go-menu navigation dispatcher. Returns `true` if it handled `action`.
    fn go_action(&mut self, action: &str) -> bool {
        let area = self.editor_view();
        match action {
            "nav.goto_declaration" => self.goto_declaration(),
            "nav.next_issue" => self.goto_diagnostic(true),
            "nav.prev_issue" => self.goto_diagnostic(false),
            "nav.line_next" => self.editor.cursor_line_down(area),
            "nav.line_prev" => self.editor.cursor_line_up(area),
            "nav.goto_paragraph" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::GotoParagraph,
                    t!("prompt.goto_paragraph").to_string(),
                ));
            }
            "nav.para_next" => self.editor.cursor_paragraph_next(area),
            "nav.para_prev" => self.editor.cursor_paragraph_prev(area),
            "nav.goto_section" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::GotoSection,
                    t!("prompt.goto_section").to_string(),
                ));
            }
            "nav.section_next" => self.editor.cursor_section_next(area),
            "nav.section_prev" => self.editor.cursor_section_prev(area),
            "nav.sentence_start" => self.editor.cursor_sentence_start(area),
            "nav.sentence_end" => self.editor.cursor_sentence_end(area),
            "nav.sentence_next" => self.editor.cursor_sentence_next(area),
            "nav.sentence_prev" => self.editor.cursor_sentence_prev(area),
            "nav.goto_sentence" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::GotoSentence,
                    t!("prompt.goto_sentence").to_string(),
                ));
            }
            "nav.word_start" => self.editor.cursor_word_start(area),
            "nav.word_end" => self.editor.cursor_word_end(area),
            "nav.word_next" => self.editor.cursor_word_next(area),
            "nav.word_prev" => self.editor.cursor_word_prev(area),
            "nav.goto_word" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::GotoWord,
                    t!("prompt.goto_word").to_string(),
                ));
            }
            "nav.matching_tag" => self.goto_matching_tag(),
            "nav.goto_percent" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::GotoPercent,
                    t!("prompt.goto_percent").to_string(),
                ));
            }
            "nav.goto_byte" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::GotoByte,
                    t!("prompt.goto_byte").to_string(),
                ));
            }
            _ => return false,
        }
        true
    }

    /// Jump to the Nth word/sentence/paragraph/section from a submitted number.
    fn accept_goto_number(&mut self, kind: PromptKind, input: &str) {
        let Ok(n) = input.trim_end_matches('%').parse::<usize>() else {
            return;
        };
        let area = self.editor_view();
        match kind {
            PromptKind::GotoParagraph => self.editor.goto_paragraph(n, area),
            PromptKind::GotoSentence => self.editor.goto_sentence(n, area),
            PromptKind::GotoWord => self.editor.goto_word(n, area),
            PromptKind::GotoPercent => self.editor.goto_percent(n, area),
            PromptKind::GotoByte => self.editor.goto_byte(n, area),
            _ => self.editor.goto_section(n, area),
        }
    }

    /// Show the call hierarchy (incoming calls / callers) of the symbol under the
    /// cursor (LSP); results land in the references jump list.
    fn call_hierarchy(&mut self) {
        if let Some(path) = self.active_path()
            && self.lsp.handles(&path)
        {
            let (line, character) = self.cursor_lsp_position(&path);
            self.lsp
                .request_prepare_call_hierarchy(&path, line, character);
        } else {
            self.status = t!("status.lsp_inactive").to_string();
        }
    }

    /// Go to the declaration of the symbol under the cursor (LSP).
    fn goto_declaration(&mut self) {
        if let Some(path) = self.active_path()
            && self.lsp.handles(&path)
        {
            let (line, character) = self.cursor_lsp_position(&path);
            self.lsp.request_declaration(&path, line, character);
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
            Prompt::new(PromptKind::LspRename, t!("prompt.lsp_rename").to_string())
                .with_input(seed),
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

    /// Replace every captured linked-editing range with `text` (highest offset
    /// first so earlier offsets stay valid).
    fn apply_linked_edit(&mut self, text: &str) {
        let Some(mut ranges) = self.linked_ranges.take() else {
            return;
        };
        ranges.sort_by_key(|&(s, _)| std::cmp::Reverse(s));
        let Some(t) = self.editor.active_tab_mut() else {
            return;
        };
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

    /// Show document symbols (all in the active file) in the static-results panel.
    fn show_document_symbols(&mut self, syms: &[(u32, u32, String)]) {
        let Some(path) = self.active_path() else {
            return;
        };
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
        ps.flags.insert(WorkspaceFlags::STATIC_RESULTS);
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
        ps.flags.insert(WorkspaceFlags::STATIC_RESULTS);
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
        ps.flags.insert(WorkspaceFlags::STATIC_RESULTS);
        ps.status = t!("status.references_n", n = hits.len()).to_string();
        ps.hits = hits;
        self.workspace_search = Some(ps);
    }

    fn goto_definition(&mut self) {
        if let Some(path) = self.active_path()
            && self.lsp.handles(&path)
        {
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
                ps.flags.insert(WorkspaceFlags::STATIC_RESULTS);
                ps.hits = hits;
                ps.status = t!("status.definitions_n", n = n, symbol = symbol).to_string();
                self.workspace_search = Some(ps);
            }
        }
    }

    /// Scan the project's files for comment tags (TODO / FIXME / HACK / XXX /
    /// BUG / NOTE) and list them in the static-results search overlay; Enter jumps
    /// to the match. Uses the file index (which honors `.gitignore`).
    fn open_todo_finder(&mut self) {
        static TAGS: &[&str] = &["TODO", "FIXME", "HACK", "XXX", "BUG", "NOTE"];
        let mut hits: Vec<Hit> = Vec::new();
        'files: for path in &self.file_index {
            let Ok(content) = std::fs::read_to_string(path) else {
                continue;
            };
            let rel = path
                .strip_prefix(&self.root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned();
            for (i, raw) in content.lines().enumerate() {
                // Match a tag as a word (a following ':' or space/paren is typical).
                if let Some(col) = TAGS
                    .iter()
                    .find_map(|tag| crate::textops::tag_column(raw, tag))
                {
                    let line = i + 1;
                    let text: String = raw.trim().chars().take(120).collect();
                    hits.push(Hit {
                        path: path.clone(),
                        line,
                        col: col + 1,
                        display: format!("{rel}:{line}: {text}"),
                    });
                    if hits.len() >= 2000 {
                        break 'files;
                    }
                }
            }
        }
        if hits.is_empty() {
            self.status = t!("status.no_todos").to_string();
            return;
        }
        hits.sort_by(|a, b| a.display.cmp(&b.display));
        let mut ps = WorkspaceSearch::new(false);
        ps.flags.insert(WorkspaceFlags::STATIC_RESULTS);
        ps.status = t!("status.todos_n", n = hits.len()).to_string();
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
        let ok = sym
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_');
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
        if self
            .workspace_search
            .as_ref()
            .is_some_and(|p| p.flags.contains(WorkspaceFlags::STATIC_RESULTS))
        {
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
        if !ps.flags.contains(WorkspaceFlags::REPLACING) {
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
        let use_regex = ps.flags.contains(WorkspaceFlags::REGEX);
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
            let (new, count) =
                crate::find_panel::replace_all(&content, &re, use_regex, &replacement);
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
        self.replace_confirm = Some(ReplaceConfirm {
            plan,
            replaced,
            lines,
            scroll: 0,
        });
    }

    /// Apply a confirmed project-wide replace: write every planned file and keep
    /// open buffers in sync, then refresh the search and report the totals.
    fn apply_replace_confirm(&mut self) {
        let Some(rc) = self.replace_confirm.take() else {
            return;
        };
        let replaced = rc.replaced;
        let mut files = 0usize;
        for (path, new) in &rc.plan {
            if let Err(e) = std::fs::write(path, new) {
                self.messages
                    .error(t!("msg.write_failed", path = path.display(), error = e).to_string());
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
        let note = t!(
            "status.replaced_in_files",
            replaced = replaced,
            files = files
        )
        .to_string();
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
                let replacing = self
                    .workspace_search
                    .as_ref()
                    .is_some_and(|p| p.flags.contains(WorkspaceFlags::REPLACING));
                let on_replace =
                    self.workspace_search.as_ref().map(|p| p.field) == Some(Field::Replace);
                if replacing && (Self::alt(&key) || on_replace) {
                    self.workspace_replace_all();
                } else {
                    self.open_selected_hit();
                }
            }
            // The scope option continues here: the panel is the "Files" stage,
            // so `Alt+I` moves on to the workspace listing in the dock.
            KeyCode::Char('i' | 'I') if Self::alt(&key) => {
                let Some(p) = self.workspace_search.as_ref() else {
                    return;
                };
                let (query, case, regex) = (
                    p.query.clone(),
                    p.flags.contains(WorkspaceFlags::CASE_SENSITIVE),
                    p.flags.contains(WorkspaceFlags::REGEX),
                );
                self.workspace_search = None;
                if query.is_empty() {
                    self.prompt = Some(Prompt::new(
                        PromptKind::SearchToDock,
                        t!("prompt.search_dock").to_string(),
                    ));
                } else {
                    self.search_workspace_to_dock(&query, case, regex);
                }
            }
            KeyCode::Char(c) if Self::alt(&key) => {
                if let Some(p) = self.workspace_search.as_mut() {
                    match c.to_ascii_lowercase() {
                        'c' => p.flags.toggle(WorkspaceFlags::CASE_SENSITIVE),
                        'r' => p.flags.toggle(WorkspaceFlags::REGEX),
                        // Replace is a mode of this panel, so a search started
                        // as a find can become a replace without reopening.
                        'h' => {
                            p.flags.toggle(WorkspaceFlags::REPLACING);
                            p.field = if p.flags.contains(WorkspaceFlags::REPLACING) {
                                Field::Replace
                            } else {
                                Field::Query
                            };
                        }
                        _ => {}
                    }
                }
                self.run_workspace_search();
            }
            KeyCode::Backspace => {
                // Editing any field except Replace (query or the path filters)
                // changes the result set, so re-run the search.
                let affects =
                    self.workspace_search.as_ref().map(|p| p.field) != Some(Field::Replace);
                if let Some(p) = self.workspace_search.as_mut() {
                    p.active_field_mut().pop();
                }
                if affects {
                    self.run_workspace_search();
                }
            }
            KeyCode::Char(c) => {
                let affects =
                    self.workspace_search.as_ref().map(|p| p.field) != Some(Field::Replace);
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
        let template = if sb.flags.contains(SearchFlags::REGEX) {
            crate::find_panel::unescape(&sb.replace)
        } else {
            sb.replace.clone()
        };
        let regex = sb.flags.contains(SearchFlags::REGEX);
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
            KeyCode::Char('q' | 'Q') | KeyCode::Esc | KeyCode::Enter => Decision::Quit,
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
            KeyCode::Esc => {
                self.prompt = None;
                self.pending_script_prompt = None;
                self.pending_rebind_action_id = None;
            }
            // Alt+Enter inserts a newline in the multi-line git-commit prompt;
            // plain Enter still submits.
            KeyCode::Enter if Self::alt(&key) => {
                if let Some(p) = self.prompt.as_mut()
                    && matches!(
                        p.kind,
                        PromptKind::GitCommit
                            | PromptKind::OrgCaptureField
                            | PromptKind::OrgCaptureReview
                            | PromptKind::OrgCloseNote
                    )
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
                    && matches!(p.kind, PromptKind::SearchToDock)
                {
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

    /// Dispatch a submitted [`Prompt`] to its kind's handler — one match arm
    /// (or grouped arms sharing a handler) per [`PromptKind`], so the line
    /// count grows with the variant count, not with any one prompt's logic.
    #[allow(clippy::too_many_lines)]
    fn accept_prompt(&mut self) {
        let Some(prompt) = self.prompt.take() else {
            return;
        };
        match prompt.kind {
            PromptKind::Open | PromptKind::SaveAs => {
                self.accept_file_prompt(prompt.kind, prompt.input.trim());
            }
            PromptKind::Rename => self.rename_file(&prompt.input),
            PromptKind::RunCommand => self.run_command(&prompt.input),
            PromptKind::Script => self.accept_script_prompt(&prompt.input),
            PromptKind::RebindKey => self.accept_rebind_key(prompt.input.trim()),
            PromptKind::ThemeSaveAs => self.save_theme_as(prompt.input.trim()),
            PromptKind::SnippetPrefixFromSelection => {
                self.save_snippet_from_selection(prompt.input.trim());
            }
            PromptKind::SearchToDock => {
                self.search_workspace_to_dock(&prompt.input, prompt.case_sensitive, prompt.regex);
            }
            PromptKind::GitCommit => self.git_commit(&prompt.input),
            PromptKind::GitNewBranch => self.git_create_branch(&prompt.input),
            PromptKind::GitClone => self.git_clone(&prompt.input),
            PromptKind::JjClone
            | PromptKind::JjDescribe
            | PromptKind::JjCommit
            | PromptKind::JjEdit
            | PromptKind::JjRebase
            | PromptKind::JjBookmarkCreate
            | PromptKind::JjBookmarkSet
            | PromptKind::JjBookmarkDelete => {
                self.accept_jj_prompt(prompt.kind, &prompt.input);
            }
            PromptKind::GitEditDescription => self.git_edit_description(&prompt.input),
            PromptKind::GitDeleteBranch => self.git_delete_branch(&prompt.input),
            PromptKind::GitGrep => self.git_grep(&prompt.input),
            PromptKind::WorkspaceSymbol => {
                if let Some(path) = self.active_path()
                    && self.lsp.handles(&path)
                {
                    self.lsp
                        .request_workspace_symbols(&path, prompt.input.trim());
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
            PromptKind::InsertFile => self.insert_file_at_cursor(prompt.input.trim()),
            PromptKind::LoadCoverageFile => self.load_coverage_file(prompt.input.trim()),
            PromptKind::ProjectCommand => self.accept_project_command_prompt(&prompt.input),
            PromptKind::OrgSchedule
            | PromptKind::OrgDeadline
            | PromptKind::OrgSparseMatch
            | PromptKind::OrgLinkTarget
            | PromptKind::OrgLinkDesc
            | PromptKind::OrgSetTags
            | PromptKind::OrgSetProperty
            | PromptKind::OrgTableSort
            | PromptKind::OrgColumnsInsertColumn
            | PromptKind::OrgColumnsInsertDblock => {
                self.accept_org_prompt(prompt.kind, prompt.input.trim());
            }
            PromptKind::SaveMacro => self.save_macro(prompt.input.trim()),
            // A closing note may be empty (DONE + CLOSED, no LOGBOOK entry).
            PromptKind::OrgCloseNote => self.org_close_note(prompt.input.trim()),
            PromptKind::OrgAgendaMatch | PromptKind::OrgAgendaSearch => {
                self.accept_org_agenda_prompt(prompt.kind, prompt.input.trim());
            }
            PromptKind::OrgCaptureField
            | PromptKind::OrgCaptureReview
            | PromptKind::RoamFind
            | PromptKind::RoamInsert
            | PromptKind::RoamCapture
            | PromptKind::RoamDailyCapture
            | PromptKind::RoamDailyDate
            | PromptKind::RoamTag
            | PromptKind::RoamAlias
            | PromptKind::RoamRef
            | PromptKind::NodeTransclusion
            | PromptKind::WorkspaceOpen
            | PromptKind::WorkspaceSave
            | PromptKind::WorkspaceAddFolder
            | PromptKind::GotoParagraph
            | PromptKind::GotoSection
            | PromptKind::GotoSentence
            | PromptKind::GotoWord
            | PromptKind::GotoPercent
            | PromptKind::GotoByte => {
                self.accept_roam_prompt(prompt.kind, prompt.input.trim());
            }
            PromptKind::DebugRepl | PromptKind::DebugWatch => {
                self.accept_debug_prompt(prompt.kind, prompt.input.trim());
            }
        }
    }

    /// Handle a completed open/save-as file prompt (`raw` is the trimmed input).
    /// Grouped out of [`App::accept_prompt`] to keep it within the line limit.
    fn accept_file_prompt(&mut self, kind: PromptKind, raw: &str) {
        match kind {
            PromptKind::Open => {
                let (path, target) = palette::parse_path_target(raw);
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
                    Err(e) => self
                        .messages
                        .error(t!("msg.save_failed", error = e).to_string()),
                }
            }
            _ => {}
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
        let pat = if case_sensitive {
            core
        } else {
            format!("(?i){core}")
        };
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

    /// Open the rename prompt for the active file, seeded with its current name.
    fn open_rename_prompt(&mut self) {
        let Some(cur) = self.editor.active_tab().and_then(|t| t.path.clone()) else {
            self.status = t!("status.rename_no_file").to_string();
            return;
        };
        let name = cur
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
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
            cur.parent()
                .map_or_else(|| PathBuf::from(raw), |d| d.join(raw))
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
            Err(e) => self
                .messages
                .error(t!("msg.rename_failed", error = e).to_string()),
        }
    }

    /// Run `cmd` in the workspace root. See [`App::run_command_in`], which
    /// this delegates to — the one async pipeline every command-running
    /// action funnels through.
    fn run_command(&mut self, cmd: &str) {
        let root = self.root.clone();
        self.run_command_in(&root, cmd);
    }

    /// Run a shell command in `dir` (the workspace root for every caller
    /// except `project.subproject.*`, which runs at the nearest subproject's
    /// directory), streaming its output (stdout and stderr merged) into the
    /// bottom dock, which is shown. The command runs in a background
    /// thread; [`App::poll_command`] drains its output each frame and
    /// [`App::cancel_command`] kills it.
    fn run_command_in(&mut self, dir: &Path, cmd: &str) {
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
            .current_dir(dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                self.bottom_dock.push(format!("[error: {e}]"));
                self.messages
                    .error(t!("msg.command_failed", error = e).to_string());
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
            // Pipe closed: the process is finishing. Reap it for the exit code,
            // but NEVER hold the lock across a blocking `wait()` — a cancel on the
            // UI thread takes the same lock to `kill()`, and if a process closed
            // its stdout while still alive, a blocking `wait()` would hold the
            // lock forever and deadlock cancellation. Poll with `try_wait`,
            // releasing the lock between polls, and recover from poisoning so one
            // panicked thread can't take the UI thread down with it.
            let code = loop {
                let status = reader_child
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .try_wait();
                match status {
                    Ok(Some(s)) => break s.code(),
                    Ok(None) => std::thread::sleep(std::time::Duration::from_millis(25)),
                    Err(_) => break None,
                }
            };
            let _ = tx.send(CmdMsg::Done(code));
        });
        self.running_command = Some(RunningCommand {
            rx,
            child,
            label: cmd.to_string(),
        });
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
                    let label = self
                        .running_command
                        .as_ref()
                        .map(|rc| rc.label.clone())
                        .unwrap_or_default();
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
        let note = t!(
            "status.tests_done",
            pass = pass,
            fail = fail,
            ignore = ignore
        )
        .to_string();
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
            // Recover from a poisoned lock so cancellation still works after a
            // panic in the reader thread; the reader never holds this lock across
            // a blocking call, so `kill()` cannot block here.
            let _ = rc
                .child
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .kill();
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
        if let Err(e) = self.store_settings() {
            self.messages.push(
                Level::Warn,
                t!("msg.settings_save_failed", error = e).to_string(),
            );
        }
    }
}

fn rect_contains(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
}

impl App {
    /// Whether an `edit.*` action handled by `run_edit_action` changes the buffer.
    /// Default-deny: only copy/search/navigation/pure-selection are read-only-safe.
    fn edit_action_mutates(action: &str) -> bool {
        const READ_ONLY_SAFE: &[&str] = &[
            "edit.copy",
            "edit.find",
            "edit.find_next",
            "edit.find_prev",
            "edit.go_first",
            "edit.go_last",
            "edit.match_bracket",
            "edit.select_all",
            "edit.select_all_occurrences",
            "edit.select_line",
            "edit.select_less",
            "edit.select_more",
            "edit.select_paragraph",
            "edit.select_section",
            "edit.column_select_down",
            "edit.column_select_up",
            "edit.undo_branch",
        ];
        !READ_ONLY_SAFE.contains(&action)
    }
}

/// Collect every leaf menu item with a keyboard accelerator (recursing into
/// submenus) into `add` as a (translated label, shortcut) pair. Feeds the
/// keyboard-shortcut overlay.
fn collect_menu_shortcuts(items: &[crate::menu::Item], add: &mut impl FnMut(String, String)) {
    for it in items {
        if let Some(sub) = it.submenu {
            collect_menu_shortcuts(sub, add);
        } else if !it.is_separator() && !it.shortcut.is_empty() {
            add(it.label(), it.shortcut.to_string());
        }
    }
}

/// Render any keymap's key token for display: strips `C-`/`S-`/`A-`
/// prefixes in order, each rendered as a named modifier, then the
/// remaining key uppercased (`"C-S-p"` → `"Ctrl Shift P"`; a bare `"C-f"`
/// → `"Ctrl F"`) — **only when at least one prefix was actually found**;
/// a token with none passes through completely unchanged, case included.
/// Handles every already-converted keymap uniformly, Emacs included
/// (T145 — a dedicated `emacs_key_display` used to exist, stripping only
/// a single leading prefix and never uppercasing a bare token; retired
/// once this loop's own "only uppercase after a real prefix" rule turned
/// out to already match it exactly). That "only if prefixed" rule is
/// load-bearing, not cosmetic: Emacs's chord-continuation bindings
/// (`C-x b`'s second key is the bare token `"b"`, not `"C-b"`) are real,
/// lowercase, unprefixed tokens — uppercasing them unconditionally would
/// have been a genuine display regression, caught only by actually
/// tracing what real Emacs tokens look like rather than assuming
/// "uppercase the key" was always safe.
fn modifier_token_display(k: &str) -> String {
    let mut rest = k;
    let mut parts = Vec::new();
    loop {
        if let Some(r) = rest.strip_prefix("C-") {
            parts.push("Ctrl");
            rest = r;
        } else if let Some(r) = rest.strip_prefix("S-") {
            parts.push("Shift");
            rest = r;
        } else if let Some(r) = rest.strip_prefix("A-") {
            parts.push("Alt");
            rest = r;
        } else {
            break;
        }
    }
    // Only uppercase the trailing key when a modifier prefix was actually
    // found — a bare token (no prefix at all) passes through verbatim,
    // matching the retired `emacs_key_display`'s fallback exactly. This
    // matters for real: Emacs's chord-continuation bindings (`C-x b`'s
    // second key, `"b"`) are bare, lowercase tokens with no prefix, and
    // every other keymap's tokens always carry at least a `C-` prefix, so
    // this changes nothing for them.
    if parts.is_empty() {
        return rest.to_string();
    }
    let key = rest.to_uppercase();
    parts
        .into_iter()
        .chain(std::iter::once(key.as_str()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Render a leader key fragment for display (`" "` → `SPC`).
fn display_key(k: &str) -> String {
    if k == " " {
        "SPC".to_string()
    } else {
        k.to_string()
    }
}

/// A short jump label for index `i`: `a`..`z`, then `aa`, `ab`, … (base-26 over
/// lowercase letters), so early lines get single-key labels.
fn jump_label(i: usize) -> String {
    if i < 26 {
        char::from(b'a' + u8::try_from(i).unwrap_or(0)).to_string()
    } else {
        let first = char::from(b'a' + u8::try_from(i / 26 - 1).unwrap_or(0));
        let second = char::from(b'a' + u8::try_from(i % 26).unwrap_or(0));
        format!("{first}{second}")
    }
}

/// The top-level menu index for an `Alt+letter` mnemonic: Vix=0, File=1, Edit=2,
/// View=3 (Alt+I, since "Vix"/"View" both start with V), Go=4 (Alt+N, since Git
/// keeps Alt+G and Alt+J is the recent-locations jump), Run=5 (Alt+R), AI=6,
/// DB=7 (Alt+D), JJ=8 (no mnemonic — Alt+J is the recent-locations jump), Git=9,
/// Org=10, Project=11 (no mnemonic — Alt+P already means `search.prev_selection`
/// in the editor; see `global_shared_key`), Tools=12, Help=13. `None` for any
/// other letter.
fn menu_index_for_alt(c: char) -> Option<usize> {
    match c.to_ascii_lowercase() {
        'v' => Some(0),
        'f' => Some(1),
        'e' => Some(2),
        'i' => Some(3),
        'n' => Some(4),
        'r' => Some(5),
        'a' => Some(6),
        'd' => Some(7),
        'g' => Some(9),
        'o' => Some(10),
        't' => Some(12),
        'h' => Some(13),
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
fn do_replace(
    t: &mut Tab,
    re: &Regex,
    regex: bool,
    template: &str,
    current: (usize, usize),
) -> usize {
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

/// The text of the line `ed`'s cursor is on — `vix-script`'s `current_line()`
/// (§ API v1). `CodeEditor` has no such accessor directly, only the pieces
/// (`code_ref`, `point`, `line_to_char`, `line_len`) to build one from.
fn script_current_line_text(ed: &crate::editor::CodeEditor) -> String {
    let code = ed.code_ref();
    let (row, _col) = code.point(ed.get_cursor());
    let start = code.line_to_char(row);
    ed.get_content_slice(start, start + code.line_len(row))
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
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    /// Stacked modifier prefixes render as separate named words, in order,
    /// with the bare key uppercased (T104c's VS Code/`IntelliJ` display
    /// helper, shared as of T104d).
    #[test]
    fn modifier_token_display_renders_stacked_modifiers() {
        assert_eq!(modifier_token_display("C-p"), "Ctrl P");
        assert_eq!(modifier_token_display("C-S-p"), "Ctrl Shift P");
        // Non-alphabetic keys (backtick) are unaffected by uppercasing.
        assert_eq!(modifier_token_display("C-`"), "Ctrl `");
        // IntelliJ's Ctrl+Alt combination, unlike anything VS Code needs.
        assert_eq!(modifier_token_display("C-A-l"), "Ctrl Alt L");
    }

    /// T145 retired the dedicated single-prefix `emacs_key_display` once
    /// every real Emacs token turned out to render identically through
    /// this one — Emacs bindings never stack more than one modifier
    /// prefix on a single token, so the loop above finds nothing left to
    /// strip after the first, same result a single-prefix check gave.
    #[test]
    fn modifier_token_display_matches_the_retired_emacs_only_renderer() {
        assert_eq!(modifier_token_display("C-x"), "Ctrl X");
        assert_eq!(modifier_token_display("C-c"), "Ctrl C");
        assert_eq!(modifier_token_display("A-Left"), "Alt LEFT");
        // A bare key (no modifier prefix) passes through verbatim, case
        // and all -- real Emacs chord-continuation bindings are exactly
        // this shape (`C-x b`'s second key is the bare token "b", not
        // "C-b"), so uppercasing it here would have been a real display
        // regression, not just a cosmetic risk.
        assert_eq!(modifier_token_display("b"), "b");
    }

    /// macOS folds `Command` into `Control`; every other platform leaves the
    /// `Super` modifier alone, and a chord without `Command` is never touched.
    #[test]
    fn command_folds_into_control_on_macos() {
        let cmd_shift = KeyEvent::new(
            KeyCode::Char('z'),
            KeyModifiers::SUPER | KeyModifiers::SHIFT,
        );
        let folded = App::command_as_control(cmd_shift).modifiers;
        if cfg!(target_os = "macos") {
            assert!(
                folded.contains(KeyModifiers::CONTROL),
                "Command acts as Control"
            );
            assert!(!folded.contains(KeyModifiers::SUPER), "Command is consumed");
            assert!(
                folded.contains(KeyModifiers::SHIFT),
                "the rest of the chord survives"
            );
        } else {
            assert_eq!(
                folded,
                KeyModifiers::SUPER | KeyModifiers::SHIFT,
                "Super is the window manager's key off macOS"
            );
        }
        let ctrl = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(
            App::command_as_control(ctrl).modifiers,
            KeyModifiers::CONTROL
        );
    }

    #[test]
    fn org_contacts_new_field_and_views() {
        let dir = std::env::temp_dir().join(format!("vix-contacts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("contacts.org"),
            "* Ada\n  :PROPERTIES:\n  :EMAIL: ada@x.io\n  :BIRTHDAY: 1815-12-10\n  :END:\n",
        )
        .unwrap();
        let mut app = App::new(dir.clone(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.build_file_index();

        // New Contact runs the built-in Contact capture template: a Name
        // prompt, then Email/Phone/Address/Birthday (left blank here), then
        // files immediately (its `immediate_finish` is set).
        app.run_action("org.capture.contact");
        for c in "Grace".chars() {
            app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        for _ in 0..5 {
            app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        }
        let buf = app.editor.active_tab().unwrap().text();
        assert!(buf.contains("* Grace") && buf.contains(":PROPERTIES:") && buf.contains(":EMAIL:"));

        // Insert Field → Phone adds a property line.
        app.run_action("org.contacts.field.phone");
        assert!(app.editor.active_tab().unwrap().text().contains(":PHONE:"));

        // Find Contacts and vCard export open buffers built from contacts.org.
        app.run_action("org.contacts.find");
        assert!(app.editor.active_tab().unwrap().text().contains("Ada"));
        app.run_action("org.contacts.vcard");
        assert!(
            app.editor
                .active_tab()
                .unwrap()
                .text()
                .contains("BEGIN:VCARD")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn help_menu_overlays_show_license_issue_and_privacy() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.run_action("help.license");
        let lic = app
            .welcome
            .as_ref()
            .expect("license overlay opens")
            .lines()
            .join("\n");
        assert!(
            lic.contains("vix") && lic.contains(env!("CARGO_PKG_VERSION")),
            "version: {lic}"
        );
        assert!(lic.contains("Apache-2.0"), "license from Cargo.toml: {lic}");

        app.run_action("help.report_issue");
        let issue = app
            .welcome
            .as_ref()
            .expect("issue overlay opens")
            .lines()
            .join("\n");
        assert!(
            issue.contains("github.com/vixide/vix/issues"),
            "issue URL: {issue}"
        );

        app.run_action("help.privacy");
        let priv_ = app
            .welcome
            .as_ref()
            .expect("privacy overlay opens")
            .lines()
            .join("\n");
        assert!(
            priv_.to_lowercase().contains("privacy"),
            "privacy text: {priv_}"
        );
    }

    #[test]
    fn workspace_save_open_and_add_folder_round_trip() {
        let base = std::env::temp_dir().join(format!("vix-ws-{}", std::process::id()));
        let proj = base.join("proj");
        let lib = base.join("lib");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(proj.join("main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(lib.join("util.rs"), "pub fn u() {}\n").unwrap();

        // Add the lib folder; the file index should now span both folders.
        let mut app = App::new(proj.clone(), Settings::default());
        app.workspace_add_folder(&lib.to_string_lossy());
        assert!(app.workspace_folders.contains(&lib), "folder added");
        let names: Vec<String> = app
            .file_index
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(
            names.iter().any(|n| n == "util.rs"),
            "index spans added folder: {names:?}"
        );

        // Open a file, save the workspace, then reopen it in a fresh app.
        app.open_path(&proj.join("main.rs"), false);
        let ws_file = base.join("test.vix-workspace");
        app.workspace_save(&ws_file.to_string_lossy());
        assert!(ws_file.exists(), "workspace file written");

        let mut app2 = App::new(base.clone(), Settings::default());
        app2.workspace_open(&ws_file.to_string_lossy());
        assert!(app2.workspace_folders.contains(&proj) && app2.workspace_folders.contains(&lib));
        assert!(
            app2.editor.tabs.iter().any(|t| t
                .path
                .as_ref()
                .and_then(|p| p.file_name())
                .is_some_and(|n| n == "main.rs")),
            "saved file reopened"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn live_backlinks_fill_the_bottom_dock_for_the_active_node() {
        let dir = std::env::temp_dir().join(format!("vix-livebl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("target.org"),
            ":PROPERTIES:\n:ID:       T1\n:END:\n#+title: Target\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("source.org"),
            "#+title: Source\nsee [[id:T1][Target]]\n",
        )
        .unwrap();

        let mut app = App::new(dir.clone(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.build_file_index();
        app.open_path(&dir.join("target.org"), false);
        app.run_action("roam.backlinks_follow");
        assert!(
            app.backlinks_follow && app.show_bottom_dock,
            "live backlinks on, dock shown"
        );
        let joined = app.bottom_dock.lines.join("\n");
        assert!(
            joined.contains("Linked references (1)"),
            "one linked ref: {joined:?}"
        );
        assert!(joined.contains("source.org"), "source listed: {joined:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn minimap_click_jumps_to_proportional_line() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.editor.new_tab_with_content(&"x\n".repeat(100)); // 100 lines
        app.layout.minimap = ratatui::layout::Rect::new(64, 0, 16, 20); // 20 rows tall
        // Row 10 of 20 over 100 lines maps to line 10*100/20 = 50.
        app.minimap_click(10);
        assert_eq!(
            app.editor.cursor_line(),
            50,
            "click maps proportionally to the file"
        );
    }

    #[test]
    fn dailies_calendar_opens_a_daily_note() {
        let dir = std::env::temp_dir().join(format!("vix-cal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut app = App::new(dir.clone(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.run_action("roam.dailies_calendar");
        assert!(
            app.show_calendar && app.calendar_dailies,
            "dailies calendar opened"
        );
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(
            !app.show_calendar && !app.calendar_dailies,
            "calendar closed on accept"
        );
        let opened = app.editor.tabs.iter().any(|t| {
            t.path
                .as_ref()
                .is_some_and(|p| p.to_string_lossy().contains("daily"))
        });
        assert!(opened, "today's daily note opened");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wiki_link_autocompletes_node_titles_on_double_bracket() {
        let dir = std::env::temp_dir().join(format!("vix-wiki-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("alpha.org"), "#+title: Alpha Notes\n").unwrap();
        std::fs::write(dir.join("doc.org"), "\n").unwrap();

        let mut app = App::new(dir.clone(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.build_file_index();
        app.open_path(&dir.join("doc.org"), false);
        // Typing "[[" in an .org file opens node-title completion.
        app.on_key(KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE));
        let popup = app
            .completion
            .as_ref()
            .expect("wiki-link completion opened");
        assert!(
            popup.items.iter().any(|i| i.label == "Alpha Notes"),
            "node title offered"
        );
        // Auto-pair already inserted `]]`, so the insertion must not re-close.
        assert!(
            popup.items.iter().any(|i| i.insert_text == "Alpha Notes"),
            "no double close"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn org_contact_link_completes_email_and_name_on_tab() {
        let dir = std::env::temp_dir().join(format!("vix-contact-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("contacts.org"),
            "* Alice Adams\n  :PROPERTIES:\n  :EMAIL: alice@example.com\n  :END:\n",
        )
        .unwrap();
        std::fs::write(dir.join("doc.org"), "\n").unwrap();

        let mut app = App::new(dir.clone(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.build_file_index();
        app.open_path(&dir.join("doc.org"), false);

        // Typing "[[mailto:" then Tab completes an email address.
        if let Some(t) = app.editor.active_tab_mut() {
            t.editor.set_content("[[mailto:");
            let end = t.editor.code_ref().len_chars();
            t.editor.set_cursor(end);
        }
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        let popup = app.completion.as_ref().expect("mailto completion opened");
        assert!(
            popup
                .items
                .iter()
                .any(|i| i.label == "Alice Adams <alice@example.com>"),
            "email candidate offered"
        );
        assert!(
            popup
                .items
                .iter()
                .any(|i| i.insert_text == "alice@example.com][Alice Adams]]"),
            "completes to a mailto link with a display name"
        );

        // Typing "[[contact:" then Tab completes a contact name instead.
        app.completion = None;
        if let Some(t) = app.editor.active_tab_mut() {
            t.editor.set_content("[[contact:");
            let end = t.editor.code_ref().len_chars();
            t.editor.set_cursor(end);
        }
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        let popup = app.completion.as_ref().expect("contact completion opened");
        assert!(
            popup.items.iter().any(|i| i.label == "Alice Adams"),
            "name candidate offered"
        );
        assert!(
            popup.items.iter().any(|i| i.insert_text == "Alice Adams]]"),
            "completes to a contact-name link"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sticky_header_shows_enclosing_scope_when_scrolled() {
        use std::fmt::Write as _;
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 10);
        let mut body = String::new();
        for i in 0..60 {
            let _ = writeln!(body, "    body{i}");
        }
        app.editor
            .new_tab_with_content(&format!("fn outer() {{\n{body}}}\n"));
        // At the top, the scope header is visible — nothing to pin.
        assert!(
            app.sticky_header().is_none(),
            "no header when the top is visible"
        );
        // Scroll deep into the function; its `fn outer()` line is now off-screen.
        app.editor.goto(40, None, app.layout.editor);
        assert!(
            app.editor.top_visible_line() > 1,
            "scrolled past the header"
        );
        let header = app.sticky_header().expect("enclosing scope is pinned");
        assert!(
            header.contains("fn outer"),
            "header is the enclosing fn: {header:?}"
        );
    }

    #[cfg(feature = "lang-rust")]
    #[test]
    fn structural_selection_expands_to_enclosing_node() {
        let dir = std::env::temp_dir().join(format!("vix-struct-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.rs"); // .rs → the Rust grammar loads, so a tree exists
        std::fs::write(&path, "fn main() { let x = 1; }\n").unwrap();

        let mut app = App::new(dir.clone(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.open_path(&path, false); // no LSP configured → offline tree-sitter path
        if let Some(t) = app.editor.active_tab_mut() {
            t.editor.set_cursor(16); // within "let x = 1;"
        }
        app.run_action("lsp.expand_selection");
        let sel = app.editor.active_tab_mut().unwrap().editor.get_selection();
        assert!(
            sel.is_some_and(|s| s.end > s.start),
            "a node range was selected: {sel:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn highlight_word_marks_all_occurrences_under_the_cursor() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.settings.highlight_word = true;
        app.editor.new_tab_with_content("foo bar foo baz foo");
        // Cursor at offset 0 sits on the first "foo".
        if let Some(t) = app.editor.active_tab_mut() {
            t.editor.set_cursor(0);
        }
        app.refresh_word_highlight();
        let marks = app
            .editor
            .active_tab()
            .unwrap()
            .editor
            .word_marks()
            .cloned()
            .unwrap_or_default();
        assert_eq!(
            marks.len(),
            3,
            "all three 'foo' occurrences marked: {marks:?}"
        );
        assert_eq!(marks[0], (0, 3));
    }

    #[test]
    fn todo_finder_lists_comment_tags() {
        let dir = std::env::temp_dir().join(format!("vix-todo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("a.rs"),
            "fn f() {}\n// TODO: wire it up\nlet todos = 1; // not a tag\n",
        )
        .unwrap();
        std::fs::write(dir.join("b.rs"), "// FIXME broken\n").unwrap();

        let mut app = App::new(dir.clone(), Settings::default());
        app.build_file_index();
        app.run_action("tools.todo_finder");
        let ps = app.workspace_search.as_ref().expect("todo panel opens");
        assert_eq!(
            ps.hits.len(),
            2,
            "TODO + FIXME, not the `todos` identifier: {:?}",
            ps.hits.iter().map(|h| &h.display).collect::<Vec<_>>()
        );
        assert!(
            ps.hits
                .iter()
                .any(|h| h.display.contains("TODO: wire it up"))
        );
        assert!(ps.hits.iter().any(|h| h.display.contains("FIXME broken")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn external_change_reloads_clean_buffer() {
        let dir = std::env::temp_dir().join(format!("vix-reload-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("note.txt");
        std::fs::write(&path, "original\n").unwrap();

        let mut app = App::new(dir.clone(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.open_path(&path, false);
        app.poll_file_changes(); // first sighting records the mtime, no action

        // Change the file on disk, then force the next poll to see a difference
        // (seed an old stored mtime so the test doesn't depend on clock resolution).
        std::fs::write(&path, "changed externally\n").unwrap();
        app.disk_mtimes
            .insert(path.clone(), std::time::SystemTime::UNIX_EPOCH);
        app.last_disk_poll = None; // bypass the throttle
        app.poll_file_changes();

        let text = app.editor.active_tab().unwrap().text();
        assert!(
            text.contains("changed externally"),
            "clean buffer reloaded: {text:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn revert_discards_unsaved_edits() {
        let dir = std::env::temp_dir().join(format!("vix-revert-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("note.txt");
        std::fs::write(&path, "original\n").unwrap();

        let mut app = App::new(dir.clone(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.open_path(&path, false);
        let area = app.layout.editor;
        app.editor.insert_str("edited ", area);
        assert!(app.editor.active_tab().unwrap().dirty);

        app.run_action("file.revert");
        let tab = app.editor.active_tab().unwrap();
        assert_eq!(tab.text(), "original\n", "buffer reverted to disk contents");
        assert!(!tab.dirty, "reverted buffer is clean");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn insert_file_inserts_contents_at_cursor() {
        let dir = std::env::temp_dir().join(format!("vix-insfile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("snippet.txt"), "SNIPPET\n").unwrap();

        let mut app = App::new(dir.clone(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.run_action("file.insert_file");
        let prompt = app.prompt.take().expect("insert-file prompt opened");
        assert!(matches!(prompt.kind, PromptKind::InsertFile));

        // Accept the prompt with a workspace-relative path.
        app.insert_file_at_cursor("snippet.txt");
        let text = app.editor.active_tab().unwrap().text();
        assert!(text.contains("SNIPPET"), "file contents inserted: {text:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn org_schedule_tags_property_and_date_shift() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.editor.new_tab_with_content("* One\nbody\n* Two\n");
        app.org_plan("SCHEDULED", "2026-08-05");
        let text = app.editor.active_tab().unwrap().text();
        assert!(
            text.contains("* One\nSCHEDULED: <2026-08-05 Wed>"),
            "{text:?}"
        );

        if let Some(t) = app.editor.active_tab_mut() {
            t.editor.set_cursor_line(1); // onto the planning line
        }
        app.run_action("org.date_up");
        let text = app.editor.active_tab().unwrap().text();
        assert!(text.contains("SCHEDULED: <2026-08-06 Thu>"), "{text:?}");

        if let Some(t) = app.editor.active_tab_mut() {
            t.editor.set_cursor_line(0);
        }
        app.org_set_tags("work urgent");
        app.org_set_property("Effort 2h");
        let text = app.editor.active_tab().unwrap().text();
        assert!(text.contains("* One :work:urgent:"), "{text:?}");
        assert!(
            text.contains(":PROPERTIES:\n:Effort: 2h\n:END:"),
            "{text:?}"
        );
        // An invalid date is refused with a status note, not applied.
        app.org_plan("DEADLINE", "not-a-date");
        assert!(!app.editor.active_tab().unwrap().text().contains("DEADLINE"));
    }

    #[test]
    fn org_new_heading_and_navigation() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.editor
            .new_tab_with_content("* One\nbody\n** Child\n* Two\n");
        app.run_action("org.new_heading");
        let text = app.editor.active_tab().unwrap().text();
        assert_eq!(text.split('\n').nth(1), Some("* "), "sibling inserted");

        app.run_action("org.nav.next");
        let line = app.editor.active_tab().unwrap().editor.cursor_line();
        assert_eq!(line, 3, "next heading is the child");
        app.run_action("org.nav.up");
        let line = app.editor.active_tab().unwrap().editor.cursor_line();
        assert_eq!(line, 1, "up goes to the parent headline");
    }

    #[test]
    fn org_refile_chooser_moves_subtree_under_target() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.editor
            .new_tab_with_content("* One\n** Task\nbody\n* Two\n");
        if let Some(t) = app.editor.active_tab_mut() {
            t.editor.set_cursor_line(1);
        }
        app.run_action("org.refile");
        let chooser = app.refile_chooser.as_ref().expect("chooser opened");
        // Candidates exclude the subtree being moved.
        let labels: Vec<&str> = chooser.targets.iter().map(|(_, l)| l.as_str()).collect();
        assert_eq!(labels, ["One", "Two"], "source subtree excluded");
        app.refile_chooser_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.refile_chooser_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.refile_chooser.is_none(), "chooser closed");
        let text = app.editor.active_tab().unwrap().text();
        let two = text.find("* Two").expect("target present");
        let task = text.find("** Task").expect("subtree present");
        assert!(task > two, "task refiled under Two: {text:?}");
    }

    #[test]
    fn org_sparse_todo_folds_non_matching_subtrees() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.editor
            .new_tab_with_content("* TODO Ship\nbody\n* Notes\nplain\n");
        app.run_action("org.sparse.todo");
        let tab = app.editor.active_tab().unwrap();
        assert!(!tab.editor.is_line_hidden(1), "TODO body visible");
        assert!(tab.editor.is_line_hidden(3), "non-TODO body hidden");
        // Show All clears the sparse tree.
        app.run_action("editor.unfold_all");
        let tab = app.editor.active_tab().unwrap();
        assert!(!tab.editor.is_line_hidden(3));
    }

    #[test]
    fn org_footnote_roundtrip_in_buffer() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.editor.new_tab_with_content("some text\n");
        if let Some(t) = app.editor.active_tab_mut() {
            t.editor.set_cursor(4);
        }
        app.run_action("org.footnote");
        let text = app.editor.active_tab().unwrap().text();
        assert!(text.starts_with("some[fn:1] text"), "{text:?}");
        assert!(text.contains("* Footnotes"), "{text:?}");
        // From the reference, the action jumps to the definition line.
        if let Some(t) = app.editor.active_tab_mut() {
            t.editor.set_cursor(5); // inside "[fn:1]"
        }
        app.run_action("org.footnote");
        let tab = app.editor.active_tab().unwrap();
        let line = tab.editor.cursor_line();
        let def_line = tab
            .text()
            .split('\n')
            .position(|l| l.starts_with("[fn:1]"))
            .expect("definition line");
        assert_eq!(line, def_line, "jumped to the definition");
    }

    #[test]
    fn org_follow_internal_link_jumps_to_headline() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.editor
            .new_tab_with_content("[[*Two][go]]\n* One\n* Two\n");
        if let Some(t) = app.editor.active_tab_mut() {
            t.editor.set_cursor(2); // inside the link
        }
        app.run_action("org.link.follow");
        let line = app.editor.active_tab().unwrap().editor.cursor_line();
        assert_eq!(line, 2, "cursor jumped to the Two headline");
    }

    #[test]
    fn org_agenda_scoping_by_file_list_and_lock() {
        let dir = std::env::temp_dir().join(format!("vix-agenda-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.org"), "* TODO Alpha\n").unwrap();
        std::fs::write(dir.join("b.org"), "* TODO Beta\n").unwrap();

        let mut app = App::new(dir.clone(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.build_file_index();

        // Default scope: every project .org file.
        assert_eq!(app.org_agenda_files().len(), 2);

        // An explicit file list narrows the scope.
        app.settings.org_agenda_files = vec!["a.org".to_string()];
        let files = app.org_agenda_files();
        assert_eq!(files.len(), 1);
        assert!(files[0].1.ends_with("a.org"), "{:?}", files[0].1);

        // The restriction lock wins over the list.
        app.open_path(&dir.join("b.org"), false);
        app.run_action("org.agenda.lock");
        let files = app.org_agenda_files();
        assert_eq!(files.len(), 1);
        assert!(files[0].1.ends_with("b.org"), "{:?}", files[0].1);

        // Unlocking falls back to the file list.
        app.run_action("org.agenda.unlock");
        let files = app.org_agenda_files();
        assert_eq!(files.len(), 1);
        assert!(files[0].1.ends_with("a.org"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn org_edit_src_roundtrip_applies_edited_body() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.editor
            .new_tab_with_content("* H\n#+begin_src rust\nlet x = 1;\n#+end_src\n");
        if let Some(t) = app.editor.active_tab_mut() {
            t.editor.set_cursor_line(2);
        }
        let tabs_before = app.editor.tabs.len();
        app.run_action("org.edit_src");
        assert_eq!(app.editor.tabs.len(), tabs_before + 1, "dedicated tab");
        assert_eq!(
            app.editor.active_tab().unwrap().text(),
            "let x = 1;\n",
            "body extracted"
        );
        if let Some(t) = app.editor.active_tab_mut() {
            t.editor.set_content("let x = 2;\n");
        }
        app.run_action("org.edit_src");
        assert_eq!(app.editor.tabs.len(), tabs_before, "dedicated tab closed");
        let text = app.editor.active_tab().unwrap().text();
        assert!(
            text.contains("#+begin_src rust\nlet x = 2;\n#+end_src"),
            "body written back: {text:?}"
        );
    }

    #[test]
    fn org_column_view_opens_interactive_overlay_and_edits_the_real_buffer() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.editor
            .new_tab_with_content("* TODO [#1] Ship :work:\n** Sub\n");
        app.editor.active_tab_mut().unwrap().path = Some(PathBuf::from("test.org"));
        let tabs_before = app.editor.tabs.len();
        app.run_action("org.column_view");
        assert!(app.column_view.is_some(), "overlay opened");
        assert_eq!(app.editor.tabs.len(), tabs_before, "no new tab created");

        // Move to the Sub row's TODO column and cycle it: the key must edit
        // the real active tab's buffer text, not a detached copy.
        app.column_view_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.column_view_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        app.column_view_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        let text = app.editor.active_tab().unwrap().text();
        assert!(text.contains("** TODO Sub"), "{text:?}");

        app.column_view_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(app.column_view.is_none(), "q closes the overlay");
    }

    #[test]
    fn org_column_view_no_org_buffer_warns_and_does_not_open() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.editor.new_tab_with_content("plain text\n");
        app.run_action("org.column_view");
        assert!(app.column_view.is_none());
    }

    #[test]
    fn org_column_view_export_still_produces_the_old_style_table() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.editor
            .new_tab_with_content("* TODO [#1] Ship :work:\n** Sub\n");
        app.run_action("org.column_view_export");
        let table = app.editor.active_tab().unwrap().text();
        assert!(table.starts_with("| ITEM | TODO | PRIORITY | TAGS |"));
        assert!(
            table.contains("| Ship | TODO | [#1] | :work: |"),
            "{table:?}"
        );
    }

    #[test]
    fn org_columns_dblock_insert_and_update_round_trip() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.editor
            .new_tab_with_content("* Project\n** TODO Task One\n** DONE Task Two\n");
        app.editor.active_tab_mut().unwrap().path = Some(PathBuf::from("proj.org"));
        if let Some(t) = app.editor.active_tab_mut() {
            t.editor.set_cursor_line(0);
        }
        app.org_columns_insert_dblock("local");
        let text = app.editor.active_tab().unwrap().text();
        assert!(text.contains("#+BEGIN: columnview :id local"), "{text:?}");
        assert!(
            text.contains("Task One") && text.contains("Task Two"),
            "{text:?}"
        );
        assert!(text.contains("#+END:"), "{text:?}");

        // Editing a headline, then updating the dblock in place, must pick
        // up the change.
        if let Some(t) = app.editor.active_tab_mut() {
            let edited = t.text().replace("Task One", "Task One Renamed");
            t.editor.set_content(&edited);
            let line = edited
                .lines()
                .position(|l| l.starts_with("#+BEGIN:"))
                .unwrap();
            t.editor.set_cursor_line(line);
        }
        app.org_columns_update_dblock();
        let updated = app.editor.active_tab().unwrap().text();
        assert!(updated.contains("Task One Renamed"), "{updated:?}");
    }

    #[test]
    fn emacs_c_c_x_chords_dispatch_extended_org_family() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.editor.new_tab_with_content("* Task\n");
        // `C-c C-x` arms the third-key prefix instead of dispatching.
        assert!(app.emacs_c_chord_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL)));
        assert!(app.emacs_c_x_prefix, "C-c C-x arms the extended family");
        app.emacs_c_x_prefix = false;
        // `C-c C-x a` toggles the ARCHIVE tag.
        assert!(app.emacs_c_x_chord_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)));
        let text = app.editor.active_tab().unwrap().text();
        assert!(text.contains(":ARCHIVE:"), "{text:?}");
        // `C-c .` (a non-ctrl second key) inserts a timestamp.
        assert!(app.emacs_c_chord_key(KeyEvent::new(KeyCode::Char('.'), KeyModifiers::NONE)));
        let text = app.editor.active_tab().unwrap().text();
        assert!(text.contains('<') && text.contains('>'), "{text:?}");
    }

    #[test]
    fn project_compile_resolves_cargo_build_and_confirms_before_running() {
        let dir = std::env::temp_dir().join(format!("vix-proj-compile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();

        let mut app = App::new(dir.clone(), Settings::default());
        app.run_action("project.compile");
        let prompt = app.prompt.as_ref().expect("compile opens a confirm prompt");
        assert!(matches!(prompt.kind, PromptKind::ProjectCommand));
        assert_eq!(prompt.input, "cargo build");

        // Accepting runs the (possibly-edited) command, caches it as the new
        // resolved `compile` command, and pushes it onto that slot's history
        // and the project's last-command-of-any-kind.
        app.accept_prompt();
        assert!(
            app.running_command.is_some(),
            "accepting the prompt started the run pipeline"
        );
        assert_eq!(
            app.project_command_cache.compile.as_deref(),
            Some("cargo build")
        );
        assert_eq!(app.project_history.compile, vec!["cargo build".to_string()]);
        assert_eq!(app.project_last_command.as_deref(), Some("cargo build"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn project_discard_command_cache_clears_only_the_cache() {
        let dir = std::env::temp_dir().join(format!("vix-proj-discard-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".vix")).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        std::fs::write(dir.join(".vix/project.toml"), "compile = \"make\"\n").unwrap();

        let mut app = App::new(dir.clone(), Settings::default());
        app.ensure_project_session_loaded();
        app.project_command_cache.compile = Some("cargo build --release".to_string());
        app.project_history.compile = vec!["cargo build".to_string()];

        app.run_action("project.discard_command_cache");

        // The cache is cleared…
        assert!(app.project_command_cache.compile.is_none());
        // …but history…
        assert_eq!(app.project_history.compile, vec!["cargo build".to_string()]);
        // …and the `.vix/project.toml` override are untouched.
        let override_text = std::fs::read_to_string(dir.join(".vix/project.toml")).unwrap();
        assert_eq!(override_text, "compile = \"make\"\n");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn project_test_at_point_runs_the_test_under_cursor_without_a_prompt() {
        let dir =
            std::env::temp_dir().join(format!("vix-proj-test-at-point-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut app = App::new(dir.clone(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.editor.new_tab_with_content(
            "fn helper() {}\n\n#[test]\nfn it_works() {\n    assert!(true);\n}\n",
        );
        app.editor.active_tab_mut().unwrap().path = Some(dir.join("src/lib.rs"));
        if let Some(t) = app.editor.active_tab_mut() {
            t.editor.set_cursor_line(4); // the `assert!(true);` line
        }

        app.run_action("project.test_at_point");
        assert!(
            app.prompt.is_none(),
            "test-at-point runs immediately, with no confirm prompt"
        );
        let rc = app
            .running_command
            .as_ref()
            .expect("the resolved test command started running");
        assert_eq!(rc.label, "cargo test -- --exact it_works");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn project_subproject_actions_report_no_subproject_outside_any_manifest() {
        let dir = std::env::temp_dir().join(format!("vix-proj-subproj-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/main.rs"), "fn main() {}\n").unwrap();
        // No manifest anywhere below `dir`, so no subproject exists.

        let mut app = App::new(dir.clone(), Settings::default());
        app.editor.new_tab_with_content("fn main() {}\n");
        app.editor.active_tab_mut().unwrap().path = Some(dir.join("src/main.rs"));

        app.run_action("project.subproject.compile");
        assert!(app.prompt.is_none());
        assert_eq!(app.status, t!("status.project_no_subproject").to_string());

        app.run_action("project.subproject.find_file");
        assert!(app.palette.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn emacs_c_p_chords_dispatch_project_actions() {
        let dir = std::env::temp_dir().join(format!("vix-proj-chords-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut app = App::new(dir.clone(), Settings::default());
        // `C-c p` arms the third-key prefix instead of dispatching.
        assert!(app.emacs_c_chord_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE)));
        assert!(app.emacs_c_p_prefix, "C-c p arms the project family");
        app.emacs_c_p_prefix = false;
        // `C-c p c` arms the fourth-key prefix.
        assert!(app.emacs_c_p_chord_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)));
        assert!(app.emacs_c_p_c_prefix, "C-c p c arms the command family");
        app.emacs_c_p_c_prefix = false;
        // `C-c p c x` dispatches `project.run_task`; no tasks are defined
        // here, so it reports as much — still proof the chord reached the
        // action rather than falling through to "no chord".
        assert!(app.emacs_c_p_c_chord_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)));
        assert_eq!(app.status, t!("status.no_tasks").to_string());
        // `C-c p c m` arms the fifth-key prefix, and `C-c p c m r` dispatches
        // `project.subproject.run`; no subproject exists here either.
        assert!(app.emacs_c_p_c_chord_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE)));
        assert!(
            app.emacs_c_p_c_m_prefix,
            "C-c p c m arms the subproject family"
        );
        app.emacs_c_p_c_m_prefix = false;
        assert!(app.emacs_c_p_c_m_chord_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)));
        assert_eq!(app.status, t!("status.project_no_subproject").to_string());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn org_block_insert_and_latex_export() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.editor.new_tab_with_content("* Title\n");
        if let Some(t) = app.editor.active_tab_mut() {
            t.editor.set_cursor(8); // end of buffer
        }
        app.run_action("org.block.src");
        let text = app.editor.active_tab().unwrap().text();
        assert!(text.contains("#+begin_src\n\n#+end_src"), "{text:?}");

        app.run_action("org.export_latex");
        let exported = app.editor.active_tab().unwrap().text();
        assert!(exported.contains(r"\section{Title}"), "{exported:?}");
        assert!(exported.contains(r"\begin{document}"));
    }

    #[test]
    fn file_index_honors_gitignore_and_prunes_vendor_dirs() {
        let dir = std::env::temp_dir().join(format!("vix-idx-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::write(dir.join(".gitignore"), "ignored.txt\n").unwrap();
        std::fs::write(dir.join("src/keep.rs"), "fn main() {}\n").unwrap();
        std::fs::write(dir.join("ignored.txt"), "secret\n").unwrap();
        std::fs::write(dir.join("target/build.out"), "artifact\n").unwrap();

        let mut app = App::new(dir.clone(), Settings::default());
        app.build_file_index();
        let names: Vec<String> = app
            .file_index
            .iter()
            .map(|p| {
                p.strip_prefix(&dir)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        assert!(
            names.iter().any(|n| n == "src/keep.rs"),
            "tracked file indexed: {names:?}"
        );
        assert!(
            !names.iter().any(|n| n == "ignored.txt"),
            ".gitignore respected: {names:?}"
        );
        assert!(
            !names.iter().any(|n| n.starts_with("target/")),
            "target pruned: {names:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_edits_to_text_renames_across_lines() {
        use crate::lsp_core::{Encoding, Position, Range};
        let text = "let foo = 1;\nbar(foo);\n";
        // Replace `foo` on line 0 (cols 4..7) and line 1 (cols 4..7) with `baz`.
        let edit = |line: u32, s: u32, e: u32| {
            (
                Range {
                    start: Position { line, character: s },
                    end: Position { line, character: e },
                },
                "baz".to_string(),
            )
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

    // ----- Org table editor (crates/vix-org-table) -------------------------

    #[test]
    fn org_table_tab_advances_field_and_realigns() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.editor.new_tab_with_content("|a|bb|\n");
        app.editor.active_tab_mut().unwrap().path = Some(PathBuf::from("test.org"));
        if let Some(t) = app.editor.active_tab_mut() {
            App::org_table_set_cursor(t, 0, 1); // inside the first field ("a")
        }
        assert!(
            app.org_table_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            "Tab inside a table field must be intercepted"
        );
        let tab = app.editor.active_tab().unwrap();
        let text = tab.text();
        let line0 = text.lines().next().unwrap();
        assert_ne!(
            line0, "|a|bb|",
            "table realigned (padded) by the Tab: {line0:?}"
        );
        assert!(line0.contains('a') && line0.contains("bb"), "{line0:?}");
        let (line, byte_col) = App::org_table_cursor_pos(tab);
        assert_eq!(line, 0);
        assert_eq!(
            App::org_table_field_index_at(line0, byte_col),
            1,
            "cursor advanced to the second field: {byte_col} in {line0:?}"
        );
    }

    #[test]
    fn org_table_enter_advances_or_creates_a_row() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.editor.new_tab_with_content("| a | b |\n");
        app.editor.active_tab_mut().unwrap().path = Some(PathBuf::from("test.org"));
        if let Some(t) = app.editor.active_tab_mut() {
            App::org_table_set_cursor(t, 0, 2); // inside the first field ("a")
        }
        assert!(
            app.org_table_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            "Enter inside a table field must be intercepted"
        );
        let tab = app.editor.active_tab().unwrap();
        assert_eq!(tab.text().lines().count(), 2, "a fresh row was appended");
        assert_eq!(
            tab.editor.cursor_line(),
            1,
            "cursor followed onto the new row"
        );
    }

    #[test]
    fn org_table_key_does_not_intercept_ordinary_text() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.editor
            .new_tab_with_content("* Heading\nSome ordinary text.\n");
        app.editor.active_tab_mut().unwrap().path = Some(PathBuf::from("test.org"));
        if let Some(t) = app.editor.active_tab_mut() {
            t.editor.set_cursor_line(1);
        }
        assert!(
            !app.org_table_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            "Tab on non-table text must fall through to normal indent behavior"
        );
        assert_eq!(
            app.editor.active_tab().unwrap().text(),
            "* Heading\nSome ordinary text.\n",
            "no-op: nothing was intercepted or rewritten"
        );
    }

    #[test]
    fn org_table_insert_row_above_shifts_the_current_row_down() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.editor.new_tab_with_content("| a | b |\n| c | d |\n");
        app.editor.active_tab_mut().unwrap().path = Some(PathBuf::from("test.org"));
        if let Some(t) = app.editor.active_tab_mut() {
            t.editor.set_cursor_line(1); // the "c | d" row
        }
        app.run_action("org.table.insert_row_above");
        let tab = app.editor.active_tab().unwrap();
        let text = tab.text();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "a row was inserted: {lines:?}");
        assert!(lines[0].contains('a') && lines[0].contains('b'));
        assert!(
            !lines[1].contains('c') && !lines[1].contains('d'),
            "the new row is empty: {:?}",
            lines[1]
        );
        assert!(
            lines[2].contains('c') && lines[2].contains('d'),
            "{lines:?}"
        );
        assert_eq!(tab.editor.cursor_line(), 1, "cursor follows the new row");
    }

    #[test]
    fn org_table_sum_column_reports_the_total_on_the_status_line() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.editor.new_tab_with_content("| 1 |\n| 2 |\n| 3 |\n");
        app.editor.active_tab_mut().unwrap().path = Some(PathBuf::from("test.org"));
        app.run_action("org.table.sum_column");
        assert!(
            app.status.contains('6'),
            "status reports the column sum: {:?}",
            app.status
        );
    }

    #[test]
    fn org_table_sort_prompt_opens_and_reorders_rows() {
        let mut app = App::new(std::env::temp_dir(), Settings::default());
        app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
        app.editor.new_tab_with_content("| 3 |\n| 1 |\n| 2 |\n");
        app.editor.active_tab_mut().unwrap().path = Some(PathBuf::from("test.org"));
        app.run_action("org.table.sort");
        let prompt = app.prompt.take().expect("sort prompt opened");
        assert!(matches!(prompt.kind, PromptKind::OrgTableSort));

        // Sort ascending, numerically, by column 1.
        app.accept_org_table_sort("1 n");
        let text = app.editor.active_tab().unwrap().text();
        assert_eq!(text, "| 1 |\n| 2 |\n| 3 |\n", "{text:?}");
    }
}
