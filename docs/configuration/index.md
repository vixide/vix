# Configuration

Vix stores its settings with [`confy`](https://crates.io/crates/confy) as a TOML
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
| `show_messages` | bool   | `true`   | Show the message drawer on startup.                                  |
| `show_status_bar` | bool | `true`   | Show the bottom status bar.                                          |
| `show_scrollbar` | bool  | `true`   | Show the editor's right-side scroll bar.                             |
| `show_bottom_dock` | bool | `false` | Show the bottom dock (log/output/data panel).                        |
| `bottom_dock_height` | int | `9`   | Height (rows) of the bottom dock; drag its top edge to resize.       |
| `scrollback`    | int    | `1000`   | Maximum lines kept in the bottom dock; the oldest are dropped past this. |
| `preview_tabs`  | bool   | `true`   | Open single-clicked / arrow-scanned files in an ephemeral preview tab. |
| `indent_style`  | string | `"spaces"`| What Tab inserts: `"spaces"` or `"tabs"`.                           |
| `tab_width`     | int    | `4`      | Number of spaces per indent when `indent_style = "spaces"`.          |
| `trim_trailing_whitespace` | bool | `true` | On save, strip trailing spaces/tabs from every line. |
| `ensure_final_newline`     | bool | `true` | On save, append a final newline if the file lacks one. |
| `theme`         | string | `"dark"` | `"dark"`, `"light"`, or the `name` of a custom theme.                |
| `locale`        | string | `"en"`   | UI language code (`en`, `es`, `fr`, `de`, `cy`, …).                  |
| `keymap`        | string | `"apple"`| Keyboard navigation style: `"apple"`, `"emacs"`, or `"vim"`.         |
| `explorer_width`| int    | `30`     | Width (columns) of the left dock; drag its right edge to resize.    |
| `messages_width`| int    | `32`     | Width (columns) of the right dock; drag its left edge to resize.    |
| `recent_files`  | list   | `[]`     | Recently opened files (absolute paths), most-recent first, capped at 15. Updated automatically; surfaced by **File → Open Recent…**. |
| `spellcheck`    | bool   | `false`  | Underline misspelled words in comments/strings (**View → Editor → Toggle Spellcheck**). |
| `dictionary_path` | string | `""` | Extra directory to search for Hunspell dictionaries, on top of the autodetected standard locations (`/usr/share/hunspell`, `/Library/Spelling`, `~/.local/share/hunspell`, `hunspell -D`, …). Empty = autodetect only. Both `<dir>/<name>.{aff,dic}` and `<dir>/<name>/index.{aff,dic}` layouts work. The spellcheck language follows the UI `locale`. |
| `lsp_enabled`   | bool   | `true`   | Master switch for Language Server Protocol features (diagnostics, hover, go-to-definition, completion). When off, no servers launch. See `spec/lsp.md`. |
| `lsp_servers`   | list   | `[]`     | Language servers, matched to files by extension. Each entry has `language_id`, `extensions`, and `command`. Empty by default — Vix ships no built-in server. |
| `contacts_dir`  | string | `""`     | Directory of vCard (`.vcf`) files for **Tools → Contacts…**. Empty = the workspace root. |
| `ai_command`    | string | `claude -p "{prompt}"` | Command template the **AI** menu runs. `{prompt}` is replaced with the action's instruction; the input text is fed on stdin (or substituted for `{file}` if the template contains it). Point this at any assistant CLI — `claude`, `codex`, `mistral`, `ollama run …`. |
| `ai_diff_review`| bool   | `true`   | Review AI replace transforms (Annotate / Improve) as an accept/reject diff before applying, instead of overwriting immediately. |
| `editorconfig`  | bool   | `true`   | Apply `.editorconfig` rules (indent style/size, trim trailing whitespace, final newline) per opened file, overriding the global settings. |
| `auto_pair`     | bool   | `true`   | Auto-insert the matching closer when typing `(` `[` `{` `"` `'` `` ` `` (wrap a selection; step over a closer; Backspace deletes an empty pair). Toggle via **View → Editor → Auto-Pair Brackets**. |

Example `config.toml`:

```toml
line_numbers = true
show_whitespace = false
show_explorer = true
show_messages = true
show_status_bar = true
show_scrollbar = true
show_bottom_dock = false
bottom_dock_height = 9
preview_tabs = true
indent_style = "spaces"
tab_width = 4
trim_trailing_whitespace = true
ensure_final_newline = true
theme = "dark"
locale = "en"
keymap = "apple"
explorer_width = 30
messages_width = 32
lsp_enabled = true

# The AI menu shells out to this command (text is piped on stdin). Swap in any
# assistant CLI, e.g. ai_command = "codex exec \"{prompt}\""
ai_command = "claude -p \"{prompt}\""

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

Most settings are also changed from inside the app and saved on quit: toggling
line numbers / visible whitespace / explorer / messages, resizing a dock (drag
its inner edge), and
choosing a theme (**View → Theme…**), language (**View → Locale…**), or keymap
(**View → Keymap…**).

## Custom themes directory

Custom JSON themes live next to the config file, in a `themes/` subdirectory
(e.g. `~/.config/vix/themes/*.json` on Linux). See [themes.md](themes.md) for the
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

- [themes.md](themes.md) — custom theme JSON format.
- [i18n.md](i18n.md) — languages and how the `locale` value is used.
