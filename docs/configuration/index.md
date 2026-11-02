# Configuration

Vix™ stores its settings with [`confy`](https://crates.io/crates/confy) as a TOML
file in the platform configuration directory, under the application name `vix`.

| Platform | Path                                                        |
| -------- | ----------------------------------------------------------- |
| Linux    | `~/.config/vix/config.toml`                                 |
| macOS    | `~/Library/Application Support/rs.vix/config.toml`          |
| Windows  | `%APPDATA%\vix\config\config.toml`                          |

The exact location follows `confy`'s platform conventions. The file is created
with default values on first save (e.g. on quit) and missing keys fall back to
their defaults, so it is safe to delete or hand-edit.

## Settings

| Key             | Type   | Default  | Meaning                                                              |
| --------------- | ------ | -------- | -------------------------------------------------------------------- |
| `line_numbers`  | bool   | `true`   | Show the line-number gutter.                                         |
| `show_whitespace`| bool  | `false`  | Show visible glyphs for space (`·`), tab (`→`), and line ending (`¶`). |
| `soft_wrap`     | bool   | `false`  | Wrap long lines across screen rows instead of scrolling horizontally. |
| `show_explorer` | bool   | `true`   | Show the file-explorer drawer on startup.                            |
| `explorer_delete` | string | `"trash"` | How the explorer's Delete acts: `"trash"` (move to the OS trash/Recycle Bin, undoable there) or `"hard"` (remove outright, no undo). Any other value falls back to `"trash"`. |
| `remote_agent` | string | `"installed"` | Remote (SSH) workspaces: `"installed"` uses a `vix` of this exact version already on the remote machine as a faster file helper; `"upload"` also copies this `vix` there when it has none and the platform matches (SHA-256 checked on the remote side first); `"off"` uses shell commands only. Any other value behaves as `"installed"`. See [Remote editing](../remote-editing/index.md#the-remote-agent). |
| `git_gutter_base` | string | `"head"` | What the editor's change gutter compares against: `"head"` (the last commit), `"index"` (the staged version, so staged hunks stop being marked), `"disk"` (the saved file, so only unsaved edits show) or `"branch:<name>"`. Any other value falls back to `"head"`. Git → Change Gutter Base… sets it. |
| `smart_commit` | string | `"ask"` | What Git → Commit does when nothing is staged but files changed: `"ask"` (confirm, then stage everything with `git add -A`), `"always"` (stage everything without asking) or `"never"` (refuse). Any other value behaves as `"ask"`. |
| `oc_engine` | string | `"auto"` | Which container engine the OC menu drives: `"auto"` (the first of `docker`, `podman`, `nerdctl` found on `PATH`) or one of those names. A named engine that is not installed falls back to the auto order. |
| `oc_mode` | string | `"full"` | `"full"`, or `"read-only"` to let the OC menu list, inspect and show logs only. Any other value behaves as `"read-only"`. |
| `clipboard_provider` | string | `"auto"` | How copy/paste reach the clipboard: `"auto"` (OSC 52 over SSH — `SSH_TTY`/`SSH_CONNECTION` set — or when the system clipboard fails; the system clipboard otherwise), `"system"` (system clipboard only) or `"osc52"` (always write the terminal's OSC 52 escape, wrapped in tmux passthrough under `TMUX`; tmux needs `allow-passthrough on`). Under OSC 52, paste reads Vix's own last copy. Any other value behaves as `"auto"`. |
| `inline_diagnostics` | string | `"cursor-line"` | Which lines show their language-server diagnostic as dimmed text at the end of the line: `"off"`, `"cursor-line"` (the cursor's line only) or `"all"`. Any other value behaves as `"cursor-line"`. The status bar shows the cursor's diagnostic whatever this is set to. |
| `inline_completion` | string | `"off"` | Where dimmed inline ghost-text suggestions come from: `"off"`, `"lsp"` (a language server that offers `textDocument/inlineCompletion`) or `"ai"` (a fill-in-the-middle prompt to the configured AI provider; the text around the cursor is sent to it). Tab accepts, Ctrl+Right accepts one word, Esc dismisses. Any other value behaves as `"off"`. |
| `show_messages` | bool   | `true`   | Show the message drawer on startup.                                  |
| `show_status_bar` | bool | `true`   | Show the bottom status bar.                                          |
| `show_scrollbar` | bool  | `true`   | Show the editor's right-side scroll bar.                             |
| `show_scrollbar_markers` | bool | `true` | Mark the scroll bar where git changes, diagnostics and search matches are. |
| `show_bottom_dock` | bool | `false` | Show the bottom dock (log/output/data panel).                        |
| `bottom_dock_height` | int | `9`   | Height (rows) of the bottom dock; drag its top edge to resize.       |
| `scrollback`    | int    | `1000`   | Maximum lines kept in the bottom dock; the oldest are dropped past this. |
| `terminal_scrollback` | int | `2000` | Lines of scrollback each integrated terminal keeps (`Shift+PgUp` scrolls back). At most `50000`; `0` keeps none. Applies to terminals opened afterwards. See [Integrated Terminal](../terminal/index.md#scrollback). |
| `preview_tabs`  | bool   | `true`   | Open single-clicked / arrow-scanned files in an ephemeral preview tab. |
| `show_welcome_dialog` | bool | `true` | Show the welcome dialog on launch. Vix sets it to `false` and saves once the dialog has been shown, so it appears on the first run only; set it back to `true` to see it again. |
| `indent_style`  | string | `"spaces"`| What Tab inserts: `"spaces"` or `"tabs"`.                           |
| `tab_width`     | int    | `4`      | Number of spaces per indent when `indent_style = "spaces"`.          |
| `wrap_column`   | int    | `80`     | Column that **Edit → Wrap** hard-wraps (fills) text at.              |
| `trim_trailing_whitespace` | bool | `true` | On save, strip trailing spaces/tabs from every line. |
| `ensure_final_newline`     | bool | `true` | On save, append a final newline if the file lacks one. |
| `theme`         | string | `"dark"` | `"dark"`, `"light"`, the `name` of a custom theme, or `"auto"` to follow the terminal's background. |
| `theme_light`   | string | `"light"` | The theme `"auto"` picks on a light terminal.                       |
| `theme_dark`    | string | `"dark"` | The theme `"auto"` picks on a dark terminal (or when it can't tell). |
| `locale`        | string | `"en"`   | UI language code (`en`, `es`, `fr`, `de`, `cy`, …).                  |
| `bidi`          | string | `"auto"` | Reorder right-to-left text (Arabic) for display: `auto`, `on` or `off`. |
| `mirror_layout` | string | `"auto"` | Mirror docks and the scroll bar for right-to-left languages: `auto`, `on` or `off`. |
| `bidi` | string | `"auto"` | Reorder right-to-left text (Arabic) for display: `auto`, `on` or `off`. |
| `keymap`        | string | `"apple"`| Keyboard navigation style: `"apple"`, `"emacs"`, or `"vi"`.         |
| `explorer_width`| int    | `30`     | Width (columns) of the left dock; drag its right edge to resize.    |
| `messages_width`| int    | `32`     | Width (columns) of the right dock; drag its left edge to resize.    |
| `recent_files`  | list   | `[]`     | Recently opened files (absolute paths), most-recent first, capped at 15. Updated automatically; surfaced by **File → Open Recent…**. |
| `spellcheck`    | bool   | `false`  | Underline misspelled words in comments/strings (**View → Editor → Toggle Spellcheck**). |
| `dictionary_path` | string | `""` | Extra directory to search for Hunspell dictionaries, on top of the autodetected standard locations (`/usr/share/hunspell`, `/Library/Spelling`, `~/.local/share/hunspell`, `hunspell -D`, …). Empty = autodetect only. Both `<dir>/<name>.{aff,dic}` and `<dir>/<name>/index.{aff,dic}` layouts work. The spellcheck language follows the UI `locale`. |
| `lsp_enabled`   | bool   | `true`   | Master switch for Language Server Protocol features (diagnostics, hover, go-to-definition, completion). When off, no servers launch. See `crates/vix-lsp/spec/index.md`. |
| `lsp_auto_complete` | bool | `true` | Request LSP completion automatically while typing an identifier or a server trigger character (debounced). Off = only on `Ctrl+Space`. |
| `lsp_servers`   | list   | `[]`     | Language servers, matched to files by extension. Each entry has `language_id`, `extensions`, and `command`. Empty by default — Vix ships no built-in server. |
| `contacts_dir`  | string | `""`     | Directory of vCard (`.vcf`) files for **Tools → Contacts…**. Empty = the workspace root. |
| `ai`            | string | `"on"`   | The AI optional switch. `"off"` hides the AI menu and every AI palette entry and refuses every AI request (the AI menu, the chat panel, the generated commit message, the DB assistant) without a rebuild; nothing is sent anywhere. Any value other than `"on"` counts as `"off"`. |
| `ai_command`    | string | `claude -p "{prompt}"` | Command template the **AI** menu runs. `{prompt}` is replaced with the action's instruction; the input text is fed on stdin (or substituted for `{file}` if the template contains it). Point this at any assistant CLI — `claude`, `codex`, `mistral`, `ollama run …`. Ignored unless `ai_provider` is `"cli"` (the default). |
| `ai_provider`   | string | `"cli"`  | Which backend the AI menu, chat panel, and DB assistant use: `"cli"` (shells out to `ai_command`, no API key needed) or `"anthropic"`/`"openai"`/`"ollama"` (calls that provider's HTTP API directly — see `crates/vix-ai-core/spec/index.md`). An unrecognized value falls back to `"cli"`. |
| `ai_endpoint`   | string | `""`     | HTTP provider endpoint override; empty uses the provider's own default. Ignored when `ai_provider` is `"cli"`. |
| `ai_model`      | string | `""`     | HTTP provider model id override; empty uses the provider's own default. Ignored when `ai_provider` is `"cli"`. |
| `ai_api_key_command` | string | `""` | Command whose stdout is the HTTP provider's API key, tried before the OS keyring (same shape as the DB workbench's `password_command`). Ignored when `ai_provider` is `"cli"`. |
| `ai_agents`     | list   | `[]`     | Agent Client Protocol coding agents (T635): each entry is a command line, e.g. `"gemini --experimental-acp"`, run without a shell. **AI → Start Agent** starts the first one, only in a trusted workspace. See `crates/vix-acp/spec/index.md`. |
| `ai_diff_review`| bool   | `true`   | Review AI replace transforms (Annotate / Improve) as an accept/reject diff before applying, instead of overwriting immediately. |
| `editorconfig`  | bool   | `true`   | Apply `.editorconfig` rules (indent style/size, trim trailing whitespace, final newline) per opened file, overriding the global settings. |
| `auto_pair`     | bool   | `true`   | Auto-insert the matching closer when typing `(` `[` `{` `"` `'` `` ` `` (wrap a selection; step over a closer; Backspace deletes an empty pair). Toggle via **View → Editor → Auto-Pair Brackets**. |
| `inline_blame`  | bool   | `false`  | Show the cursor line's git blame (`author, date · summary`) dimmed at the end of the line. Toggle via **Git → Toggle Inline Blame**. |
| `show_outline_dock` | bool | `false` | Show the code-outline sidebar (symbol list that follows the cursor). Toggle via **View → Layout → Outline Sidebar**. |
| `outline_width` | int    | `28`     | Width (columns) of the outline sidebar. |
| `debug_width`   | int    | `36`     | Width (columns) of the debugger panel. |
| `debug_adapters`| list   | `[]`     | Debug adapters (DAP), matched to files by extension. Each entry has `adapter_id`, `extensions`, `command`, and optional `launch` and `attach` tables (`attach` is used by **Run → Attach**). See `crates/vix-dap/spec/index.md`. |
| `test_command`  | string | `cargo test` | Command run by **Tools → Run Tests**; its output is parsed into the test panel. |
| `test_width`    | int    | `40`     | Width (columns) of the test-results panel. |
| `project_snippets` | string | `<config>/snippets/snippets.json` | Project snippet file, relative to the project root. See `crates/vix-snippets/spec/index.md`. |
| `coverage_path` | string | `""` (unset) | Default path pre-filled in **Tools → Load Coverage File…**'s prompt (an LCOV or Cobertura XML report). See `crates/vix-coverage/spec/index.md`. |
| `relative_line_numbers` | bool | `false` | Number lines relative to the cursor line (the cursor's own line stays absolute). |
| `show_breadcrumbs` | bool | `false` | Show the breadcrumb bar (`file ▸ enclosing symbol`) above the editor. |
| `show_minimap`  | bool   | `false`  | Show the code-overview minimap column at the right of the editor. |
| `sticky_scroll` | bool   | `true`   | Pin the enclosing scope's header line at the top while scrolling. |
| `code_lens`     | bool   | `true`   | Draw code lenses (Run, Debug, references, local Run test) as dimmed rows above their lines. |
| `document_colors` | bool | `true`   | Draw a swatch for each color a language server reports. |
| `rainbow_brackets` | bool | `false` | Color matching brackets by nesting depth. |
| `highlight_word` | bool  | `false`  | Passively highlight every occurrence of the word under the cursor. |
| `sticky_search_highlight` | bool | `true` | Keep search-match highlights after the Find box closes, until toggled off. |
| `show_menu_tooltips` | bool | `true` | Show hover tooltips on menu names and items. |
| `format_on_save` | bool  | `false`  | On save, run the language server's formatter when the file has one. |
| `prettier_languages` | list of strings | `[]` | Languages formatted with your own Prettier by **Format Document** and format-on-save: any of `javascript`, `typescript`, `json`, `css`, `html`, `markdown`, `yaml`, `graphql`, `handlebars`. See [Format with Prettier](../prettier/index.md). |
| `auto_save`     | string | `"off"`  | Save dirty file-backed buffers on their own: `"off"`, `"interval"` (the active buffer every few seconds) or `"focus_lost"` (every buffer Save All could write, when the terminal window loses focus). An old `true`/`false` still loads as `"interval"`/`"off"`. |
| `persistent_undo` | bool | `true`   | Persist each file's undo tree across sessions, restored when the content still matches. |
| `hot_exit` | bool | `true` | Back up unsaved buffers while idle and on quit, and offer them back (with a diff) when the file is next opened. A clean save deletes the backup. See [Hot exit](../undo-store/index.md#hot-exit). |
| `large_file_mb` | int | `50` | Ask before opening a file larger than this many megabytes (`0` never asks; a 1 GiB ceiling always applies). |
| `read_only_paths` | list of strings | Cargo registry/git, `~/.rustup`, `/nix/store`, Homebrew Cellar, `node_modules`, `site-packages` | Path globs that open read-only (`~/` is home, `**` spans directories). **View → Editor → Read-Only** unlocks a tab. |
| `ephemeral_paths` | list of strings | `["**/.git/**"]` | Path globs the session never records, such as `COMMIT_EDITMSG`. |
| `restore_session` | bool | `true`   | Reopen the previous session (files, focus, cursors) when launched in a workspace with no file argument. |
| `recent_files_max` | int | `20`     | How many entries to keep in `recent_files`. |
| `time_zone`     | string | `UTC`    | Active IANA time zone (e.g. `America/New_York`), chosen via **Tools → Time Zone…**. |
| `org_agenda_files` | list of strings | `[]` | The Org agenda's explicit file list (workspace-relative). Empty means every `.org` file in the project. |
| `org_capture_templates` | list of tables | `Anything`, `Todo`, `Contact` | Org-capture templates for **Org → Capture**. See `crates/vix-org-capture/spec/index.md`. |
| `org_priority_highest` | char | `0`  | Highest `[#X]` priority cookie character (Vix defaults to numeric `0`..`9`, unlike Emacs's `A`..`C`). |
| `org_priority_lowest` | char | `9`   | Lowest priority cookie character. |
| `org_priority_default` | char | `0`  | Priority given to a headline that had no cookie yet. |
| `command_recents` | list of strings | `[]` | Action ids recently run from the palette, surfaced at the top of the `>` list. Managed by Vix. |
| `db_connections` | list of tables | `[]` | Saved database connections for the **DB** menu. Passwords are never stored; they are prompted per session. See `crates/vix-db/spec/index.md`. |
| `[im]` | table | see below | The **IM** menu: `nick` (`""` = OS user name), `notify` (`"mentions"`; or `"all"`, `"off"`), `history` (`"session"`; or `"disk"`), `history_days` (`30`), `blocked` (`[]`), and `[[im.accounts]]` IRC accounts (`name`, `server`, `port` = 6697, `tls` = true, `channels`, `sasl_user`). Passwords come only from `VIX_IM_PASSWORD_<NAME>`. See [IM](../im/index.md). |
| `jupyter_confirm_evaluate` | string | `"untrusted"` | When running an Org `src` block in a Jupyter kernel asks first: `"untrusted"` asks before a block's first run in a session unless the workspace is trusted (and always for a file outside the workspace), `"always"` asks before every run, `"never"` never asks. A block's `:eval query` still asks and `:eval never` still refuses. Only in a build with the `jupyter` feature. See [Jupyter Kernels](../jupyter/index.md). |
| `[jupyter_kernels]` | table | `{}` | Which Jupyter kernelspec runs each language, as `language = "kernelspec name"` (for example `python = "python3"`). Used by **Run → Jupyter → Run** when no kernel for the language is running yet; empty picks the first installed kernelspec for the language. Only in a build with the `jupyter` feature. See [Jupyter Kernels](../jupyter/index.md). |

Example `config.toml`:

```toml
line_numbers = true
show_whitespace = false
show_explorer = true
explorer_delete = "trash"
remote_agent = "installed"
git_gutter_base = "head"
smart_commit = "ask"
oc_engine = "auto"
oc_mode = "full"
clipboard_provider = "auto"
inline_diagnostics = "cursor-line"
inline_completion = "off"
show_messages = true
show_status_bar = true
show_scrollbar = true
show_scrollbar_markers = true
show_bottom_dock = false
bottom_dock_height = 9
preview_tabs = true
indent_style = "spaces"
tab_width = 4
wrap_column = 80
trim_trailing_whitespace = true
ensure_final_newline = true
theme = "dark"
locale = "en"
bidi = "auto"
keymap = "apple"
explorer_width = 30
messages_width = 32
lsp_enabled = true

# AI is optional: ai = "off" hides the AI menu and refuses every AI request.
ai = "on"

# The AI menu shells out to this command (text is piped on stdin). Swap in any
# assistant CLI, e.g. ai_command = "codex exec \"{prompt}\""
ai_command = "claude -p \"{prompt}\""

# Or skip the CLI and call a provider's HTTP API directly (see
# crates/vix-ai-core/spec/index.md). The key comes from ai_api_key_command's
# stdout, or the OS keyring (service "vix-ai", account "anthropic") if that's
# empty -- never written here.
# ai_provider = "anthropic"
# ai_model = "claude-sonnet-5"
# ai_api_key_command = "pass show anthropic-api-key"

# Coding agents that speak the Agent Client Protocol (AI -> Start Agent runs the
# first, only in a trusted workspace; see crates/vix-acp/spec/index.md):
# ai_agents = ["gemini --experimental-acp"]

# One [[lsp_servers]] block per language server you have installed:
[[lsp_servers]]
language_id = "rust"
extensions = ["rs"]
command = ["rust-analyzer"]
```

`trim_trailing_whitespace` and `ensure_final_newline` normalize each file when it
is saved (both default on). Note that trimming removes Markdown's trailing
two-space hard line breaks — set `trim_trailing_whitespace = false` if you rely on
them.

Some keys are **managed by Vix rather than hand-edited** — `recent_files`,
`command_recents`, `db_connections`, and the window/dock geometry are written
back as you use the editor.

Most settings are also changed from inside the app and saved on quit: toggling
line numbers / visible whitespace / explorer / messages, resizing a dock (drag
its inner edge), and
choosing a theme (**View → Theme…**), language (**View → Locale…**), or keymap
(**View → Keymap…**).

## Project and language settings

A workspace can carry its own settings in `<root>/.vix/settings.toml`, in the
same format as `config.toml`, naming only the keys it changes. A
`[languages.<id>]` table sets per-language values, in either file, for each
of Vix's language ids (`rust`, `python`, `markdown`, `shell`, `text`, …):

```toml
# <root>/.vix/settings.toml
tab_width = 2
format_on_save = true

[languages.python]
tab_width = 4

[languages.markdown]
soft_wrap = true
trim_trailing_whitespace = false
```

Later layers win: built-in defaults, then your `config.toml`, then the
project file, then the buffer's language table. Tables merge key by key, so
a project's `[languages.python]` adds to yours rather than replacing it;
any other value replaces yours whole.

- **Trust first.** A project file can change which commands Vix runs, so it
  applies only after you trust the workspace, at the same prompt that asks
  about `.vix/scripts/` ("Trust Workspace?"). Until then, or after **N**, it
  is ignored. A remote (SSH) workspace's project file is never read.
- **Per-language keys** are the ones about a single buffer: `indent_style`,
  `tab_width`, `wrap_column`, `trim_trailing_whitespace`,
  `ensure_final_newline`, `format_on_save`, `soft_wrap`, `show_whitespace`,
  `line_numbers`, `relative_line_numbers`, `auto_pair` and
  `rainbow_brackets`. Other keys in a language table are reported and
  ignored. A file's `.editorconfig` still wins for indentation and the
  on-save rules.
- **Unknown keys** in the project file (a typo, or a key from a newer Vix)
  are reported in the message drawer and ignored; a value of the wrong type
  makes Vix ignore the whole project file and say why.
- **Your file stays yours.** Changes made inside Vix are saved to
  `config.toml`, never to the project file, and the project's values are
  never copied into `config.toml`.
- **Live reload.** Saving `.vix/settings.toml` (or `config.toml`) in Vix
  applies it at once, as does **Vix → Reload Settings**.

## Custom themes directory

Custom JSON themes live next to the config file, in a `tui/themes/` subdirectory
(e.g. `~/.config/vix/themes/*.json` on Linux). See [themes.md](../themes/index.md) for the
file format. Set `theme` to a custom theme's `name` to load it on startup.

## Command-line flags

```
vix [FILES]...        Open one or more files; the last is focused.
                      A file may include a position: path:line[:col].
    --locale <CODE>   Use language CODE for this run (overrides `locale`,
                      not saved). e.g. --locale fr
    --help            Print usage.
    --version         Print the version.
```

## See also

- [themes.md](../themes/index.md) — custom theme JSON format.
- [i18n.md](../internationalization/index.md) — languages and how the `locale` value is used.

---

Vix™ and Vix IDE™ are trademarks.
