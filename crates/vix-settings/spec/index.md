# Configuration

User settings, persisted with `confy`. Vix keeps all user preferences in a
single configuration file, persisted with the [`confy`] crate. There is no
separate preferences dialog: the file is plain [TOML], and
**Vix → Settings…** opens it directly in the editor so you can edit it like
any other file.

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

On the Rust side, `Settings`'s ~30 `bool` fields are grouped into small
per-topic sub-structs (`GutterSettings`, `PanelSettings`, `SaveSettings`,
and others — improvement plan T149), each `#[serde(flatten)]`'d back onto
`Settings` — `clippy::struct_excessive_bools` counts bools in any one
struct, so one large group doesn't dodge the lint, only several small ones
do. This is purely a Rust-source organization: `#[serde(flatten)]` keeps
every one of these settings a flat top-level `config.toml` key, exactly as
if it were still declared directly on `Settings` — a config file written
before T149 loads identically, and the table below still lists every
setting as one flat key regardless of which Rust struct it actually lives
on.

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
| `terminal_scrollback` | integer | `2000` | Lines of scrollback each integrated terminal keeps (Shift+PgUp scrolls back; T631). At most `50000` whatever is set; `0` turns it off. Read when a terminal opens. A cell is about 36 bytes, so 2000 lines of 120 columns is roughly 9 MB per terminal. See [`vix-terminal`](../../vix-terminal/spec/index.md#scrollback-t631). |
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
| `lsp_auto_complete` | boolean | `true` | Request LSP completion automatically while typing an identifier or a server trigger character (debounced), instead of only on `Ctrl+Space`. |
| `lsp_servers` | array of tables | `[]` | Configured language servers, matched to files by extension (see below). |
| `debug_adapters` | array of tables | `[]` | Configured debug adapters (DAP), matched to files by extension: `adapter_id`, `extensions`, `command`, and optional `launch` and `attach` string tables (`attach` is what **Run → Attach** sends, T617). See [`crates/vix-dap/spec/index.md`](../../vix-dap/spec/index.md). |
| `show_welcome_dialog` | boolean | `true` | Show the welcome dialog on launch. Vix sets it to `false` and saves as soon as the dialog has been shown once, so it appears on the first run only; set it back to `true` to see it again. |
| `contacts_dir` | string | `""` | Directory of vCard (`.vcf`) files for the contact browser (**Tools → Contacts…**). Empty = use the workspace root. |
| `org_capture_templates` | array of tables | `Anything`/`Todo`/`Contact` (see below) | Named, placeholder-driven templates for the **Org → Capture** submenu (see below). |
| `org_priority_highest` | char | `'0'` | Highest-priority character for a headline's `[#X]` cookie (**Org → Priority Up/Down**). "Highest" sorts first — Vix's default is numeric (`'0'` highest .. `'9'` lowest), unlike Emacs's default `'A'`..`'C'`. |
| `org_priority_lowest` | char | `'9'` | Lowest-priority character; see `org_priority_highest`. |
| `org_priority_default` | char | `'0'` | Priority given to a headline that had no `[#X]` cookie yet, by **Org → Priority Up/Down**. |
| `org_agenda_files` | array of strings | `[]` | The Org agenda's explicit file list (workspace-relative paths), managed by **Org → Agenda → File List**. Empty means every `.org` file in the project. |
| `time_zone` | string | `"UTC"` | Active time zone as an IANA canonical name (e.g. `"UTC"`, `"America/New_York"`). Chosen via **Tools → Time Zone…**; used app-wide (e.g. the clock panel). |
| `restore_session` | bool | `true` | Reopen the previous [session](../../vix-session/spec/index.md) (open files, focused tab, cursor positions) when launched in a workspace with no file argument. Saved per workspace in `session.toml`. |
| `code_lens` | bool | `true` | Draw code lenses (T726) -- a language server's "Run \| Debug \| 3 references" and local "Run test" lenses -- as dimmed rows above their lines. Toggled by **View → Editor → Code Lens** (`view.code_lens`). Held in `LanguageFeatureSettings` (`Settings::language_features`), flattened like every group. |
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

Alongside `config.toml`, Vix looks for a `tui/themes/` directory in the same config
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

## Live reload (T626)

Editing `config.toml` takes effect without a restart. When `App::save` writes
the buffer whose path is the active settings file (`App::settings_path` when
set, else `Settings::config_path()`, compared after canonicalizing), the host
re-reads it with `Settings::try_load_from`. **Vix → Reload Settings**
(`vix.reload_settings`) does the same on demand, without saving a buffer.

- **On error** (e.g. a TOML syntax error) the current in-memory settings are
  kept unchanged and an error message (`msg.settings_reload_failed`) is
  posted, so a half-typed edit never resets everything to defaults.
- **On success** `App::apply_settings(old, new)` installs the new settings and
  re-resolves what was derived from them at startup: the theme (including the
  `theme_light`/`theme_dark` pair), the keymap (an unknown id falls back to
  Apple with the same error as at startup, and a changed keymap resets
  per-keymap modal state), the UI locale (an unknown code is ignored), the
  time zone, and the clipboard provider (T625); then it rebuilds the
  menus (the View → Theme list is re-scanned, so a newly added custom theme
  appears). Each piece is re-applied only when its setting changed, because
  theme, locale and clipboard provider are process-global.

Most other settings are read from `App::settings` at the point of use, so they
take effect immediately too. The editor display flags copied into each tab at
startup (line numbers, soft wrap, whitespace) are not re-applied by a reload;
their View-menu toggles still work.

## Project and language settings (T627)

Settings come in four layers, each overriding the one before it:

1. **built-in** — `Settings::default()`;
2. **user** — the config file above;
3. **project** — `<root>/.vix/settings.toml` (`vix_settings::project_settings_path`),
   in the same format as the user file, holding only the keys the project
   wants to change;
4. **language** — a `languages` table of partial settings keyed by Vix's
   language id (the one syntax highlighting uses: `rust`, `python`,
   `markdown`, `shell`, `text`, …), resolved per buffer.

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

**Merging.** The project file is deep-merged onto the user settings
(`vix_settings::merge_layer`): a table merges key by key, recursively
(`[im]`, `[languages.rust]`), and any other value — a string, a number, an
array — replaces the user's value whole. The merged table must still
deserialize as `Settings`; when it doesn't (a wrong type, say), the project
file is ignored and the error is reported.

**Trust.** A project file changes what Vix runs (`ai_command`,
`test_command`, `lsp_servers`, …), so it is read only when the workspace is
trusted: the same persisted T132 decision (`WorkspaceSession::
scripts_trusted`) that loads `.vix/scripts/`. The trust prompt is asked
once for a workspace that has project scripts **or** a project settings
file, in every build (it no longer needs the `script` capability); trusting
applies the file at once. An untrusted or undecided workspace ignores the
file. A remote (SSH) workspace never reads one, since the file is on the
remote machine and T714's remote trust covers only project commands.

**Unknown keys.** `vix_settings::unknown_keys` lists the project file's
dotted keys that `Settings` doesn't define (a typo, or a key from a newer
Vix) — top-level keys, and keys inside a known non-empty table such as
`[im]`; each is reported as a warning (`msg.project_settings_unknown_key`)
and otherwise ignored. `vix_settings::language_key_problems` lists keys in
a `languages` table that are not per-language settings; each is reported
(`msg.settings_language_key_ignored`) and ignored.

**Per-language keys** (`vix_settings::LANGUAGE_KEYS`) are the ones that
describe a buffer: `indent_style`, `tab_width`, `wrap_column`,
`trim_trailing_whitespace`, `ensure_final_newline`, `format_on_save`,
`soft_wrap`, `show_whitespace`, `line_numbers`, `relative_line_numbers`,
`auto_pair` and `rainbow_brackets`. `Settings::for_language(lang)` returns
the settings with that language's table applied (borrowed unchanged when
there is none); the host uses it when a buffer opens (indent, display
flags), when it is saved (trim/final newline, format on save and its tab
size) and for **Edit → Wrap**. A buffer's `.editorconfig` still overrides
indent and the on-save rules, as before. `languages` tables in the user file
work the same way, and the project's `[languages.x]` merges key by key onto
the user's.

**Saving.** In-app changes are still written to the user file, never the
project file, and the project's values are never copied into the user
file: `vix_settings::unmerge(user, merged, effective)` starts from the user
settings as loaded and takes from the live settings only the top-level keys
that changed since the project file was merged.

**Live reload.** Saving `<root>/.vix/settings.toml` re-reads both files, as
saving the user file does (T626), and so do **Vix → Reload Settings** and
switching workspaces. A load error in the project file keeps the user
settings and reports it.

## As implemented in Vix

`crates/vix-settings/src/lib.rs` defines the `Settings` struct (every field, its doc comment,
and its default), its 11 `#[serde(flatten)]`'d bool-group sub-structs (T149:
`GutterSettings`, `EditorVisualSettings`, `PanelSettings`,
`SecondaryPanelSettings`, `ViewportSettings`, `MiscSettings`, `SaveSettings`,
`EditorBehaviorSettings`, `TypingSettings`, `StartupSettings`,
`SubsystemSettings`), the `LspServer` entry type, the `MAX_RECENT_FILES` cap
(`15`), and the `load`/`save`/`config_path`/`themes_dir`/`indent_string`
helpers, all backed by `confy` under the app name `vix` and config stem
`config`.
`tui/src/main.rs` calls `Settings::load()` at startup. In `tui/src/app.rs`,
`open_settings_file` handles the `vix.settings` menu action (save-then-open),
`reload_settings_from_disk` and `apply_settings` (in `tui/src/app/session.rs`)
implement live reload, and
`on_exit` persists settings (and shuts down LSP) when Vix quits.

[`confy`]: https://crates.io/crates/confy
[TOML]: https://toml.io/

## Capability data types (T584)

`vix-settings` owns the plain serde types for the sections whose behavior lives
elsewhere: `Connection` and `Kind` (DB, with their small pure helpers),
`DebugAdapter` (DAP), and `CaptureTemplate`, `EntryType` and
`default_capture_templates` (org-capture). `vix-db`, `vix-dap` and
`vix-org-capture` depend on `vix-settings` and re-export them, so existing paths
(`vix_db::connect::Connection`) still work. The direction matters: `Settings`
must round-trip every section in every build, because `Settings::save` writes
back only the fields it knows — gating a field behind a capability would silently
delete the user's config the first time a build without that capability saved.
A test loads a config with every section and re-saves it unchanged.
