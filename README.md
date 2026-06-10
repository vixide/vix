# Vix — Simple Terminal Rust IDE

Vix is a keyboard-friendly terminal text editor written in Rust. It opens
text files, edits them, and saves them — with a menu bar, a file-explorer drawer,
tabbed buffers, a message drawer, a command palette, find & replace, a calendar
box, switchable themes, a switchable UI language, and switchable keyboard
navigation styles. It is built on [`ratatui`] and an in-house code-editor widget.
By default it uses familiar macOS/Windows shortcuts (Ctrl+C / Ctrl+V, not modal),
and you can switch to Emacs or Vim "keyways" if you prefer. Mouse and keyboard
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
- **Tabbed editor** — each file is a tab; a full multiline editor with optional
  Tree-sitter syntax highlighting, undo/redo, selection, system clipboard, line
  numbers (toggleable), a block cursor, and a right-side scrollbar.
- **Mouse support** — click to place the cursor, drag to select, wheel to scroll;
  click tabs, files, messages, and menus; click the dock toggle icons.
- **Image viewing** — open a PNG/JPG/GIF/… and it renders in a read-only image
  tab (via `ratatui-image`, on a graphics-capable terminal).
- **File explorer** — a directory tree in the left drawer; arrow-scan opens files
  in an ephemeral *preview* tab so you can browse without piling up tabs.
- **File explorer ops** — copy/cut/paste (`Ctrl+C`/`X`/`V`) with conflict
  prompts, `Shift+Up/Down` multi-select, and `Delete`; open buffers follow file
  moves and close on delete.
- **Command palette** (`Ctrl+P`) — four modes via prefix: file finder, `>`
  commands, `#` buffers, `:` go-to-line. Space-separated fuzzy matching.
- **Find & Replace** — incremental search, `F3`/`Shift+F3` navigation, Case /
  Whole-Word / Regex toggles, capture groups (`$1`, `${name}`) and escapes;
  project-wide search/replace and interactive query-replace.
- **Go to definition** (`F12`) — heuristic, language-agnostic jump to a symbol's
  likely definition across the project.
- **Position history** (`Alt+Left` / `Alt+Right`) — jump back and forward
  through cursor positions, like a browser.
- **Message drawer** — advice and notifications, each individually dismissable.
- **Calendar box** — local date/time, UTC ISO-8601 instant, ISO-8601 week date,
  and a navigable month grid with today highlighted (all computed with [`jiff`]).
- **Themes** — two built-in monochrome themes (Dark and Light) plus custom
  themes loaded from JSON, chosen live in **View → Theme…**. See
  [`docs/themes.md`](docs/themes.md).
- **Internationalization** — the whole UI is translatable; 15 languages are
  selectable (English, Spanish, French, German, Welsh fully; Irish, Scottish
  Gaelic, Polish, Portuguese, Russian, Arabic, Hindi, Bengali, Chinese, and
  Japanese with core coverage + English fallback). Choose one in
  **View → Locale…**, via the `locale` setting, or with `--locale`. See
  [`docs/i18n.md`](docs/i18n.md).
- **Keyways** — switch the keyboard navigation style in **View → Keyway…**:
  **Apple** (default; modifier shortcuts), **Emacs** (`Ctrl` chords with a
  `Ctrl+X` prefix), or **Vim** (modal Normal/Insert + `:` command line). See
  [`docs/keybindings.md`](docs/keybindings.md).
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
text. The grammar set lives in the internal `vix-code-editor-panel` crate. Note
that the built-in themes are monochrome by design; token colors appear only when
a custom theme defines a `syntax` block (see [`docs/themes.md`](docs/themes.md)).

## Configuration

Settings are stored with [`confy`] as TOML in the platform configuration
directory (e.g. `~/.config/vix/config.toml` on Linux). Custom themes live
alongside in `~/.config/vix/themes/*.json`. See
[`docs/configuration.md`](docs/configuration.md) for every key.

## Keyboard shortcuts

A few of the most common; see [`docs/keybindings.md`](docs/keybindings.md) for
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

## Project layout

Vix is a Cargo workspace: a main `vix` crate (the application) plus several small
internal crates, each owning one self-contained, separately-testable concern.

```
src/                            the vix application crate
  main.rs        binary entry point: CLI (clap), terminal setup, event loop
  lib.rs         library crate root; loads i18n translations (rust-i18n)
  app.rs         central state, event routing, action dispatch
  editor.rs      tabs/buffers over the code-editor widget
  explorer.rs    file-tree drawer
  menu.rs        menu-bar definitions + dropdown state
  palette.rs     command palette + fuzzy matching
  search.rs      find/replace toolbar state
  project_search.rs  project-wide search/replace panel
  query.rs       interactive query-replace session
  messages.rs    notifications drawer
  fileops.rs     explorer copy/cut/paste/delete helpers
  settings.rs    confy-backed settings + themes directory
  theme.rs       Nerd Font icons + re-export of the theme model
  ui.rs          all rendering
locales/         rust-i18n translation files (en/es/fr/de/cy)

vix-code-editor-panel/          the center editor widget (Tree-sitter, themeable)
vix-date-time-calendar-panel/   calendar date/time logic + navigable month grid
vix-theme-chooser/              theme model, styles, custom JSON themes, chooser
vix-locale-chooser/             available UI languages + chooser
vix-keyway-chooser/             keyboard navigation styles (Apple/Emacs/Vim) + chooser
vix-keyboard-shortcut-panel/    keyboard-help rows

spec/            design specification (the source of truth)
docs/            architecture, keybindings, themes, i18n, configuration
examples/        runnable examples that drive the library API
tests/           integration tests (no terminal required)
```

See [`docs/architecture.md`](docs/architecture.md) for the design, the workspace
shape, and why the dependency versions are pinned the way they are. The full
documentation map is in [`index.md`](index.md).

## Examples

```sh
cargo run --example headless_edit   # open, edit, and save a file with no TUI
cargo run --example list_commands   # print every command-palette command
```

## Testing

```sh
cargo test                 # the vix crate's integration + doc tests (no terminal)
cargo test --workspace     # also runs each internal crate's unit tests
cargo clippy --workspace   # lints (the tree is warning-clean)
```

The editing logic lives in the library crate and is exercised by
`tests/integration.rs` without needing a live terminal — typing, open/save round
trips, go-to-line, fuzzy matching, the search-pattern builder, regex replace with
capture groups, theme/locale/keyway switching, the Emacs and Vim keyways, and the
ISO/date formatting are all covered. Each internal crate carries its own focused
unit tests.

## License

Licensed under either of Apache-2.0 or MIT at your option.

[`ratatui`]: https://crates.io/crates/ratatui
[`jiff`]: https://crates.io/crates/jiff
[`confy`]: https://crates.io/crates/confy
[Nerd Font]: https://www.nerdfonts.com/
