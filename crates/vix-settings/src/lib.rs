//! User settings, persisted with [`confy`].
//!
//! Settings live in the platform configuration directory under the application
//! name `vix` (e.g. `~/.config/vix/config.toml` on Linux). [`confy`] picks the
//! right location per OS and handles (de)serialization, so this module only
//! defines the schema and thin load/save wrappers.
//!
//! ```
//! use vix_settings::Settings;
//!
//! // Defaults are always available even when no config file exists yet.
//! let defaults = Settings::default();
//! assert_eq!(defaults.theme, "dark");
//! assert_eq!(defaults.locale, "en");
//! ```

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use serde::{Deserialize, Serialize};

/// Application name used by [`confy`] to locate the config directory.
const APP_NAME: &str = "vix";

/// Config file stem; the on-disk file is `config.<ext>` (e.g. `config.toml`).
const CONFIG_NAME: &str = "config";

/// Persisted user preferences.
///
/// Every field has a default (see [`Settings::default`]); `#[serde(default)]`
/// lets older config files load even when new fields are added.
///
/// Boolean toggles are grouped into small `#[serde(flatten)]`'d sub-structs
/// (T149) rather than left as ~30 direct `bool` fields on `Settings` itself —
/// `clippy::struct_excessive_bools` counts bools in any one struct, so a
/// single large group doesn't dodge the lint, only many small ones do.
/// `#[serde(flatten)]` keeps the on-disk `config.toml` format unchanged: each
/// field still round-trips as its own flat top-level key, exactly as if it
/// were still declared directly on `Settings`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Line-number gutter and inline whitespace-glyph display (T149).
    #[serde(flatten)]
    pub gutter: GutterSettings,
    /// Soft-wrap, rainbow brackets, and word-highlight visual aids (T149).
    #[serde(flatten)]
    pub editor_visual: EditorVisualSettings,
    /// Explorer, message drawer, and bottom dock visibility (T149).
    #[serde(flatten)]
    pub panels: PanelSettings,
    /// Status bar, breadcrumb bar, and outline sidebar visibility (T149).
    #[serde(flatten)]
    pub secondary_panels: SecondaryPanelSettings,
    /// Scrollbar, minimap, and menu-tooltip visibility (T149).
    #[serde(flatten)]
    pub viewport: ViewportSettings,
    /// Independent toggles with no natural sibling group (T149).
    #[serde(flatten)]
    pub misc: MiscSettings,
    /// What happens to a file's content on save, before it's written (T149).
    #[serde(flatten)]
    pub save: SaveSettings,
    /// Editor-session behaviors: autosave, sticky scroll, persistent undo (T149).
    #[serde(flatten)]
    pub editor_behavior: EditorBehaviorSettings,
    /// Typing-time behaviors: editorconfig, auto-pair, spellcheck (T149).
    #[serde(flatten)]
    pub typing: TypingSettings,
    /// What happens when Vix starts (T149).
    #[serde(flatten)]
    pub startup: StartupSettings,
    /// Whole-subsystem on/off switches: LSP, modal engine, inline blame (T149).
    #[serde(flatten)]
    pub subsystems: SubsystemSettings,
    /// How the file explorer's Delete acts: `"trash"` (default — move to the
    /// OS trash/Recycle Bin, undoable from there) or `"hard"` (remove
    /// outright, `fs::remove_file`/`remove_dir_all`, no undo). Any other
    /// value falls back to `"trash"`, the safer default, rather than
    /// silently hard-deleting on a typo.
    pub explorer_delete: String,
    /// Height (rows) of the bottom dock; drag its top edge to resize.
    pub bottom_dock_height: u16,
    /// Maximum lines retained in the bottom dock (scrollback); oldest dropped past this.
    pub scrollback: usize,
    /// Indentation inserted by Tab: `"spaces"` (default) or `"tabs"`.
    pub indent_style: String,
    /// Number of spaces per indent when `indent_style` is `"spaces"`.
    pub tab_width: usize,
    /// Column that Edit → Wrap hard-wraps (fills) text at.
    pub wrap_column: usize,
    /// Color theme: `"dark"` (default) or `"light"`.
    pub theme: String,
    /// UI language as a locale code (e.g. `"en"`, `"es"`, `"fr"`, `"de"`, `"cy"`).
    /// Used as the default; a `--locale` CLI flag overrides it for one run.
    pub locale: String,
    /// Keyboard navigation style id: `"apple"` (default), `"vscode"`, `"emacs"`,
    /// or `"vi"`.
    pub keymap: String,
    /// Width (columns) of the left dock (file explorer); drag its right edge to
    /// resize.
    pub explorer_width: u16,
    /// Width (columns) of the right dock (message drawer); drag its left edge to
    /// resize.
    pub messages_width: u16,
    /// Width (columns) of the code-outline sidebar.
    pub outline_width: u16,
    /// Width (columns) of the debugger panel (call stack / variables / watch).
    pub debug_width: u16,
    /// Recently opened files, most-recent first (absolute paths). Capped to
    /// [`recent_files_max`](Self::recent_files_max); surfaced by **File → Open
    /// Recent…**.
    pub recent_files: Vec<String>,
    /// How many entries to keep in [`recent_files`](Self::recent_files).
    pub recent_files_max: usize,
    /// Action ids of commands recently run from the command palette, most-recent
    /// first; surfaced at the top of the `>` command list.
    pub command_recents: Vec<String>,
    /// Extra directory to search for Hunspell dictionaries, on top of the
    /// autodetected standard locations. Empty = autodetect only. Both the
    /// `<dir>/<name>.{aff,dic}` and `<dir>/<name>/index.{aff,dic}` layouts work.
    pub dictionary_path: String,
    /// Configured language servers, matched to files by extension. Each entry is
    /// a language id (sent to the server), the file extensions it handles, and
    /// the command (program + args) to launch. Empty by default — Vix ships no
    /// built-in server, so add the ones you have installed, e.g.
    /// `{ language_id = "rust", extensions = ["rs"], command = ["rust-analyzer"] }`.
    pub lsp_servers: Vec<LspServer>,
    /// Directory of vCard (`.vcf`) files for the contact browser (Tools →
    /// Contacts…). Empty = use the workspace root.
    pub contacts_dir: String,
    /// Org-capture templates for the **Org → Capture** submenu: named,
    /// placeholder-driven templates filed at a cursor/id/file/headline/
    /// datetree target. Seeded with `Anything`/`Todo`/`Contact` by default;
    /// see the `vix-org-capture` crate and `crates/vix-org-capture/spec/index.md`.
    pub org_capture_templates: Vec<vix_org_capture::CaptureTemplate>,
    /// Highest-priority character for a headline's `[#X]` priority cookie
    /// (**Org → Priority**). "Highest" sorts first — Vix's default is
    /// numeric (`'0'` highest .. `'9'` lowest), unlike Emacs's default
    /// `'A'`..`'C'`.
    pub org_priority_highest: char,
    /// Lowest-priority character; see `org_priority_highest`.
    pub org_priority_lowest: char,
    /// Priority given to a headline that had no cookie yet, by **Org →
    /// Priority → Increase/Decrease**.
    pub org_priority_default: char,
    /// The Org agenda's explicit file list (workspace-relative paths),
    /// managed by **Org → Agenda → File List**. Empty (the default) means
    /// every `.org` file in the project — the pre-list behavior.
    pub org_agenda_files: Vec<String>,
    /// Active time zone as an IANA canonical name (e.g. `"UTC"`,
    /// `"America/New_York"`). Chosen via Tools → Time Zone…; used app-wide
    /// (e.g. the clock panel).
    pub time_zone: String,
    /// Command template the **AI** menu runs over editor text. The placeholder
    /// `{prompt}` is replaced with the action's instruction; if the template also
    /// contains `{file}` it is replaced with the path of a temp file holding the
    /// input text, otherwise that file is redirected to the command's stdin. This
    /// lets you point the AI menu at any CLI assistant — `claude` (default),
    /// `codex`, `mistral`, `ollama run …`, etc. See [`Settings::ai_command_line`].
    pub ai_command: String,
    /// Which backend the AI menu, chat panel, and DB workbench assistant use.
    /// `"cli"` (default) shells out to [`ai_command`](Self::ai_command),
    /// unchanged since the AI features first shipped — no API key for Vix to
    /// hold. `"anthropic"`, `"openai"`, or `"ollama"` instead call that
    /// provider's HTTP API directly via the `vix-ai-core` crate, using
    /// `ai_endpoint`/`ai_model` and a keyring-backed API key (see
    /// `ai_api_key_command` and `crates/vix-ai-core/spec/index.md`). An
    /// unrecognized value falls back to `"cli"`.
    pub ai_provider: String,
    /// HTTP provider endpoint override (T124); empty uses the provider's own
    /// default (e.g. Anthropic's public Messages API, or
    /// `http://localhost:11434` for Ollama). Ignored when `ai_provider` is
    /// `"cli"`.
    pub ai_endpoint: String,
    /// HTTP provider model id override (T124); empty uses the provider's own
    /// default. Ignored when `ai_provider` is `"cli"`.
    pub ai_model: String,
    /// Command whose stdout is the HTTP provider's API key (T124), tried
    /// before the OS keyring — the same shape as `vix-db`'s
    /// `password_command` (e.g. `"pass show anthropic-api-key"`). Ignored
    /// for `ai_provider = "cli"`; optional for `"ollama"` (a local server
    /// needs no key by default).
    pub ai_api_key_command: String,
    /// Configured debug adapters (DAP), matched to files by extension. Empty by
    /// default — add the adapters you have installed.
    pub debug_adapters: Vec<vix_dap::DebugAdapter>,
    /// Command run by **Tools → Run Tests**, whose output is parsed into a
    /// pass/fail tree (e.g. `cargo test`, `pytest -v`, `npm test`).
    pub test_command: String,
    /// Project snippet file, relative to the project root. Loaded alongside the
    /// global and media-type snippet files. See the `vix-snippets` crate spec.
    pub project_snippets: String,
    /// Default path to a coverage report (LCOV or Cobertura XML), pre-filled
    /// in the **Tools → Load Coverage File…** prompt. Empty means no
    /// default; Vix never generates a coverage report itself. See the
    /// `vix-coverage` crate spec.
    pub coverage_path: String,
    /// Width (columns) of the test-results panel.
    pub test_width: u16,
    /// Saved database connections for the **DB** menu (the `vix-db` crate spec). Passwords
    /// are never stored here; they are prompted for per session.
    pub db_connections: Vec<vix_db::connect::Connection>,
}

/// Line-number gutter and inline whitespace-glyph display toggles (T149 —
/// see [`Settings`]'s own doc comment for why these are grouped).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GutterSettings {
    /// Show the line-number gutter.
    pub line_numbers: bool,
    /// Show line numbers relative to the cursor line (hybrid: cursor line absolute).
    pub relative_line_numbers: bool,
    /// Render visible glyphs for whitespace (space, tab, line ending).
    pub show_whitespace: bool,
}

impl Default for GutterSettings {
    fn default() -> Self {
        GutterSettings {
            line_numbers: true,
            relative_line_numbers: false,
            show_whitespace: false,
        }
    }
}

/// Editor visual-aid toggles: how lines wrap, and what gets colored or
/// highlighted while editing (T149).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorVisualSettings {
    /// Wrap long lines across screen rows instead of scrolling horizontally.
    pub soft_wrap: bool,
    /// Color matching brackets by nesting depth (rainbow brackets).
    pub rainbow_brackets: bool,
    /// Passively highlight every occurrence of the word under the cursor.
    pub highlight_word: bool,
}

/// Primary panel visibility on startup: the file explorer, message drawer,
/// and bottom dock (T149).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelSettings {
    /// Show the file explorer on startup.
    pub show_explorer: bool,
    /// Show the message drawer on startup.
    pub show_messages: bool,
    /// Show the bottom dock (log/output/data panel).
    pub show_bottom_dock: bool,
}

impl Default for PanelSettings {
    fn default() -> Self {
        PanelSettings {
            show_explorer: true,
            show_messages: true,
            show_bottom_dock: true,
        }
    }
}

/// Secondary panel visibility: the status bar, breadcrumb bar, and
/// code-outline sidebar (T149).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SecondaryPanelSettings {
    /// Show the bottom status bar.
    pub show_status_bar: bool,
    /// Show the breadcrumb bar (file ▸ enclosing symbol) above the editor.
    pub show_breadcrumbs: bool,
    /// Show the code-outline sidebar (symbol list that follows the cursor).
    pub show_outline_dock: bool,
}

impl Default for SecondaryPanelSettings {
    fn default() -> Self {
        SecondaryPanelSettings {
            show_status_bar: true,
            show_breadcrumbs: false,
            show_outline_dock: false,
        }
    }
}

/// Editor-adjacent chrome visibility: the scroll bar, minimap, and menu
/// tooltips (T149).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewportSettings {
    /// Show the editor's right-side scroll bar.
    pub show_scrollbar: bool,
    /// Show a code-overview minimap column at the right of the editor.
    pub show_minimap: bool,
    /// Show hover tooltips (help text) on the menu bar's menus and items.
    pub show_menu_tooltips: bool,
}

impl Default for ViewportSettings {
    fn default() -> Self {
        ViewportSettings {
            show_scrollbar: true,
            show_minimap: false,
            show_menu_tooltips: true,
        }
    }
}

/// Independent toggles that don't share a natural sibling group (T149):
/// ephemeral preview tabs, sticky search highlights, and AI diff review.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MiscSettings {
    /// Open single-clicked / arrow-scanned files in an ephemeral preview tab.
    pub preview_tabs: bool,
    /// Keep search-match highlights visible after the Find box closes ("sticky"
    /// highlights), until they are explicitly toggled off. When false, closing
    /// Find clears the highlights.
    pub sticky_search_highlight: bool,
    /// Review AI replace transforms (Annotate / Improve) as an accept/reject diff
    /// before applying, instead of overwriting the text immediately. On by default.
    pub ai_diff_review: bool,
}

impl Default for MiscSettings {
    fn default() -> Self {
        MiscSettings {
            preview_tabs: true,
            sticky_search_highlight: true,
            ai_diff_review: true,
        }
    }
}

/// What happens to a file's content on save, before it's written (T149).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SaveSettings {
    /// On save, strip trailing spaces/tabs from every line.
    pub trim_trailing_whitespace: bool,
    /// On save, append a final newline if the file does not end with one.
    pub ensure_final_newline: bool,
    /// On save, run the language server's formatter (when the file has one).
    pub format_on_save: bool,
}

impl Default for SaveSettings {
    fn default() -> Self {
        SaveSettings {
            trim_trailing_whitespace: true,
            ensure_final_newline: true,
            format_on_save: false,
        }
    }
}

/// Editor-session behaviors: periodic autosave, sticky scroll, and
/// cross-session persistent undo (T149).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorBehaviorSettings {
    /// Periodically save the active dirty file-backed buffer (every few seconds).
    pub auto_save: bool,
    /// Pin the enclosing scope's header line at the top of the editor while
    /// scrolling (sticky scroll).
    pub sticky_scroll: bool,
    /// Persist each file's undo tree across sessions (restored on reopen when the
    /// file content still matches).
    pub persistent_undo: bool,
}

impl Default for EditorBehaviorSettings {
    fn default() -> Self {
        EditorBehaviorSettings {
            auto_save: false,
            sticky_scroll: true,
            persistent_undo: true,
        }
    }
}

/// Typing-time behaviors: `.editorconfig` overrides, bracket/quote
/// auto-pairing, and spellcheck underlines (T149).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TypingSettings {
    /// Apply `.editorconfig` rules (indent style/size, trim/final-newline on save)
    /// for opened files, overriding the global settings per file. On by default.
    pub editorconfig: bool,
    /// Auto-insert the matching closer when an opening bracket/quote is typed (and
    /// delete both with Backspace inside an empty pair). On by default.
    pub auto_pair: bool,
    /// Underline misspelled words in comments and strings.
    pub spellcheck: bool,
}

impl Default for TypingSettings {
    fn default() -> Self {
        TypingSettings {
            editorconfig: true,
            auto_pair: true,
            spellcheck: false,
        }
    }
}

/// What happens when Vix starts (T149): restoring the previous session, and
/// showing the first-run welcome dialog.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StartupSettings {
    /// Reopen the previous session (open files, focused tab, cursor positions)
    /// when Vix is launched in a workspace with no file given on the command
    /// line. The session is saved per workspace root in `session.toml`.
    pub restore_session: bool,
    /// Show the welcome dialog on launch. Vix sets it to `false` and saves the
    /// settings as soon as the dialog has been shown once, so it appears on the
    /// first run only; set it back to `true` to see it again.
    pub show_welcome_dialog: bool,
}

impl Default for StartupSettings {
    fn default() -> Self {
        StartupSettings {
            restore_session: true,
            show_welcome_dialog: true,
        }
    }
}

/// Whole-subsystem on/off switches (T149): the LSP client, the modal
/// (Vim-style) editing engine, and inline git blame.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SubsystemSettings {
    /// Master switch for Language Server Protocol features (diagnostics, hover,
    /// go-to-definition, completion). When off, no servers are launched.
    pub lsp_enabled: bool,
    /// Use `vix-modal`'s real mode engine (Normal/Insert/Visual/Visual Line,
    /// composable operators, counts, registers, text objects, dot-repeat) for
    /// the Vi and Spacemacs keymaps' Normal-mode vocabulary, instead of the
    /// original flat binding table. On by default since T115 shipped the
    /// full v1 slice (improvement plan T112–T115); a few real-Vim nuances
    /// are still deliberately unimplemented (see
    /// `crates/vix-modal/spec/index.md`'s own § Rollout and its T112–T115
    /// status notes) — turn this off to fall back to the original table if
    /// one of them matters to you.
    pub modal_engine: bool,
    /// Show the git blame for the cursor's line inline (dimmed, end of line). Off
    /// by default; toggle via **Git → Toggle Inline Blame**.
    pub inline_blame: bool,
}

impl Default for SubsystemSettings {
    fn default() -> Self {
        SubsystemSettings {
            lsp_enabled: true,
            modal_engine: true,
            inline_blame: false,
        }
    }
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
            gutter: GutterSettings::default(),
            editor_visual: EditorVisualSettings::default(),
            panels: PanelSettings::default(),
            secondary_panels: SecondaryPanelSettings::default(),
            viewport: ViewportSettings::default(),
            misc: MiscSettings::default(),
            save: SaveSettings::default(),
            editor_behavior: EditorBehaviorSettings::default(),
            typing: TypingSettings::default(),
            startup: StartupSettings::default(),
            subsystems: SubsystemSettings::default(),
            explorer_delete: "trash".to_string(),
            bottom_dock_height: 9,
            scrollback: 1000,
            indent_style: "spaces".to_string(),
            tab_width: 4,
            wrap_column: 80,
            theme: "dark".to_string(),
            locale: "en".to_string(),
            keymap: "apple".to_string(),
            explorer_width: 30,
            messages_width: 32,
            outline_width: 28,
            debug_width: 36,
            recent_files: Vec::new(),
            recent_files_max: MAX_RECENT_FILES,
            command_recents: Vec::new(),
            dictionary_path: String::new(),
            lsp_servers: Vec::new(),
            contacts_dir: String::new(),
            org_capture_templates: vix_org_capture::defaults(),
            org_priority_highest: '0',
            org_priority_lowest: '9',
            org_priority_default: '0',
            org_agenda_files: Vec::new(),
            time_zone: "UTC".to_string(),
            // Placeholders are single-quoted by `ai_command_line`, so the
            // template must NOT add quotes of its own (doing so would let chat
            // text break out and inject shell commands).
            ai_command: "claude -p {prompt}".to_string(),
            ai_provider: "cli".to_string(),
            ai_endpoint: String::new(),
            ai_model: String::new(),
            ai_api_key_command: String::new(),
            debug_adapters: Vec::new(),
            test_command: "cargo test".to_string(),
            test_width: 40,
            project_snippets: "config/snippets/snippets.json".to_string(),
            coverage_path: String::new(),
            db_connections: Vec::new(),
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

    /// Load settings from an explicit file, falling back to
    /// [`Settings::default`] on any error (missing file, parse failure, …).
    /// Used by tests and embedders that keep a config outside the user's config
    /// directory; [`Settings::load`] is the normal entry point.
    #[must_use]
    pub fn load_from(path: &std::path::Path) -> Settings {
        confy::load_path(path).unwrap_or_default()
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

    /// Build the shell command the AI menu runs for `prompt` over the input text
    /// stored at `file`, expanding the [`ai_command`](Self::ai_command) template.
    ///
    /// `{prompt}` and `{file}` are substituted as **POSIX single-quoted** strings
    /// so that a chat message — free user text, e.g. containing `"`, `` ` ``,
    /// `$(…)`, or `;` — cannot break out of its argument and inject shell
    /// commands when the result is run via `sh -c`. Because the placeholders
    /// arrive pre-quoted, templates must **not** wrap them in quotes of their own
    /// (the built-in default does not). If the template contains `{file}` it is
    /// substituted; otherwise the text is fed on stdin via an appended redirect.
    /// An empty template falls back to the default `claude` invocation.
    #[must_use]
    pub fn ai_command_line(&self, prompt: &str, file: &str) -> String {
        let template = if self.ai_command.trim().is_empty() {
            "claude -p {prompt}"
        } else {
            self.ai_command.as_str()
        };
        let with_prompt = template.replace("{prompt}", &sh_single_quote(prompt));
        if with_prompt.contains("{file}") {
            with_prompt.replace("{file}", &sh_single_quote(file))
        } else {
            format!("{with_prompt} < {}", sh_single_quote(file))
        }
    }

    /// Persist settings to the user's config directory. `config.toml` can
    /// carry sensitive content (e.g. a custom `ai_command` template with an
    /// embedded API key), so on Unix it's narrowed to owner-only after each
    /// save (T133) — best-effort, and not perfectly race-free at creation
    /// time (`confy` gives no hook to choose the mode as the file is made,
    /// only after), but this is a single-user local desktop config
    /// directory, not a shared or network-exposed one.
    ///
    /// # Errors
    ///
    /// Returns a [`confy::ConfyError`] if the config directory cannot be
    /// created or the file cannot be written/serialized.
    pub fn save(&self) -> Result<(), confy::ConfyError> {
        confy::store(APP_NAME, Some(CONFIG_NAME), self)?;
        if let Some(path) = Self::config_path() {
            vix_fileops::restrict_to_owner(&path);
        }
        Ok(())
    }

    /// Persist settings to an explicit file (the counterpart of
    /// [`Settings::load_from`]); parent directories are created as needed.
    /// Narrowed to owner-only on Unix, same as [`Settings::save`].
    ///
    /// # Errors
    ///
    /// Returns a [`confy::ConfyError`] if the file cannot be written or
    /// serialized.
    pub fn save_to(&self, path: &std::path::Path) -> Result<(), confy::ConfyError> {
        confy::store_path(path, self)?;
        vix_fileops::restrict_to_owner(path);
        Ok(())
    }

    /// The on-disk settings file path (e.g. `~/.config/vix/config.toml`), or
    /// `None` if the config location cannot be determined.
    #[must_use]
    pub fn config_path() -> Option<std::path::PathBuf> {
        confy::get_configuration_file_path(APP_NAME, Some(CONFIG_NAME)).ok()
    }

    /// Directory holding custom JSON themes (`<config dir>/themes/`), or `None`
    /// if the config location cannot be determined.
    #[must_use]
    pub fn themes_dir() -> Option<std::path::PathBuf> {
        confy::get_configuration_file_path(APP_NAME, Some(CONFIG_NAME))
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("themes")))
    }

    /// Directory holding global user `.rhai` scripts (`<config dir>/scripts/`),
    /// or `None` if the config location cannot be determined. See
    /// `crates/vix-script/spec/index.md`, "Script discovery".
    #[must_use]
    pub fn scripts_dir() -> Option<std::path::PathBuf> {
        confy::get_configuration_file_path(APP_NAME, Some(CONFIG_NAME))
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("scripts")))
    }

    /// File holding the user's personal spellcheck word list, one word per line
    /// (`<config dir>/user_dictionary.txt`), or `None` if the config location
    /// cannot be determined. Words added via the spell-suggest popup persist here.
    #[must_use]
    pub fn user_dictionary_path() -> Option<std::path::PathBuf> {
        confy::get_configuration_file_path(APP_NAME, Some(CONFIG_NAME))
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("user_dictionary.txt")))
    }

    /// File holding saved keyboard macros (`<config dir>/macros.toml`), or `None`
    /// if the config location cannot be determined.
    #[must_use]
    pub fn macros_path() -> Option<std::path::PathBuf> {
        confy::get_configuration_file_path(APP_NAME, Some(CONFIG_NAME))
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("macros.toml")))
    }

    /// File holding the user's persisted key binding overrides
    /// (`<config dir>/keybindings.toml`), or `None` if the config location
    /// cannot be determined. See `crates/vix-keybindings/spec/index.md`,
    /// "Persisted user overrides".
    #[must_use]
    pub fn keybindings_path() -> Option<std::path::PathBuf> {
        confy::get_configuration_file_path(APP_NAME, Some(CONFIG_NAME))
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("keybindings.toml")))
    }
}

/// Quote `s` as a single POSIX shell token using single quotes, escaping any
/// embedded single quote as `'\''`. The result is inert under `sh -c`: nothing
/// inside is expanded, so it is safe to interpolate untrusted text into a
/// command line.
fn sh_single_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::{
        EditorBehaviorSettings, GutterSettings, PanelSettings, SaveSettings,
        SecondaryPanelSettings, Settings, SubsystemSettings, TypingSettings, ViewportSettings,
        sh_single_quote,
    };

    #[test]
    fn t124_ai_provider_defaults_to_cli_with_no_http_config() {
        let s = Settings::default();
        assert_eq!(s.ai_provider, "cli");
        assert_eq!(s.ai_endpoint, "");
        assert_eq!(s.ai_model, "");
        assert_eq!(s.ai_api_key_command, "");
    }

    #[test]
    fn default_ai_command_single_quotes_prompt_and_stdin_file() {
        let s = Settings::default();
        assert_eq!(
            s.ai_command_line("Summarize this text.", "/tmp/in.txt"),
            "claude -p 'Summarize this text.' < '/tmp/in.txt'"
        );
    }

    #[test]
    fn custom_ai_command_with_file_placeholder_substitutes_quoted_path() {
        let s = Settings {
            ai_command: "codex exec {prompt} {file}".to_string(),
            ..Settings::default()
        };
        assert_eq!(
            s.ai_command_line("Explain this text.", "/tmp/in.txt"),
            "codex exec 'Explain this text.' '/tmp/in.txt'"
        );
    }

    #[test]
    fn empty_ai_command_falls_back_to_default() {
        let s = Settings {
            ai_command: "   ".to_string(),
            ..Settings::default()
        };
        assert_eq!(
            s.ai_command_line("Define this text.", "/tmp/in.txt"),
            "claude -p 'Define this text.' < '/tmp/in.txt'"
        );
    }

    #[test]
    fn prompt_cannot_inject_shell_commands() {
        let s = Settings::default();
        // A chat message packed with shell metacharacters must remain a single,
        // inert argument — no unescaped `$(`, backtick, `;`, or `"` breakout.
        let evil = "\"; rm -rf ~; echo $(id) `whoami`";
        let cmd = s.ai_command_line(evil, "/tmp/in.txt");
        // The prompt is fully enclosed in one single-quoted span.
        assert!(cmd.starts_with("claude -p '"), "{cmd}");
        // No command-substitution or statement separators survive OUTSIDE quotes:
        // the only single quotes are the delimiters we added plus the escaped
        // form `'\''` for the literal `"` there isn't; verify metachars are quoted.
        assert!(cmd.contains("'\"; rm -rf ~; echo $(id) `whoami`'"), "{cmd}");
    }

    #[test]
    fn sh_single_quote_escapes_embedded_single_quotes() {
        assert_eq!(sh_single_quote("a'b"), "'a'\\''b'");
        assert_eq!(sh_single_quote("plain"), "'plain'");
        assert_eq!(sh_single_quote("$(x)`y`"), "'$(x)`y`'");
    }

    #[test]
    #[cfg(unix)]
    fn save_to_narrows_the_file_to_owner_only() {
        // config.toml can carry sensitive content (e.g. a custom ai_command
        // template with an embedded API key, T133) -- an explicit-path save
        // must come back owner-only regardless of umask.
        use std::os::unix::fs::PermissionsExt as _;
        let path =
            std::env::temp_dir().join(format!("vix-settings-mode-{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&path);
        Settings::default().save_to(&path).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        std::fs::remove_file(&path).ok();
    }

    /// T149: the whole point of `#[serde(flatten)]`-ing the bool groups
    /// instead of e.g. nesting them under real TOML tables (`[gutter]`,
    /// `[panels]`, …) is that `config.toml`'s on-disk shape doesn't change
    /// at all — every setting is still one flat top-level key, exactly as
    /// if `Settings` still declared it directly. A config file written by
    /// the *old*, pre-T149 schema (flat keys, no group nesting) must still
    /// load correctly, with the overridden keys taking effect and every
    /// other key (bool or not) falling back to its default via
    /// `#[serde(default)]` — including keys that now live in a different
    /// Rust struct than the top-level one.
    #[test]
    fn old_flat_config_format_still_loads_correctly() {
        let path = std::env::temp_dir().join(format!(
            "vix-settings-old-format-{}.toml",
            std::process::id()
        ));
        std::fs::write(
            &path,
            r#"
line_numbers = false
show_welcome_dialog = false
ai_diff_review = false
modal_engine = false
theme = "light"
"#,
        )
        .unwrap();
        let s = Settings::load_from(&path);
        std::fs::remove_file(&path).ok();

        // The five keys present in the old-format file took effect...
        assert!(!s.gutter.line_numbers, "line_numbers override applied");
        assert!(
            !s.startup.show_welcome_dialog,
            "show_welcome_dialog override applied"
        );
        assert!(!s.misc.ai_diff_review, "ai_diff_review override applied");
        assert!(!s.subsystems.modal_engine, "modal_engine override applied");
        assert_eq!(s.theme, "light");
        // ...and every key the old file didn't mention -- in every group,
        // not just the one two keys above happened to touch -- still
        // resolves to its real default, proving `#[serde(default)]`
        // reaches through the flatten boundary correctly.
        assert_eq!(
            s.gutter.show_whitespace,
            GutterSettings::default().show_whitespace
        );
        assert_eq!(
            s.panels.show_explorer,
            PanelSettings::default().show_explorer
        );
        assert_eq!(
            s.secondary_panels.show_status_bar,
            SecondaryPanelSettings::default().show_status_bar
        );
        assert_eq!(
            s.viewport.show_scrollbar,
            ViewportSettings::default().show_scrollbar
        );
        assert_eq!(
            s.save.format_on_save,
            SaveSettings::default().format_on_save
        );
        assert_eq!(
            s.editor_behavior.auto_save,
            EditorBehaviorSettings::default().auto_save
        );
        assert_eq!(s.typing.spellcheck, TypingSettings::default().spellcheck);
        assert_eq!(
            s.subsystems.lsp_enabled,
            SubsystemSettings::default().lsp_enabled
        );
    }

    /// A full round trip (`Settings::default()` saved, then reloaded)
    /// preserves every field — sampled across every group plus a few
    /// non-bool fields, not just the ones the test above happened to
    /// override.
    #[test]
    fn default_settings_round_trip_through_toml_is_lossless() {
        let path = std::env::temp_dir().join(format!(
            "vix-settings-round-trip-{}.toml",
            std::process::id()
        ));
        let original = Settings::default();
        original.save_to(&path).unwrap();
        let reloaded = Settings::load_from(&path);
        std::fs::remove_file(&path).ok();

        assert_eq!(reloaded.gutter.line_numbers, original.gutter.line_numbers);
        assert_eq!(
            reloaded.editor_visual.rainbow_brackets,
            original.editor_visual.rainbow_brackets
        );
        assert_eq!(reloaded.panels.show_explorer, original.panels.show_explorer);
        assert_eq!(
            reloaded.secondary_panels.show_breadcrumbs,
            original.secondary_panels.show_breadcrumbs
        );
        assert_eq!(
            reloaded.viewport.show_minimap,
            original.viewport.show_minimap
        );
        assert_eq!(reloaded.misc.preview_tabs, original.misc.preview_tabs);
        assert_eq!(
            reloaded.save.trim_trailing_whitespace,
            original.save.trim_trailing_whitespace
        );
        assert_eq!(
            reloaded.editor_behavior.sticky_scroll,
            original.editor_behavior.sticky_scroll
        );
        assert_eq!(reloaded.typing.auto_pair, original.typing.auto_pair);
        assert_eq!(
            reloaded.startup.restore_session,
            original.startup.restore_session
        );
        assert_eq!(
            reloaded.subsystems.inline_blame,
            original.subsystems.inline_blame
        );
        assert_eq!(reloaded.theme, original.theme);
        assert_eq!(reloaded.tab_width, original.tab_width);
    }
}
