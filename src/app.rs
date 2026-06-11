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
use crate::menu::{Menu, MENUS};
use crate::messages::{Level, Messages};
use crate::palette::{self, Action as PAction, Entry, Mode as PMode, Palette};
use crate::project_search::{Hit, ProjectSearch};
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
        .filter_map(|f| f.contents_utf8().and_then(vix_theme_chooser::parse_theme))
        .collect()
}

/// Which dock is being resized by an in-progress edge drag.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DockResize {
    /// The left dock (explorer); drag its right edge.
    Left,
    /// The right dock (messages); drag its left edge.
    Right,
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
}

/// Which kind of single-line prompt is open.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    /// Open-file prompt.
    Open,
    /// Save-as prompt.
    SaveAs,
}

/// A single-line input prompt (open / save-as).
pub struct Prompt {
    /// Which prompt this is.
    pub kind: PromptKind,
    /// Title shown in the prompt border.
    pub title: String,
    /// Current input text.
    pub input: String,
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

/// Theme chooser overlay state (View -> Themes), re-exported from
/// [`vix_theme_chooser`]. Moving the selection previews the theme live; Enter
/// commits and persists it, Esc reverts.
pub use vix_theme_chooser::Chooser as ThemeChooser;

/// Locale chooser overlay state (View -> Locale), re-exported from
/// [`vix_locale_chooser`]. Moving the selection previews the language live;
/// Enter commits and persists it, Esc reverts.
pub use vix_locale_chooser::Chooser as LocaleChooser;

/// Keyway chooser overlay state (View -> Keyway), re-exported from
/// [`vix_keyway_chooser`]. Moving the selection highlights a keyboard navigation
/// style; Enter commits and persists it, Esc reverts.
pub use vix_keyway_chooser::Chooser as KeywayChooser;

/// Nerd Font palette overlay state (Tools -> Nerd Font Palette), re-exported from
/// [`vix_nerd_font_palette`]. Arrow keys move within the glyph grid; Enter (or a
/// click) inserts the highlighted glyph into the active editor, Esc closes.
pub use vix_nerd_font_palette::Palette as NerdPalette;

/// The active keyboard navigation style, derived from `settings.keyway`. It
/// decides how raw key events are dispatched (see [`App::on_key`]): `Apple` uses
/// modifier shortcuts, `Emacs` uses `Ctrl` chords, `Vim` is modal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Keyway {
    /// Modifier-key shortcuts (the default), e.g. `Ctrl+O` to open.
    Apple,
    /// `Ctrl` chords and the `Ctrl+X` prefix, e.g. `Ctrl+X Ctrl+F` to open.
    Emacs,
    /// Modal editing: a Normal mode for motions/commands and an Insert mode.
    Vim,
}

impl Keyway {
    /// Parse a persisted keyway id; anything unrecognized is [`Keyway::Apple`].
    fn from_id(id: &str) -> Self {
        match id {
            "emacs" => Keyway::Emacs,
            "vim" => Keyway::Vim,
            _ => Keyway::Apple,
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
    /// Row list rectangle of the open chooser overlay (theme/locale/keyway), so a
    /// click can hit-test which row was picked.
    pub chooser: Rect,
    /// Glyph-grid rectangle of the open Nerd Font palette, so a click can
    /// hit-test which cell was picked.
    pub nerd_palette: Rect,
}

/// The whole application state.
pub struct App {
    /// Project root directory.
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
    /// Project-wide search panel, when open.
    pub project_search: Option<ProjectSearch>,
    /// Single-line prompt, when open.
    pub prompt: Option<Prompt>,
    /// In-progress paste operation, when active.
    pub paste: Option<PasteOp>,
    /// Pending confirmation, when active.
    pub confirm: Option<Confirm>,
    /// Theme chooser overlay, when open.
    pub theme_chooser: Option<ThemeChooser>,
    /// Locale chooser overlay, when open.
    pub locale_chooser: Option<LocaleChooser>,
    /// Keyway chooser overlay, when open.
    pub keyway_chooser: Option<KeywayChooser>,
    /// Recent-files chooser overlay, when open.
    pub recent_chooser: Option<RecentChooser>,
    /// Nerd Font palette (character picker) overlay, when open.
    pub nerd_palette: Option<NerdPalette>,
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
    /// Whether the calendar box is shown.
    pub show_calendar: bool,
    /// Month navigation state for the calendar box.
    pub calendar: crate::calendar::Calendar,
    /// Whether the keyboard-help overlay is shown.
    pub show_help: bool,
    /// Status-bar text.
    pub status: String,
    /// Set to request application exit.
    pub should_quit: bool,
    /// Pane rectangles recorded during the last render.
    pub layout: Layout,
    /// File paths under the project root, for the palette file finder.
    file_index: Vec<PathBuf>,
    /// Cursor offset captured when the palette opened, so the `:` go-to-line
    /// preview can revert on cancel and the jump records the true origin.
    palette_origin: Option<usize>,
    /// True while the editor scrollbar thumb is being dragged, so the drag keeps
    /// scrolling even if the pointer drifts off the one-column track.
    scrollbar_active: bool,
    /// Which dock (if any) is being resized by an in-progress edge drag.
    dock_resize: Option<DockResize>,
    /// Emacs keyway: a `Ctrl+X` prefix has been pressed and the next key
    /// completes the chord. Always false in other keyways.
    emacs_prefix: bool,
    /// Vim keyway: true in Insert mode, false in Normal mode. Meaningless in
    /// other keyways.
    vim_insert: bool,
    /// Vim keyway: the in-progress `:` command-line text, when the command line
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
        let editor = Editor::new(
            settings.line_numbers,
            settings.show_whitespace,
            settings.soft_wrap,
            settings.indent_string(),
        );
        let mut messages = Messages::default();
        messages.advice(t!("msg.welcome").to_string());
        messages.info(t!("msg.welcome_hint").to_string());

        App {
            explorer: Explorer::new(root.clone()),
            root,
            editor,
            messages,
            menu: Menu::default(),
            palette: None,
            search: None,
            query_replace: None,
            project_search: None,
            prompt: None,
            paste: None,
            confirm: None,
            theme_chooser: None,
            locale_chooser: None,
            keyway_chooser: None,
            recent_chooser: None,
            nerd_palette: None,
            dialog: None,
            clip: Vec::new(),
            clip_cut: false,
            nav_history: Vec::new(),
            nav_idx: 0,
            picker: None,
            show_explorer: settings.show_explorer,
            show_messages: settings.show_messages,
            show_calendar: false,
            calendar: crate::calendar::Calendar::new(),
            show_help: false,
            focus: Focus::Editor,
            status: t!("status.ready").to_string(),
            should_quit: false,
            layout: Layout::default(),
            settings,
            file_index: Vec::new(),
            palette_origin: None,
            scrollbar_active: false,
            dock_resize: None,
            emacs_prefix: false,
            vim_insert: false,
            vim_cmd: None,
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
        // Modal layers, in priority order.
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
            match key.code {
                KeyCode::Left => self.calendar.prev_month(),
                KeyCode::Right => self.calendar.next_month(),
                KeyCode::Esc | KeyCode::Char('q') => self.show_calendar = false,
                _ => {}
            }
            return;
        }
        if self.theme_chooser.is_some() {
            self.theme_key(key);
            return;
        }
        if self.locale_chooser.is_some() {
            self.locale_key(key);
            return;
        }
        if self.keyway_chooser.is_some() {
            self.keyway_key(key);
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
        if self.query_replace.is_some() {
            self.qr_key(key);
            return;
        }
        if self.project_search.is_some() {
            self.ps_key(key);
            return;
        }
        if self.confirm.is_some() {
            self.confirm_key(key);
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
        // Keyway-specific dispatch. Each keyway first gets a chance to consume the
        // key; `Emacs`/`Vim` then fall back to the shared keys (menu mnemonics and
        // function keys) before the focused pane handles it.
        match self.active_keyway() {
            Keyway::Apple => {
                if self.global_key(key) {
                    return;
                }
            }
            Keyway::Emacs => {
                if self.emacs_key(key) || self.global_shared_key(key) {
                    return;
                }
            }
            Keyway::Vim => {
                if self.vim_key(key) || self.global_shared_key(key) {
                    return;
                }
            }
        }
        match self.focus {
            Focus::Editor => self.editor_key(key),
            Focus::Explorer => self.explorer_key(key),
            Focus::Messages => self.messages_key(key),
        }
    }

    /// The keyboard navigation style currently in effect.
    fn active_keyway(&self) -> Keyway {
        Keyway::from_id(&self.settings.keyway)
    }

    /// A short keyway-mode indicator for the status bar (Vim's mode / command
    /// line, or Emacs's pending chord prefix), or `None` when there is nothing to
    /// show (e.g. the Apple keyway).
    #[must_use]
    pub fn mode_indicator(&self) -> Option<String> {
        match self.active_keyway() {
            Keyway::Vim => Some(if let Some(cmd) = &self.vim_cmd {
                format!(":{cmd}")
            } else if self.vim_insert {
                t!("status.vim_insert").to_string()
            } else {
                t!("status.vim_normal").to_string()
            }),
            Keyway::Emacs if self.emacs_prefix => Some("C-x-".to_string()),
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

    /// Global shortcuts available when no modal is active (Apple keyway). Returns
    /// true if the key was consumed.
    fn global_key(&mut self, key: KeyEvent) -> bool {
        self.apple_ctrl_key(key) || self.global_shared_key(key)
    }

    /// The Apple keyway's `Ctrl`-letter shortcuts. Returns true if consumed.
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
                    'w' => self.run_action("file.close"),
                    'p' => self.run_action("tools.palette"),
                    'b' => self.run_action("view.explorer"),
                    'e' => self.toggle_focus_explorer_editor(),
                    'f' if Self::shift(&key) => self.run_action("search.project"),
                    'f' => self.run_action("edit.find"),
                    'r' if Self::alt(&key) => self.run_action("edit.query_replace"),
                    'r' => self.run_action("edit.replace"),
                    '/' => self.run_action("edit.toggle_comment"),
                    _ => return false,
                }
                return true;
            }
        }
        false
    }

    /// Keys shared by every keyway: menu-bar mnemonics and function keys. Returns
    /// true if consumed.
    fn global_shared_key(&mut self, key: KeyEvent) -> bool {
        if Self::alt(&key) {
            if let KeyCode::Char(c) = key.code {
                // The Vix menu is index 0; the rest follow (File=1, …, Help=5).
                let idx = match c.to_ascii_lowercase() {
                    'f' => Some(1),
                    'e' => Some(2),
                    'v' => Some(3),
                    't' => Some(4),
                    'h' => Some(5),
                    _ => None,
                };
                if let Some(i) = idx {
                    self.menu.open_index(i);
                    return true;
                }
            }
        }
        match key.code {
            KeyCode::Left if Self::alt(&key) => {
                self.nav_back();
                true
            }
            KeyCode::Right if Self::alt(&key) => {
                self.nav_forward();
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

    // ----- keyway: Emacs --------------------------------------------------

    /// Feed a key to the editor as if it were typed with no modifiers, but only
    /// when the editor pane is focused. Used to translate keyway motions
    /// (`Ctrl+F`, `l`, …) into the editor's existing handling.
    fn editor_motion(&mut self, code: KeyCode) {
        if self.focus == Focus::Editor {
            self.editor_key(KeyEvent::new(code, KeyModifiers::NONE));
        }
    }

    /// Emacs keyway dispatch: the `Ctrl+X` prefix and `Ctrl`-key chords. Returns
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

    // ----- keyway: Vim ----------------------------------------------------

    /// Vim keyway dispatch: Normal-mode motions/commands, Insert mode, and the
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
            "q!" => self.run_action("file.quit"),
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
                self.prompt = Some(Prompt {
                    kind: PromptKind::Open,
                    title: t!("prompt.open").to_string(),
                    input: String::new(),
                });
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
                self.prompt = Some(Prompt {
                    kind: PromptKind::SaveAs,
                    title: t!("prompt.save_as").to_string(),
                    input: cur,
                });
            }
            "file.close" => {
                self.editor.close_active();
                self.status = t!("status.closed_buffer").into();
            }
            "file.quit" => self.should_quit = true,
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
            "edit.find" => self.start_search(false),
            "edit.replace" => self.start_search(true),
            "edit.query_replace" => {
                self.start_search(true);
                if let Some(s) = self.search.as_mut() {
                    s.interactive = true;
                }
            }
            "search.project" => self.open_project_search(false),
            "search.project_replace" => self.open_project_search(true),
            "search.next_selection" => self.find_selection(true),
            "search.prev_selection" => self.find_selection(false),
            "nav.goto_definition" => self.goto_definition(),
            "nav.goto_symbol" => self.open_palette_seeded("@"),
            "view.theme" => self.open_theme_chooser(),
            "view.locale" => self.open_locale_chooser(),
            "view.keyway" => self.open_keyway_chooser(),
            "tools.calendar" => {
                self.show_calendar = !self.show_calendar;
                // Always open on the present month; navigation is per-session.
                if self.show_calendar {
                    self.calendar.reset();
                }
            }
            "tools.nerd_palette" => self.open_nerd_palette(),
            "tools.palette" => self.open_palette(),
            // The left/right docks are the explorer and message drawers. Both the
            // old action ids and the new dock-named ones route to one method.
            "view.line_numbers" | "tools.line_numbers" => self.toggle_editor_line_numbers(),
            "view.whitespace" => self.toggle_editor_whitespace(),
            "view.soft_wrap" => self.toggle_editor_soft_wrap(),
            "view.left_dock" | "view.explorer" => self.toggle_left_dock(),
            "view.right_dock" | "view.messages" => self.toggle_right_dock(),
            "tab.next" => self.editor.next_tab(),
            "tab.prev" => self.editor.prev_tab(),
            "help.shortcuts" => self.show_help = true,
            "vix.about" => {
                self.dialog = Some(Dialog {
                    title: t!("menu.item.vix.about").to_string(),
                    body: format!("Vix {}", env!("CARGO_PKG_VERSION")),
                    editor: None,
                });
            }
            "vix.website" => self.open_text_dialog(
                t!("menu.item.vix.website").to_string(),
                "https://github.com/joelparkerhenderson/vix",
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
            Ok(p) => self.status = t!("status.saved", path = p.display()).to_string(),
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
                KeyCode::Char('v' | 'x' | 'z' | 'y' | 'k' | 'd')
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
                self.explorer.toggle_selected();
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

    /// Record a real (non-preview) file open at the front of the recent list,
    /// de-duplicated and capped. Stored canonicalized so reopening is reliable.
    fn record_recent(&mut self, path: &Path) {
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let entry = canon.to_string_lossy().into_owned();
        let recent = &mut self.settings.recent_files;
        recent.retain(|p| p != &entry);
        recent.insert(0, entry);
        recent.truncate(crate::settings::MAX_RECENT_FILES);
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
        if self.theme_chooser.is_some() {
            self.theme_mouse(mouse);
            return;
        }
        if self.locale_chooser.is_some() {
            self.locale_mouse(mouse);
            return;
        }
        if self.keyway_chooser.is_some() {
            self.keyway_mouse(mouse);
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
        // Keyboard-only modal overlays swallow all mouse input rather than
        // letting a click fall through to the editor/explorer underneath.
        if self.show_help
            || self.show_calendar
            || self.palette.is_some()
            || self.prompt.is_some()
            || self.search.is_some()
            || self.query_replace.is_some()
            || self.project_search.is_some()
            || self.confirm.is_some()
            || self.paste.as_ref().is_some_and(|p| p.conflict.is_some())
        {
            return;
        }
        let (col, row) = (mouse.column, mouse.row);

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
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) if Some(col) == left_edge => {
                self.dock_resize = Some(DockResize::Left);
                return;
            }
            MouseEventKind::Down(MouseButton::Left) if Some(col) == right_edge => {
                self.dock_resize = Some(DockResize::Right);
                return;
            }
            MouseEventKind::Drag(MouseButton::Left) if self.dock_resize.is_some() => {
                self.resize_dock(col);
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
            None => {}
        }
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
        let inner_top = self.layout.explorer.y + 1; // inside the border
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
        let dd = self.layout.menu_dropdown;
        if rect_contains(dd, col, row) {
            // Items start one row below the dropdown's top border.
            let top = dd.y + 1;
            if let Some(mi) = self.menu.open {
                let items = MENUS[mi].items;
                let idx = row.saturating_sub(top) as usize;
                if row >= top && idx < items.len() && !items[idx].is_separator() {
                    let action = items[idx].action;
                    self.menu.close();
                    self.run_action(action);
                }
            }
            return;
        }
        // Clicked outside both the bar and the dropdown: dismiss the menu.
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
        for (i, m) in MENUS.iter().enumerate() {
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
        let dd = self.layout.menu_dropdown;
        if rect_contains(dd, col, row) {
            // Items start one row below the dropdown's top border.
            let top = dd.y + 1;
            if let Some(mi) = self.menu.open {
                let items = MENUS[mi].items;
                let idx = row.saturating_sub(top) as usize;
                if row >= top && idx < items.len() && !items[idx].is_separator() {
                    self.menu.item = idx;
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
                if let Some(action) = self.menu.selected_action() {
                    self.menu.close();
                    self.run_action(action);
                }
            }
            KeyCode::Esc | KeyCode::F(10) => self.menu.close(),
            _ => {}
        }
    }

    // ----- theme chooser --------------------------------------------------

    /// Custom themes available to choose from: those installed in the user's
    /// themes directory first (so they win on a name clash), then the themes
    /// bundled into the binary.
    fn available_custom_themes() -> Vec<crate::theme::CustomTheme> {
        let mut themes = Settings::themes_dir()
            .map(|d| vix_theme_chooser::load_custom_themes(&d))
            .unwrap_or_default();
        themes.extend(bundled_themes());
        themes
    }

    /// Apply a persisted theme value: a built-in mode (`"dark"`/`"light"`) or a
    /// custom theme by name (user-installed or bundled).
    fn apply_saved_theme(value: &str) {
        if value == "dark" || value == "light" {
            crate::theme::set_mode(crate::theme::Mode::from_name(value));
            crate::theme::set_custom(None);
            return;
        }
        match Self::available_custom_themes()
            .into_iter()
            .find(|t| t.name == value)
        {
            Some(theme) => crate::theme::set_custom(Some(theme)),
            // Unknown name: fall back to the default built-in.
            None => crate::theme::set_mode(crate::theme::Mode::Dark),
        }
    }

    fn open_theme_chooser(&mut self) {
        self.theme_chooser = Some(ThemeChooser::open(Self::available_custom_themes()));
    }

    /// Display name for a theme choice (built-in names are translated).
    fn choice_label(choice: &vix_theme_chooser::Choice) -> String {
        match choice.builtin() {
            Some(mode) => t!(mode.label()).to_string(),
            None => choice.custom_name().unwrap_or_default().to_string(),
        }
    }

    fn theme_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Down => {
                if let Some(tc) = self.theme_chooser.as_mut() {
                    if key.code == KeyCode::Up {
                        tc.up();
                    } else {
                        tc.down();
                    }
                    // Preview the highlighted theme live.
                    vix_theme_chooser::apply(tc.selected_choice());
                }
                self.editor.refresh_theme();
            }
            KeyCode::Enter => {
                if let Some(tc) = self.theme_chooser.take() {
                    let choice = tc.selected_choice().clone();
                    vix_theme_chooser::apply(&choice);
                    self.editor.refresh_theme();
                    self.settings.theme = choice.id();
                    self.status =
                        t!("status.theme", theme = Self::choice_label(&choice)).to_string();
                }
            }
            KeyCode::Esc => {
                if let Some(tc) = self.theme_chooser.take() {
                    vix_theme_chooser::apply(tc.original_choice());
                    self.editor.refresh_theme();
                    self.status = t!("status.theme_unchanged").into();
                }
            }
            _ => {}
        }
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

    // ----- keyway chooser -------------------------------------------------

    fn open_keyway_chooser(&mut self) {
        self.keyway_chooser = Some(KeywayChooser::open(&self.settings.keyway));
    }

    fn keyway_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Down => {
                if let Some(kc) = self.keyway_chooser.as_mut() {
                    if key.code == KeyCode::Up {
                        kc.up();
                    } else {
                        kc.down();
                    }
                }
            }
            KeyCode::Enter => {
                if let Some(kc) = self.keyway_chooser.take() {
                    let id = kc.selected_id();
                    self.settings.keyway = id.to_string();
                    self.reset_keyway_modes();
                    self.status = t!("status.keyway", keyway = id).to_string();
                }
            }
            KeyCode::Esc => {
                // Only reachable while the chooser is open, so just discard it.
                self.keyway_chooser = None;
                self.status = t!("status.keyway_unchanged").to_string();
            }
            _ => {}
        }
    }

    /// Reset per-keyway session state (Emacs chord prefix, Vim mode/command line)
    /// so a freshly chosen keyway starts clean — Vim begins in Normal mode.
    fn reset_keyway_modes(&mut self) {
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

    fn theme_mouse(&mut self, mouse: MouseEvent) {
        if let Some(idx) = self.chooser_row(mouse) {
            if let Some(tc) = self.theme_chooser.as_mut() {
                if idx < tc.choices.len() {
                    tc.selected = idx;
                    // Preview the highlighted theme live, as Up/Down does.
                    vix_theme_chooser::apply(tc.selected_choice());
                    self.editor.refresh_theme();
                }
            }
        }
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

    fn keyway_mouse(&mut self, mouse: MouseEvent) {
        if let Some(idx) = self.chooser_row(mouse) {
            if let Some(kc) = self.keyway_chooser.as_mut() {
                if idx < vix_keyway_chooser::KEYWAYS.len() {
                    kc.selected = idx;
                }
            }
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
        let Some(pat) = self.search.as_ref().and_then(super::search::SearchBar::pattern) else {
            self.clear_marks();
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
        let count = self.find_with(&re, forward);
        if let Some(s) = self.search.as_mut() {
            s.status = if count == 0 {
                t!("status.no_matches").into()
            } else {
                t!("status.matches", count = count).to_string()
            };
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
        let Ok(re) = Regex::new(&regex::escape(&query)) else {
            return;
        };
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
        let mut matches: Vec<(usize, usize)> = Vec::new();
        for m in re.find_iter(&content) {
            let code = t.editor.code_ref();
            matches.push((code.byte_to_char(m.start()), code.byte_to_char(m.end())));
        }
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
        let count = re.find_iter(&text).count();
        let new_text = if use_regex {
            let rep = unescape(&replacement);
            re.replace_all(&text, rep.as_str()).into_owned()
        } else {
            re.replace_all(&text, regex::NoExpand(&replacement)).into_owned()
        };
        tab.editor.set_content(&new_text);
        tab.editor.remove_marks();
        tab.dirty = true;
        tab.preview = false;
        if let Some(s) = self.search.as_mut() {
            s.status = t!("status.replaced", count = count).to_string();
        }
    }

    // ----- project-wide search / replace ---------------------------------

    /// Heuristic "go to definition": find likely definitions of the identifier
    /// under the cursor across the project (keyword-prefixed declarations). Not
    /// a semantic LSP — fast, offline, and language-agnostic.
    fn goto_definition(&mut self) {
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
                let mut ps = ProjectSearch::new(false);
                ps.query.clone_from(&symbol);
                ps.static_results = true;
                ps.hits = hits;
                ps.status = t!("status.definitions_n", n = n, symbol = symbol).to_string();
                self.project_search = Some(ps);
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

    fn open_project_search(&mut self, replacing: bool) {
        self.build_file_index();
        self.project_search = Some(ProjectSearch::new(replacing));
        self.run_project_search();
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

    fn run_project_search(&mut self) {
        // Static result lists (e.g. go-to-definition) are not re-searched.
        if self.project_search.as_ref().is_some_and(|p| p.static_results) {
            return;
        }
        let Some(ps) = self.project_search.as_ref() else {
            return;
        };
        let Some(pat) = ps.pattern() else {
            if let Some(p) = self.project_search.as_mut() {
                p.hits.clear();
                p.selected = 0;
                p.status = t!("status.project_search_prompt").into();
            }
            return;
        };
        let re = match Regex::new(&pat) {
            Ok(r) => r,
            Err(e) => {
                if let Some(p) = self.project_search.as_mut() {
                    p.status = t!("msg.bad_regex", error = e).to_string();
                }
                return;
            }
        };

        let mut hits: Vec<Hit> = Vec::new();
        let mut files = 0usize;
        'outer: for path in &self.file_index {
            let Some(content) = self.current_text(path) else {
                continue;
            };
            let rel = path
                .strip_prefix(&self.root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned();
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

        if let Some(p) = self.project_search.as_mut() {
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

    fn project_replace_all(&mut self) {
        let Some(ps) = self.project_search.as_ref() else {
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
                if let Some(p) = self.project_search.as_mut() {
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
            let count = re.find_iter(&content).count();
            if count == 0 {
                continue;
            }
            let new = if use_regex {
                let rep = unescape(&replacement);
                re.replace_all(&content, rep.as_str()).into_owned()
            } else {
                re.replace_all(&content, regex::NoExpand(&replacement)).into_owned()
            };
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

        self.run_project_search();
        if let Some(p) = self.project_search.as_mut() {
            p.status = t!("status.replaced_in_files", replaced = replaced, files = files).to_string();
        }
    }

    fn ps_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.project_search = None,
            KeyCode::Up => {
                if let Some(p) = self.project_search.as_mut() {
                    p.up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.project_search.as_mut() {
                    p.down();
                }
            }
            KeyCode::Tab => {
                if let Some(p) = self.project_search.as_mut() {
                    p.toggle_field();
                }
            }
            KeyCode::Enter => {
                let replacing = self.project_search.as_ref().is_some_and(|p| p.replacing);
                let on_replace = self.project_search.as_ref().map(|p| p.field) == Some(Field::Replace);
                if replacing && (Self::alt(&key) || on_replace) {
                    self.project_replace_all();
                } else {
                    self.open_selected_hit();
                }
            }
            KeyCode::Char(c) if Self::alt(&key) => {
                if let Some(p) = self.project_search.as_mut() {
                    match c.to_ascii_lowercase() {
                        'c' => p.case_sensitive = !p.case_sensitive,
                        'r' => p.regex = !p.regex,
                        _ => {}
                    }
                }
                self.run_project_search();
            }
            KeyCode::Backspace => {
                let in_query = self.project_search.as_ref().map(|p| p.field) == Some(Field::Query);
                if let Some(p) = self.project_search.as_mut() {
                    p.active_field_mut().pop();
                }
                if in_query {
                    self.run_project_search();
                }
            }
            KeyCode::Char(c) => {
                let in_query = self.project_search.as_ref().map(|p| p.field) == Some(Field::Query);
                if let Some(p) = self.project_search.as_mut() {
                    p.active_field_mut().push(c);
                }
                if in_query {
                    self.run_project_search();
                }
            }
            _ => {}
        }
    }

    fn open_selected_hit(&mut self) {
        let target = self
            .project_search
            .as_ref()
            .and_then(|p| p.selected_hit())
            .map(|h| (h.path.clone(), h.line, h.col));
        if let Some((path, line, col)) = target {
            self.project_search = None;
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
            unescape(&sb.replace)
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
        if let Err(e) = self.settings.save() {
            self.messages
                .push(Level::Warn, t!("msg.settings_save_failed", error = e).to_string());
        }
    }
}

fn rect_contains(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
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
fn next_match_from(t: &Tab, re: &Regex, from_char: usize) -> Option<(usize, usize)> {
    let content = t.text();
    let code = t.editor.code_ref();
    let bfrom = code.char_to_byte(from_char);
    for m in re.find_iter(&content) {
        if m.start() >= bfrom {
            return Some((code.byte_to_char(m.start()), code.byte_to_char(m.end())));
        }
    }
    None
}

/// Replace the single match at char offset `current.0` in `t`, returning the
/// char offset just past the inserted text (where searching should resume).
fn do_replace(t: &mut Tab, re: &Regex, regex: bool, template: &str, current: (usize, usize)) -> usize {
    let content = t.text();
    let bstart = t.editor.code_ref().char_to_byte(current.0);
    let mut expansion: Option<(usize, usize, String)> = None;
    for caps in re.captures_iter(&content) {
        let m = caps.get(0).unwrap();
        if m.start() == bstart {
            let mut out = String::new();
            if regex {
                caps.expand(template, &mut out);
            } else {
                out.push_str(template);
            }
            expansion = Some((m.start(), m.end(), out));
            break;
        }
        if m.start() > bstart {
            break;
        }
    }
    match expansion {
        Some((bs, be, exp)) => {
            let mut new = String::with_capacity(content.len() + exp.len());
            new.push_str(&content[..bs]);
            new.push_str(&exp);
            new.push_str(&content[be..]);
            let resume = current.0 + exp.chars().count();
            t.editor.set_content(&new);
            resume
        }
        None => current.1,
    }
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

/// Interpret `\n`, `\t`, `\r`, `\\` escapes in a regex replacement template,
/// leaving `$group` references intact for the regex engine to expand.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') | None => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

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
    fn renders_project_search_panel_with_hits() {
        let dir = std::env::temp_dir().join(format!("vix-ps-unit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("note.txt"), "the needle is here\n").unwrap();

        let mut app = App::new(dir.clone(), Settings::default());
        app.run_action("search.project");
        for c in "needle".chars() {
            app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        assert_eq!(app.project_search.as_ref().unwrap().hits.len(), 1);

        let mut terminal = Terminal::new(TestBackend::new(100, 40)).unwrap();
        terminal.draw(|f| crate::ui::draw(&mut app, f)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("Search in Project"), "panel title rendered");
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
