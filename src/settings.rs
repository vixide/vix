//! User settings, persisted with [`confy`].
//!
//! Settings live in the platform configuration directory under the application
//! name `vix` (e.g. `~/.config/vix/config.toml` on Linux). [`confy`] picks the
//! right location per OS and handles (de)serialization, so this module only
//! defines the schema and thin load/save wrappers.
//!
//! ```
//! use vix::settings::Settings;
//!
//! // Defaults are always available even when no config file exists yet.
//! let defaults = Settings::default();
//! assert_eq!(defaults.theme, "dark");
//! assert_eq!(defaults.locale, "en");
//! ```

use serde::{Deserialize, Serialize};

/// Application name used by [`confy`] to locate the config directory.
const APP_NAME: &str = "vix";

/// Config file stem; the on-disk file is `config.<ext>` (e.g. `config.toml`).
const CONFIG_NAME: &str = "config";

/// Persisted user preferences.
///
/// Every field has a default (see [`Settings::default`]); `#[serde(default)]`
/// lets older config files load even when new fields are added.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Show the line-number gutter.
    pub line_numbers: bool,
    /// Render visible glyphs for whitespace (space, tab, line ending).
    pub show_whitespace: bool,
    /// Wrap long lines across screen rows instead of scrolling horizontally.
    pub soft_wrap: bool,
    /// Show the file explorer on startup.
    pub show_explorer: bool,
    /// Show the message drawer on startup.
    pub show_messages: bool,
    /// Show the bottom status bar.
    pub show_status_bar: bool,
    /// Show the editor's right-side scroll bar.
    pub show_scrollbar: bool,
    /// Show the bottom dock (log/output/data panel).
    pub show_bottom_dock: bool,
    /// Height (rows) of the bottom dock; drag its top edge to resize.
    pub bottom_dock_height: u16,
    /// Maximum lines retained in the bottom dock (scrollback); oldest dropped past this.
    pub scrollback: usize,
    /// Open single-clicked / arrow-scanned files in an ephemeral preview tab.
    pub preview_tabs: bool,
    /// On save, strip trailing spaces/tabs from every line.
    pub trim_trailing_whitespace: bool,
    /// On save, append a final newline if the file does not end with one.
    pub ensure_final_newline: bool,
    /// Indentation inserted by Tab: `"spaces"` (default) or `"tabs"`.
    pub indent_style: String,
    /// Number of spaces per indent when `indent_style` is `"spaces"`.
    pub tab_width: usize,
    /// Color theme: `"dark"` (default) or `"light"`.
    pub theme: String,
    /// UI language as a locale code (e.g. `"en"`, `"es"`, `"fr"`, `"de"`, `"cy"`).
    /// Used as the default; a `--locale` CLI flag overrides it for one run.
    pub locale: String,
    /// Keyboard navigation style id: `"apple"` (default), `"vscode"`, `"emacs"`,
    /// or `"vim"`.
    pub keymap: String,
    /// Width (columns) of the left dock (file explorer); drag its right edge to
    /// resize.
    pub explorer_width: u16,
    /// Width (columns) of the right dock (message drawer); drag its left edge to
    /// resize.
    pub messages_width: u16,
    /// Recently opened files, most-recent first (absolute paths). Capped to
    /// [`recent_files_max`](Self::recent_files_max); surfaced by **File → Open
    /// Recent…**.
    pub recent_files: Vec<String>,
    /// How many entries to keep in [`recent_files`](Self::recent_files).
    pub recent_files_max: usize,
    /// Underline misspelled words in comments and strings.
    pub spellcheck: bool,
    /// Extra directory to search for Hunspell dictionaries, on top of the
    /// autodetected standard locations. Empty = autodetect only. Both the
    /// `<dir>/<name>.{aff,dic}` and `<dir>/<name>/index.{aff,dic}` layouts work.
    pub dictionary_path: String,
    /// Master switch for Language Server Protocol features (diagnostics, hover,
    /// go-to-definition, completion). When off, no servers are launched.
    pub lsp_enabled: bool,
    /// Configured language servers, matched to files by extension. Each entry is
    /// a language id (sent to the server), the file extensions it handles, and
    /// the command (program + args) to launch. Empty by default — Vix ships no
    /// built-in server, so add the ones you have installed, e.g.
    /// `{ language_id = "rust", extensions = ["rs"], command = ["rust-analyzer"] }`.
    pub lsp_servers: Vec<LspServer>,
}

/// One configured language server (a `lsp_servers` entry).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspServer {
    /// LSP `languageId` sent in `didOpen` (e.g. `"rust"`, `"python"`).
    pub language_id: String,
    /// File extensions (without the dot) this server handles, e.g. `["rs"]`.
    pub extensions: Vec<String>,
    /// Launch command: program then args, e.g. `["rust-analyzer"]`.
    pub command: Vec<String>,
}

/// Default cap for [`Settings::recent_files`] (the `recent_files_max` setting).
pub const MAX_RECENT_FILES: usize = 15;

impl Default for Settings {
    fn default() -> Self {
        Settings {
            line_numbers: true,
            show_whitespace: false,
            soft_wrap: false,
            show_explorer: true,
            show_messages: true,
            show_status_bar: true,
            show_scrollbar: true,
            show_bottom_dock: false,
            bottom_dock_height: 9,
            scrollback: 1000,
            preview_tabs: true,
            trim_trailing_whitespace: true,
            ensure_final_newline: true,
            indent_style: "spaces".to_string(),
            tab_width: 4,
            theme: "dark".to_string(),
            locale: "en".to_string(),
            keymap: "apple".to_string(),
            explorer_width: 30,
            messages_width: 32,
            recent_files: Vec::new(),
            recent_files_max: MAX_RECENT_FILES,
            spellcheck: false,
            dictionary_path: String::new(),
            lsp_enabled: true,
            lsp_servers: Vec::new(),
        }
    }
}

impl Settings {
    /// Load settings from the user's config directory, falling back to
    /// [`Settings::default`] on any error (missing file, parse failure, …).
    #[must_use]
    pub fn load() -> Settings {
        confy::load(APP_NAME, Some(CONFIG_NAME)).unwrap_or_default()
    }

    /// The string Tab inserts: a tab character for `indent_style = "tabs"`, else
    /// [`Settings::tab_width`] spaces. An empty width falls back to one space.
    #[must_use]
    pub fn indent_string(&self) -> String {
        if self.indent_style == "tabs" {
            "\t".to_string()
        } else {
            " ".repeat(self.tab_width.max(1))
        }
    }

    /// Persist settings to the user's config directory.
    ///
    /// # Errors
    ///
    /// Returns a [`confy::ConfyError`] if the config directory cannot be
    /// created or the file cannot be written/serialized.
    pub fn save(&self) -> Result<(), confy::ConfyError> {
        confy::store(APP_NAME, Some(CONFIG_NAME), self)
    }

    /// Directory holding custom JSON themes (`<config dir>/themes/`), or `None`
    /// if the config location cannot be determined.
    #[must_use]
    pub fn themes_dir() -> Option<std::path::PathBuf> {
        confy::get_configuration_file_path(APP_NAME, Some(CONFIG_NAME))
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("themes")))
    }
}
