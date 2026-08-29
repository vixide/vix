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

Settings load at startup (`Settings::load()`, called from `main`) and are written
back with `Settings::save()` whenever Vix changes one itself — the welcome dialog
turning `show_welcome_dialog` off, agenda-file edits, recent commands, database
connections, and a final save on exit. Load is forgiving: a missing file, a parse
failure, or any other error falls back to the built-in defaults rather than
refusing to start.

`Settings::load_from(path)` / `Settings::save_to(path)` are the same operations
against an explicit file rather than the user's config directory. The host holds
the choice in `App::settings_path` (`None` = the user's config file, set via
`App::with_settings_path`) and routes every write through it, so a run pointed at
another config file — a test, an embedder — never touches the user's.

Because the schema is `#[serde(default)]`, an older config file still loads
cleanly when new fields are added — each missing field takes its default.

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
| `show_bottom_dock` | boolean | `true` | Show the bottom dock (log/output/data panel). All three docks (left explorer, right messages, bottom) are shown by default. |
| `bottom_dock_height` | integer | `9` | Height in rows of the bottom dock; drag its top edge to resize. |
| `scrollback` | integer | `1000` | Maximum lines retained in the bottom dock (scrollback); the oldest are dropped past this. |
| `preview_tabs` | boolean | `true` | Open single-clicked / arrow-scanned files in an ephemeral preview tab. |
| `trim_trailing_whitespace` | boolean | `true` | On save, strip trailing spaces and tabs from every line. |
| `ensure_final_newline` | boolean | `true` | On save, append a final newline if the file does not end with one. |
| `indent_style` | string | `"spaces"` | Indentation inserted by Tab: `"spaces"` or `"tabs"`. |
| `tab_width` | integer | `4` | Number of spaces per indent when `indent_style` is `"spaces"`. |
| `wrap_column` | integer | `80` | Column that **Edit → Wrap** hard-wraps (fills) the selection or the paragraph at the cursor to. |
| `theme` | string | `"dark"` | Color theme: `"dark"` or `"light"` (or a custom theme, see below). |
| `locale` | string | `"en"` | UI language as a locale code (e.g. `"en"`, `"es"`, `"fr"`, `"de"`, `"cy"`). Used as the default; a `--locale` CLI flag overrides it for one run. |
| `keymap` | string | `"apple"` | Keyboard navigation style id: `"apple"`, `"vscode"`, `"emacs"`, or `"vi"`. |
| `explorer_width` | integer | `30` | Width in columns of the left dock (file explorer); drag its right edge to resize. |
| `messages_width` | integer | `32` | Width in columns of the right dock (message drawer); drag its left edge to resize. |
| `recent_files` | array of strings | `[]` | Recently opened files, most-recent first (absolute paths). Surfaced by **File → Open Recent…**. |
| `recent_files_max` | integer | `15` | How many entries to keep in `recent_files`. |
| `spellcheck` | boolean | `false` | Underline misspelled words in comments and strings. |
| `dictionary_path` | string | `""` | Extra directory to search for Hunspell dictionaries, on top of the autodetected standard locations. Empty = autodetect only. |
| `lsp_enabled` | boolean | `true` | Master switch for Language Server Protocol features (diagnostics, hover, go-to-definition, completion). When off, no servers are launched. |
| `lsp_servers` | array of tables | `[]` | Configured language servers, matched to files by extension (see below). |
| `show_welcome_dialog` | boolean | `true` | Show the welcome dialog on launch. Vix sets it to `false` and saves as soon as the dialog has been shown once, so it appears on the first run only; set it back to `true` to see it again. |
| `contacts_dir` | string | `""` | Directory of vCard (`.vcf`) files for the contact browser (**Tools → Contacts…**). Empty = use the workspace root. |
| `org_capture_templates` | array of tables | `Anything`/`Todo`/`Contact` (see below) | Named, placeholder-driven templates for the **Org → Capture** submenu (see below). |
| `org_priority_highest` | char | `'0'` | Highest-priority character for a headline's `[#X]` cookie (**Org → Priority Up/Down**). "Highest" sorts first — Vix's default is numeric (`'0'` highest .. `'9'` lowest), unlike Emacs's default `'A'`..`'C'`. |
| `org_priority_lowest` | char | `'9'` | Lowest-priority character; see `org_priority_highest`. |
| `org_priority_default` | char | `'0'` | Priority given to a headline that had no `[#X]` cookie yet, by **Org → Priority Up/Down**. |
| `org_agenda_files` | array of strings | `[]` | The Org agenda's explicit file list (workspace-relative paths), managed by **Org → Agenda → File List**. Empty means every `.org` file in the project. |
| `time_zone` | string | `"UTC"` | Active time zone as an IANA canonical name (e.g. `"UTC"`, `"America/New_York"`). Chosen via **Tools → Time Zone…**; used app-wide (e.g. the clock panel). |
| `restore_session` | bool | `true` | Reopen the previous [session](../../vix-session/spec/index.md) (open files, focused tab, cursor positions) when launched in a workspace with no file argument. Saved per workspace in `session.toml`. |
| `sticky_search_highlight` | bool | `true` | Keep [search-match highlights](../../vix-find-panel/spec/index.md) visible after the Find box closes, until toggled off. When `false`, closing Find clears them. |

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

## Org-capture templates

`org_capture_templates` holds the entries for **Org → Capture**: each is a
named template with placeholders and a filing target. See
[`crates/vix-org/spec/index.md`](../../vix-org/spec/index.md) § Capture and
[`crates/vix-org-capture/spec/index.md`](../../../crates/vix-org-capture/spec/index.md) for the
placeholder syntax and target shapes. Each entry is a TOML table:

| Field | Type | Meaning |
| --- | --- | --- |
| `key` | string | Short id shown in the Capture menu/chooser. |
| `description` | string | Human-readable label. |
| `entry_type` | string | `"entry"` (default; a headline), `"plain"`, `"item"`, `"check-item"`, or `"table-line"`. |
| `target` | string | `"cursor"` (default), `"id:<ID>"`, `"file:<path>"`, `"file+headline:<path>#<Headline>"`, or `"file+datetree:<path>"`. |
| `template` | string | The template body — literal text plus placeholders. |
| `prepend` | boolean | Insert at the top of the target instead of the bottom. Default `false`. |
| `empty_lines` | integer | Blank lines to pad before/after the inserted entry. Default `0`. |
| `immediate_finish` | boolean | Skip the review step and file immediately once every prompt is answered. Default `false`. |
| `clock_in` | boolean | Start a `CLOCK:` entry on the newly captured headline. Default `false`. |

The defaults seed three templates (`a` Anything, `t` Todo, `c` Contact, all
targeting the cursor) that match Vix's original fixed capture actions. Add
more as TOML array-of-tables entries, e.g. a journal captured into a date
tree:

```toml
[[org_capture_templates]]
key = "j"
description = "Journal"
target = "file+datetree:journal.org"
template = "* %U %^{Entry}"
```

## Persistence on exit

In-app changes (toggles, dock sizes, the recent-files list, and similar) live in
memory while Vix runs and are written back to the config file on exit
(`on_exit`), which also shuts down any running language servers. A save failure
on exit is non-fatal: it surfaces as a warning message rather than blocking the
quit. Settings are also saved on demand when you open the file via **Vix →
Settings…**.

## As implemented in Vix

`crates/vix-settings/src/lib.rs` defines the `Settings` struct (every field, its doc comment,
and its default), the `LspServer` entry type, the `MAX_RECENT_FILES` cap (`15`),
and the `load`/`save`/`config_path`/`themes_dir`/`indent_string` helpers, all
backed by `confy` under the app name `vix` and config stem `config`.
`src/main.rs` calls `Settings::load()` at startup. In `src/app.rs`,
`open_settings_file` handles the `vix.settings` menu action (save-then-open), and
`on_exit` persists settings (and shuts down LSP) when Vix quits.

[`confy`]: https://crates.io/crates/confy
[TOML]: https://toml.io/
