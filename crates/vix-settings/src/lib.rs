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
// Independent persisted preferences; each maps to one flat TOML key. Grouping
// them would break the on-disk format and only relocate the lint.
#[allow(clippy::struct_excessive_bools)]
pub struct Settings {
    /// Show the line-number gutter.
    pub line_numbers: bool,
    /// Show line numbers relative to the cursor line (hybrid: cursor line absolute).
    pub relative_line_numbers: bool,
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
    /// Show the breadcrumb bar (file ▸ enclosing symbol) above the editor.
    pub show_breadcrumbs: bool,
    /// Show the code-outline sidebar (symbol list that follows the cursor).
    pub show_outline_dock: bool,
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
    /// On save, run the language server's formatter (when the file has one).
    pub format_on_save: bool,
    /// Periodically save the active dirty file-backed buffer (every few seconds).
    pub auto_save: bool,
    /// Pin the enclosing scope's header line at the top of the editor while
    /// scrolling (sticky scroll).
    pub sticky_scroll: bool,
    /// Color matching brackets by nesting depth (rainbow brackets).
    pub rainbow_brackets: bool,
    /// Persist each file's undo tree across sessions (restored on reopen when the
    /// file content still matches).
    pub persistent_undo: bool,
    /// Show a code-overview minimap column at the right of the editor.
    pub show_minimap: bool,
    /// Show hover tooltips (help text) on the menu bar's menus and items.
    pub show_menu_tooltips: bool,
    /// Passively highlight every occurrence of the word under the cursor.
    pub highlight_word: bool,
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
    /// Whether the first-run welcome screen has been shown. Set true after the
    /// welcome panel first appears, so it does not pop up on every launch.
    pub welcomed: bool,
    /// Directory of vCard (`.vcf`) files for the contact browser (Tools →
    /// Contacts…). Empty = use the workspace root.
    pub contacts_dir: String,
    /// Active time zone as an IANA canonical name (e.g. `"UTC"`,
    /// `"America/New_York"`). Chosen via Tools → Time Zone…; used app-wide
    /// (e.g. the clock panel).
    pub time_zone: String,
    /// Reopen the previous session (open files, focused tab, cursor positions)
    /// when Vix is launched in a workspace with no file given on the command
    /// line. The session is saved per workspace root in `session.toml`.
    pub restore_session: bool,
    /// Keep search-match highlights visible after the Find box closes ("sticky"
    /// highlights), until they are explicitly toggled off. When false, closing
    /// Find clears the highlights.
    pub sticky_search_highlight: bool,
    /// Command template the **AI** menu runs over editor text. The placeholder
    /// `{prompt}` is replaced with the action's instruction; if the template also
    /// contains `{file}` it is replaced with the path of a temp file holding the
    /// input text, otherwise that file is redirected to the command's stdin. This
    /// lets you point the AI menu at any CLI assistant — `claude` (default),
    /// `codex`, `mistral`, `ollama run …`, etc. See [`Settings::ai_command_line`].
    pub ai_command: String,
    /// Review AI replace transforms (Annotate / Improve) as an accept/reject diff
    /// before applying, instead of overwriting the text immediately. On by default.
    pub ai_diff_review: bool,
    /// Apply `.editorconfig` rules (indent style/size, trim/final-newline on save)
    /// for opened files, overriding the global settings per file. On by default.
    pub editorconfig: bool,
    /// Auto-insert the matching closer when an opening bracket/quote is typed (and
    /// delete both with Backspace inside an empty pair). On by default.
    pub auto_pair: bool,
    /// Show the git blame for the cursor's line inline (dimmed, end of line). Off
    /// by default; toggle via **Git → Toggle Inline Blame**.
    pub inline_blame: bool,
    /// Configured debug adapters (DAP), matched to files by extension. Empty by
    /// default — add the adapters you have installed.
    pub debug_adapters: Vec<vix_dap::DebugAdapter>,
    /// Command run by **Tools → Run Tests**, whose output is parsed into a
    /// pass/fail tree (e.g. `cargo test`, `pytest -v`, `npm test`).
    pub test_command: String,
    /// Project snippet file, relative to the project root. Loaded alongside the
    /// global and media-type snippet files. See the `vix-snippets` crate spec.
    pub project_snippets: String,
    /// Width (columns) of the test-results panel.
    pub test_width: u16,
    /// Saved database connections for the **DB** menu (the `vix-db` crate spec). Passwords
    /// are never stored here; they are prompted for per session.
    pub db_connections: Vec<vix_db::connect::Connection>,
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
            relative_line_numbers: false,
            show_whitespace: false,
            soft_wrap: false,
            show_explorer: true,
            show_messages: true,
            show_status_bar: true,
            show_breadcrumbs: false,
            show_outline_dock: false,
            show_scrollbar: true,
            show_bottom_dock: false,
            bottom_dock_height: 9,
            scrollback: 1000,
            preview_tabs: true,
            trim_trailing_whitespace: true,
            ensure_final_newline: true,
            format_on_save: false,
            auto_save: false,
            sticky_scroll: true,
            rainbow_brackets: false,
            persistent_undo: true,
            show_minimap: false,
            show_menu_tooltips: true,
            highlight_word: false,
            indent_style: "spaces".to_string(),
            tab_width: 4,
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
            spellcheck: false,
            dictionary_path: String::new(),
            lsp_enabled: true,
            lsp_servers: Vec::new(),
            welcomed: false,
            contacts_dir: String::new(),
            time_zone: "UTC".to_string(),
            restore_session: true,
            sticky_search_highlight: true,
            // Placeholders are single-quoted by `ai_command_line`, so the
            // template must NOT add quotes of its own (doing so would let chat
            // text break out and inject shell commands).
            ai_command: "claude -p {prompt}".to_string(),
            ai_diff_review: true,
            editorconfig: true,
            auto_pair: true,
            inline_blame: false,
            debug_adapters: Vec::new(),
            test_command: "cargo test".to_string(),
            test_width: 40,
            project_snippets: "config/snippets/snippets.json".to_string(),
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

    /// Persist settings to the user's config directory.
    ///
    /// # Errors
    ///
    /// Returns a [`confy::ConfyError`] if the config directory cannot be
    /// created or the file cannot be written/serialized.
    pub fn save(&self) -> Result<(), confy::ConfyError> {
        confy::store(APP_NAME, Some(CONFIG_NAME), self)
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
    use super::{Settings, sh_single_quote};

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
}
