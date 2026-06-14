# Configuration

Vix keeps all user preferences in a single configuration file, persisted with the
[`confy`] crate. There is no separate preferences dialog: the file is plain
[TOML], and **Vix → Settings…** opens it directly in the editor so you can edit
it like any other file.

## Where the config file lives

`confy` chooses the right per-OS location under the application name `vix`, with
the file stem `config`, so the on-disk file is `config.toml`. On Linux this is
typically:

```
~/.config/vix/config.toml
```

macOS and Windows use their own platform configuration directories; the exact
path is whatever `confy` resolves for the platform. The code exposes this path
via `Settings::config_path()`, which returns `None` only when the config
location cannot be determined.

Settings load at startup (`Settings::load()`, called from `main`). Load is
forgiving: a missing file, a parse failure, or any other error falls back to the
built-in defaults rather than refusing to start. Because the schema is
`#[serde(default)]`, an older config file still loads cleanly when new fields are
added — each missing field takes its default.

## Editing settings

Choose **Vix → Settings…** to open the config file. Vix first writes the current
in-memory settings to disk (so the file exists and reflects any in-app changes),
then opens it as an editor tab focused for editing. Edit the TOML, save the file,
and the values apply on the next load. If the config location cannot be
determined, Vix shows a status message instead of opening anything.

## Settings reference

Every setting has a default, so a config file may specify as many or as few as
you like. Types are TOML types; the defaults below are the values used when a
field is absent.

| Setting | Type | Default | Meaning |
| --- | --- | --- | --- |
| `line_numbers` | boolean | `true` | Show the line-number gutter. |
| `show_whitespace` | boolean | `false` | Render visible glyphs for whitespace (space, tab, line ending). |
| `soft_wrap` | boolean | `false` | Wrap long lines across screen rows instead of scrolling horizontally. |
| `show_explorer` | boolean | `true` | Show the file explorer on startup. |
| `show_messages` | boolean | `true` | Show the message drawer on startup. |
| `show_status_bar` | boolean | `true` | Show the bottom status bar. |
| `show_scrollbar` | boolean | `true` | Show the editor's right-side scroll bar. |
| `show_bottom_dock` | boolean | `false` | Show the bottom dock (log/output/data panel). |
| `bottom_dock_height` | integer | `9` | Height in rows of the bottom dock; drag its top edge to resize. |
| `scrollback` | integer | `1000` | Maximum lines retained in the bottom dock (scrollback); the oldest are dropped past this. |
| `preview_tabs` | boolean | `true` | Open single-clicked / arrow-scanned files in an ephemeral preview tab. |
| `trim_trailing_whitespace` | boolean | `true` | On save, strip trailing spaces and tabs from every line. |
| `ensure_final_newline` | boolean | `true` | On save, append a final newline if the file does not end with one. |
| `indent_style` | string | `"spaces"` | Indentation inserted by Tab: `"spaces"` or `"tabs"`. |
| `tab_width` | integer | `4` | Number of spaces per indent when `indent_style` is `"spaces"`. |
| `theme` | string | `"dark"` | Color theme: `"dark"` or `"light"` (or a custom theme, see below). |
| `locale` | string | `"en"` | UI language as a locale code (e.g. `"en"`, `"es"`, `"fr"`, `"de"`, `"cy"`). Used as the default; a `--locale` CLI flag overrides it for one run. |
| `keymap` | string | `"apple"` | Keyboard navigation style id: `"apple"`, `"vscode"`, `"emacs"`, or `"vim"`. |
| `explorer_width` | integer | `30` | Width in columns of the left dock (file explorer); drag its right edge to resize. |
| `messages_width` | integer | `32` | Width in columns of the right dock (message drawer); drag its left edge to resize. |
| `recent_files` | array of strings | `[]` | Recently opened files, most-recent first (absolute paths). Surfaced by **File → Open Recent…**. |
| `recent_files_max` | integer | `15` | How many entries to keep in `recent_files`. |
| `spellcheck` | boolean | `false` | Underline misspelled words in comments and strings. |
| `dictionary_path` | string | `""` | Extra directory to search for Hunspell dictionaries, on top of the autodetected standard locations. Empty = autodetect only. |
| `lsp_enabled` | boolean | `true` | Master switch for Language Server Protocol features (diagnostics, hover, go-to-definition, completion). When off, no servers are launched. |
| `lsp_servers` | array of tables | `[]` | Configured language servers, matched to files by extension (see below). |
| `welcomed` | boolean | `false` | Whether the first-run welcome screen has been shown. Set true after the welcome panel first appears, so it does not pop up on every launch. |
| `contacts_dir` | string | `""` | Directory of vCard (`.vcf`) files for the contact browser (**Tools → Contacts…**). Empty = use the workspace root. |
| `time_zone` | string | `"UTC"` | Active time zone as an IANA canonical name (e.g. `"UTC"`, `"America/New_York"`). Chosen via **Tools → Time Zone…**; used app-wide (e.g. the clock panel). |

### Indentation

The string Tab inserts is derived from two settings: with `indent_style = "tabs"`
Vix inserts a tab character; otherwise it inserts `tab_width` spaces. A
`tab_width` of `0` falls back to a single space.

### Recent files

`recent_files` is maintained automatically as you open files; it is capped to
`recent_files_max` entries (default `15`). You normally do not edit it by hand —
it is what populates **File → Open Recent…**.

## The themes directory

Alongside `config.toml`, Vix looks for a `themes/` directory in the same config
folder (e.g. `~/.config/vix/themes/`), exposed via `Settings::themes_dir()`.
Drop custom JSON theme files there, then set `theme` to the theme's name to use
it, in addition to the built-in `"dark"` and `"light"` themes.

## LSP server configuration

Vix ships no built-in language server. To get diagnostics, hover,
go-to-definition, and completion, keep `lsp_enabled = true` and add the servers
you have installed to `lsp_servers`. Each entry is a TOML table with three keys:

| Field | Type | Meaning |
| --- | --- | --- |
| `language_id` | string | The LSP `languageId` sent in `didOpen` (e.g. `"rust"`, `"python"`). |
| `extensions` | array of strings | File extensions, without the leading dot, that this server handles (e.g. `["rs"]`). |
| `command` | array of strings | Launch command: program first, then arguments (e.g. `["rust-analyzer"]`). |

A Rust example:

```toml
lsp_enabled = true

[[lsp_servers]]
language_id = "rust"
extensions = ["rs"]
command = ["rust-analyzer"]
```

Servers are matched to a file by its extension. With `lsp_enabled = false`, no
servers are launched regardless of what `lsp_servers` contains.

## Persistence on exit

In-app changes (toggles, dock sizes, the recent-files list, and similar) live in
memory while Vix runs and are written back to the config file on exit
(`on_exit`), which also shuts down any running language servers. A save failure
on exit is non-fatal: it surfaces as a warning message rather than blocking the
quit. Settings are also saved on demand when you open the file via **Vix →
Settings…**.

## As implemented in Vix

`src/settings.rs` defines the `Settings` struct (every field, its doc comment,
and its default), the `LspServer` entry type, the `MAX_RECENT_FILES` cap (`15`),
and the `load`/`save`/`config_path`/`themes_dir`/`indent_string` helpers, all
backed by `confy` under the app name `vix` and config stem `config`.
`src/main.rs` calls `Settings::load()` at startup. In `src/app.rs`,
`open_settings_file` handles the `vix.settings` menu action (save-then-open), and
`on_exit` persists settings (and shuts down LSP) when Vix quits.

[`confy`]: https://crates.io/crates/confy
[TOML]: https://toml.io/
