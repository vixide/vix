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
| `show_explorer` | bool   | `true`   | Show the file-explorer drawer on startup.                            |
| `show_messages` | bool   | `true`   | Show the message drawer on startup.                                  |
| `preview_tabs`  | bool   | `true`   | Open single-clicked / arrow-scanned files in an ephemeral preview tab. |
| `theme`         | string | `"dark"` | `"dark"`, `"light"`, or the `name` of a custom theme.                |
| `locale`        | string | `"en"`   | UI language code (`en`, `es`, `fr`, `de`, `cy`, …).                  |
| `keyway`        | string | `"apple"`| Keyboard navigation style: `"apple"`, `"emacs"`, or `"vim"`.         |
| `explorer_width`| int    | `30`     | Width (columns) of the left dock; drag its right edge to resize.    |
| `messages_width`| int    | `32`     | Width (columns) of the right dock; drag its left edge to resize.    |

Example `config.toml`:

```toml
line_numbers = true
show_whitespace = false
show_explorer = true
show_messages = true
preview_tabs = true
theme = "dark"
locale = "en"
keyway = "apple"
explorer_width = 30
messages_width = 32
```

Most settings are also changed from inside the app and saved on quit: toggling
line numbers / visible whitespace / explorer / messages, resizing a dock (drag
its inner edge), and
choosing a theme (**View → Theme…**), language (**View → Locale…**), or keyway
(**View → Keyway…**).

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
