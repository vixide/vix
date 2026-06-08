# STRIDE — Simple Terminal Rust IDE

STRIDE is a keyboard-friendly terminal text editor written in Rust. It opens
text files, edits them, and saves them — with a menu bar, a file-explorer drawer,
tabbed buffers, a message drawer, a command palette, find & replace, and a
calendar box. It is built on [`ratatui`] and [`ratatui-code-editor`], and uses
familiar macOS/Windows shortcuts (Ctrl+C / Ctrl+V, not modal editing). Mouse and
keyboard both work.

```
 File  Edit  Tools  Help                                       STRIDE
╭─ Explorer ─────────╮╭ main.rs │ spec.md ────────────────────╮╭─ Messages ─────╮
│  src               ││╭──────────────────────────────────────╮││ Welcome to     │
│   app.rs           │││ 1  fn main() {                       △│││ STRIDE…      x │
│   editor.rs        │││ 2      println!("hi");               ┃│││ Ctrl+B toggles │
│   main.rs          │││ 3  }                                 ┃│││ the explorer x │
│  spec              │││ 4                                    ▽││╰────────────────╯
│   spec.md          ││╰──────────────────────────────────────╯│
╰────────────────────╯╰─────────────────────────────────────────╯
 src/main.rs — Saved                              Ln 2:Col 5    
```

## Features

- **Menu bar** — File / Edit / Tools / Help, navigable entirely by keyboard
  (`F10` or `Alt+F/E/T/H`).
- **Tabbed editor** — each file is a tab; a full multiline editor with
  Tree-sitter syntax highlighting, undo/redo, selection, system clipboard, line
  numbers (toggleable), and a right-side scrollbar.
- **Mouse support** — click to place the cursor, drag to select, wheel to scroll;
  click tabs, files, messages, and menus.
- **Image viewing** — open a PNG/JPG/GIF/… and it renders in a read-only image
  tab (via `ratatui-image`, on a graphics-capable terminal).
- **File explorer ops** — copy/cut/paste (`Ctrl+C`/`X`/`V`) with conflict
  prompts, `Shift+Up/Down` multi-select, and `Delete`; open buffers follow file
  moves and close on delete.
- **Keyboard help** — press `F1` for an overlay of every shortcut.
- **File explorer** — a directory tree in the left drawer; arrow-scan opens files
  in an ephemeral *preview* tab so you can browse without piling up tabs.
- **Command palette** (`Ctrl+P`) — four modes via prefix: file finder, `>`
  commands, `#` buffers, `:` go-to-line. Space-separated fuzzy matching.
- **Find & Replace** — incremental search, `F3`/`Shift+F3` navigation, Case /
  Whole-Word / Regex toggles, capture groups (`$1`, `${name}`) and escapes.
- **Message drawer** — advice and notifications, each individually dismissable.
- **Calendar box** — local clock, UTC ISO-8601 instant, ISO-8601 week date, and a
  month grid with today highlighted (all computed with [`jiff`]).
- **Settings** — persisted as JSON under `~/.config/stride/`.

## Install & run

Requires a Rust toolchain (1.74+).

```sh
cargo run                 # open the editor rooted at the current directory
cargo run -- src/main.rs  # open one or more files on launch
cargo run -- file.rs:42:7 # open and jump straight to line 42, column 7
cargo build --release     # optimized binary at target/release/stride
```

For best results use a [Nerd Font] so the file/folder/clock glyphs render.

## Keyboard shortcuts

A few of the most common; see [`docs/keybindings.md`](docs/keybindings.md) for
the full reference.

| Shortcut | Action                | Shortcut   | Action            |
| -------- | --------------------- | ---------- | ----------------- |
| `Ctrl+P` | Command palette       | `Ctrl+F`   | Find              |
| `Ctrl+O` | Open file…            | `Ctrl+R`   | Find & Replace    |
| `Ctrl+S` | Save                  | `F3`       | Find next         |
| `Ctrl+W` | Close tab             | `Ctrl+B`   | Toggle explorer   |
| `Ctrl+Q` | Quit                  | `Ctrl+E`   | Focus explorer    |
| `Ctrl+Z` | Undo                  | `F10`      | Open menu bar     |
| `F1`     | Keyboard help overlay | `F3`       | Find next         |

## Project layout

```
src/
  main.rs      binary entry point: terminal setup + event loop
  lib.rs       library crate (everything below is a public module)
  app.rs       central state, event routing, action dispatch
  editor.rs    tabs/buffers over ratatui-code-editor
  explorer.rs  file-tree drawer
  menu.rs      menu-bar definitions + dropdown state
  palette.rs   command palette + fuzzy matching
  search.rs    find/replace toolbar state
  messages.rs  notifications drawer
  datetime.rs  jiff-based time/ISO formatting + month grid
  settings.rs  serde-backed settings
  theme.rs     colors + Nerd Font icons
  ui.rs        all rendering
spec/          design specification
docs/          architecture + keybindings reference
examples/      runnable examples that drive the library API
tests/         integration tests (no terminal required)
```

See [`docs/architecture.md`](docs/architecture.md) for the design, including why
the dependency versions are pinned the way they are.

## Examples

```sh
cargo run --example headless_edit   # open, edit, and save a file with no TUI
cargo run --example list_commands   # print every command-palette command
```

## Testing

```sh
cargo test        # integration + doc tests; runs without a terminal
cargo clippy      # lints (the tree is warning-clean)
```

The editing logic lives in the library crate and is exercised by
`tests/integration.rs` without needing a live terminal — typing, open/save round
trips, go-to-line, fuzzy matching, the search-pattern builder, regex replace with
capture groups, and the ISO/date formatting are all covered.

## License

Licensed under either of Apache-2.0 or MIT at your option.

[`ratatui`]: https://crates.io/crates/ratatui
[`ratatui-code-editor`]: https://crates.io/crates/ratatui-code-editor
[`jiff`]: https://crates.io/crates/jiff
[Nerd Font]: https://www.nerdfonts.com/
