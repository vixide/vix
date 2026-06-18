# Vix — Simple Terminal Rust IDE

Vix is a keyboard-friendly terminal text editor written in Rust. It opens
text files, edits them, and saves them — with a menu bar, a file-explorer drawer,
tabbed buffers, a message drawer, a command palette, find & replace, a calendar
box, switchable themes, a switchable UI language, and switchable keyboard
navigation styles. It is built on [`ratatui`] and an in-house code-editor widget.
By default it uses familiar macOS/Windows shortcuts (Ctrl+C / Ctrl+V, not modal),
and you can switch to Emacs or Vim "keymaps" if you prefer. Mouse and keyboard
both work.

```
 File  Edit  View  Tools  Help                              Vix    
╭─ Explorer ─────────╮╭ main.rs │ spec.md ────────────────────╮╭─ Messages ─────╮
│  src               ││╭──────────────────────────────────────╮││ Welcome to     │
│   app.rs           │││ 1  fn main() {                       △│││ Vix…      x │
│   editor.rs        │││ 2      println!("hi");               ┃│││ Ctrl+B toggles │
│   main.rs          │││ 3  }                                 ┃│││ the explorer x │
│  spec              │││ 4                                    ▽││╰────────────────╯
│   spec.md          ││╰──────────────────────────────────────╯│
╰────────────────────╯╰─────────────────────────────────────────╯
 src/main.rs — Saved                              Ln 2:Col 5    
```

## Features

- **Menu bar** — File / Edit / View / Tools / Help, navigable entirely by
  keyboard (`F10` or `Alt+F/E/V/T/H`), and by mouse.
- **Tabbed editor** (`editor_core`, Vix's fully-custom widget) — each file is a
  tab; a full multiline editor with optional Tree-sitter syntax highlighting,
  undo/redo, selection, system clipboard, a block cursor, a right-side scrollbar,
  **soft wrap**, **bracket matching**, toggleable line numbers and visible
  whitespace, and configurable indentation (`indent_style` / `tab_width`).
- **Editing comforts** — Smart Home (`Home` → first non-blank, then col 0),
  comment toggle (`Ctrl+/`), find next/previous occurrence of the selection
  (`Alt+N`/`Alt+P`), live go-to-line preview (palette `:`), and on-save
  trim-trailing-whitespace / ensure-final-newline.
- **Rich status bar** — language, line ending (LF/CRLF), encoding, the selected
  character/line count, and line:column.
- **Mouse support** — click to place the cursor, drag to select, wheel to scroll;
  click tabs, files, messages, and menus; click the dock toggle icons.
- **Image viewing** — open a PNG/JPG/GIF/… and it renders in a read-only image
  tab (via `ratatui-image`, on a graphics-capable terminal).
- **File explorer** — a directory tree in the left drawer; arrow-scan opens files
  in an ephemeral *preview* tab so you can browse without piling up tabs.
- **File explorer ops** — copy/cut/paste (`Ctrl+C`/`X`/`V`) with conflict
  prompts, `Shift+Up/Down` multi-select, and `Delete`; open buffers follow file
  moves and close on delete.
- **Command palette** (`Ctrl+P`) — five modes via prefix: file finder, `>`
  commands, `#` buffers, `:` go-to-line, `@` go-to-symbol. Space-separated fuzzy
  matching.
- **Find & Replace** — incremental search, `F3`/`Shift+F3` navigation, Case /
  Whole-Word / Regex toggles, capture groups (`$1`, `${name}`) and escapes;
  workspace-wide search/replace and interactive query-replace.
- **Go to definition** (`F12`) — heuristic, language-agnostic jump to a symbol's
  likely definition across the workspace.
- **Position history** (`Alt+Left` / `Alt+Right`) — jump back and forward
  through cursor positions, like a browser.
- **Message drawer** — advice and notifications, each individually dismissable.
- **Calendar box** — local date/time, UTC ISO-8601 instant, ISO-8601 week date,
  and a navigable month grid with today highlighted (all computed with [`jiff`]).
- **Nerd Font palette** (Tools menu) — a character picker: browse a grid of Nerd
  Font glyphs and click (or arrow + Enter) to insert one into the editor.
- **Themes** — JSON themes with per-region colors; Dark, Light, and more ship
  bundled, plus your own loaded from JSON, chosen live in **View → Theme…**. See
  [`docs/themes/index.md`](docs/themes/index.md).
- **Internationalization** — the whole UI is translatable; 27 languages are
  selectable, from English/Spanish/French/German/Welsh (fullest coverage)
  through Italian, Korean, Turkish, Dutch, Vietnamese, Greek, and more — even
  Klingon and Sindarin. Any untranslated key falls back to English. Choose one in
  **View → Locale…**, via the `locale` setting, or with `--locale`. See
  [`docs/internationalization/index.md`](docs/internationalization/index.md).
- **Keymaps** — switch the keyboard navigation style in **View → Keymap…**:
  **Apple** (default; modifier shortcuts), **Emacs** (`Ctrl` chords with a
  `Ctrl+X` prefix), or **Vim** (modal Normal/Insert + `:` command line). See
  [`docs/keybindings/index.md`](docs/keybindings/index.md).
- **Keyboard help** — press `F1` for an overlay of every shortcut.

## Install & run

Requires a Rust toolchain (1.86+).

```sh
cargo run                  # open the editor rooted at the current directory
cargo run -- src/main.rs   # open one or more files on launch
cargo run -- file.rs:42:7  # open and jump straight to line 42, column 7
cargo run -- --locale fr   # start in French (overrides the saved language)
cargo build --release      # optimized binary at target/release/vix
vix --help                 # full CLI usage
```

For best results use a [Nerd Font] so the file/folder/clock glyphs render.

### Syntax highlighting grammars (binary size)

Each Tree-sitter grammar is a sizeable compiled C parser, so they are gated
behind Cargo features. The default compiles a lean set (Rust, Markdown, JSON,
TOML); choose more or fewer at build time:

```sh
cargo build --release                                              # common grammars (default)
cargo build --release --no-default-features                        # no highlighting (smallest)
cargo build --release --no-default-features --features syntax-all  # all grammars (largest)
```

Files whose grammar isn't compiled in still open — just as plain (unhighlighted)
text. The grammar set lives in the `editor_core` module (queries in `langs/`). Token colors
appear only when the active theme defines a `syntax` block (see
[`docs/themes/index.md`](docs/themes/index.md)).

## Configuration

Settings are stored with [`confy`] as TOML in the platform configuration
directory (e.g. `~/.config/vix/config.toml` on Linux). Custom themes live
alongside in `~/.config/vix/themes/*.json`. See
[`docs/configuration/index.md`](docs/configuration/index.md) for every key.

## Keyboard shortcuts

A few of the most common; see [`docs/keybindings/index.md`](docs/keybindings/index.md) for
the full reference, or press `F1` in the app.

| Shortcut | Action                | Shortcut   | Action            |
| -------- | --------------------- | ---------- | ----------------- |
| `Ctrl+P` | Command palette       | `Ctrl+F`   | Find              |
| `Ctrl+O` | Open file…            | `Ctrl+R`   | Find & Replace    |
| `Ctrl+S` | Save                  | `F3`       | Find next         |
| `Ctrl+W` | Close tab             | `Ctrl+B`   | Toggle explorer   |
| `Ctrl+Q` | Quit                  | `Ctrl+E`   | Focus explorer    |
| `Ctrl+Z` | Undo                  | `F10`      | Open menu bar     |
| `F12`    | Go to definition      | `F1`       | Keyboard help     |

## Source layout

Vix is a **single Cargo crate** (edition 2024, no workspace members): the
application plus ~70 focused modules under `src/`, each owning one
self-contained, separately-testable concern.

```
src/
  main.rs         binary entry point: CLI (clap), terminal setup, event loop
  lib.rs          library crate root; loads i18n translations (rust-i18n)
  app.rs          central state, event routing, action dispatch
  editor.rs       tabs/buffers over the editor_core widget
  editor_core/    Vix's fully-custom editor widget (Tree-sitter, soft wrap,
                  folds, inlay hints, themeable)
  explorer.rs / left_dock.rs    file-tree state
  menu.rs         menu-bar definitions + 3-level dropdown state
  palette.rs      command palette + fuzzy matching
  find_panel.rs   find/replace box state + search/replace engine
  workspace_search.rs  workspace-wide search/replace
  lsp.rs / lsp_core/   Language Server Protocol (host IO + protocol core)
  git.rs          git status/diff/staging (git CLI)
  settings.rs     confy-backed settings + themes directory
  ui.rs           all rendering
  …               tools, panels, pickers, docks, models (see crate-map)
langs/            Tree-sitter highlight queries (feature-gated grammars)
locales/          rust-i18n translation files (English fallback)
spec/             design specification (the source of truth)
docs/             architecture, keybindings, themes, i18n, LSP, configuration
examples/         runnable examples that drive the library API
tests/            integration tests (no terminal required)
```

See [`docs/architecture/`](docs/architecture/) for the design and
[`AGENTS/share/crate-map.md`](AGENTS/share/crate-map.md) for the full module map.
The documentation map is in [`index.md`](index.md).

## Examples

```sh
cargo run --example headless_edit   # open, edit, and save a file with no TUI
cargo run --example list_commands   # print every command-palette command
```

## Testing

```sh
cargo test                 # integration + module unit + doc tests (no terminal)
cargo clippy --workspace --all-targets -- -D warnings   # lints (kept clean)
```

The editing logic lives in the library and is exercised by
`tests/integration.rs` without needing a live terminal — typing, open/save round
trips, go-to-line, fuzzy matching, the search-pattern builder, regex replace with
capture groups, theme/locale/keymap switching, the Emacs and Vim keymaps, LSP
message parsing, and ISO/date formatting are all covered. Each module also
carries its own focused unit tests.

## License

Licensed under either of Apache-2.0 or MIT at your option.

[`ratatui`]: https://crates.io/crates/ratatui
[`jiff`]: https://crates.io/crates/jiff
[`confy`]: https://crates.io/crates/confy
[Nerd Font]: https://www.nerdfonts.com/
