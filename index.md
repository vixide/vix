# Vix IDE

Vix IDE is a high-speed high-security text editor featuring an integrated development environment. Vix provides a file explorer, keymaps, tools, locales, themes, and much more.

Vix looks like this:

```txt
Vix  File  Edit  View  Tools  AI  Git  Org  Debug  Help
╭─Explorer-──╮╭ main.rs ──────────────────╮╭Messages────────╮
│ README.md  ││ 1  fn main() {            ││ Welcome to Vix │ 
│ src        ││ 2      println!("hello"); ││ Ctrl+B toggles │
│   main.rs  ││ 3  }                      ││                │
╰────────────╯╰───────────────────────────╯╰────────────────╯
╭─Output────────────────────────────────────────────────────╮
│ hello                                                     │
╰───────────────────────────────────────────────────────────╯
src/main.rs — Ready              main • text UTF-8 Ln 2:Col 5
```

## Features

- **Menus** — File Edit View Tools AI Git Org Debug Help.
- **Editor** - Tabs, Undo/Redo, tree-sitters, syntax highlighting, etc.
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
- **Edit surfaces** (Edit → Mode) — view and edit the active buffer as a CSV/TSV
  table, a folding prose outline, a JSON/YAML tree, a hex byte dump, or a SQL
  statement list.
- **Insert** (Tools menu) — UUID/ZID, Markdown/HTML/SQL/LaTeX/Org snippets,
  inline Org markers and `#+BEGIN/END` blocks, Lorem ipsum, and Date/Time presets
  (ISO 8601 / RFC 3339 / epoch); plus a **QR Code** generator.
- **Snippets** — a searchable picker plus prefix-and-Tab expansion, loaded from
  JSON files (bundled, global, per-media-type, and project scopes).
- **Org mode** (Org menu) — headline promote/demote, subtree move, TODO cycling,
  checkbox toggle, fold cycling, and export to Markdown/HTML.
- **Media types** (Tools → Media Types) — a searchable MIME catalog (text/binary)
  with insert and extension lookup.
- **Test runner** (Tools → Run Tests) — parses `cargo test`/pytest-style output
  into a pass/fail panel with jump-to-failure.
- **Debugger** (Debug menu) — a DAP client: breakpoints, stepping, call stack,
  variables, watches, and an evaluate REPL.
- **Integrated terminal & tasks** — a shell in a panel, plus named `tasks.toml`
  commands and a compare-with-file diff.
- **Multi-cursor & column editing** — a caret on every match of the selection
  (select-all-occurrences), or a rectangular block (`Alt+Shift+↑/↓`).
- **Git** — status / diff / blame, **stage / unstage / revert per hunk**, diff
  navigation, branch switch & merge, stash, amend, and a merge-conflict resolver.
- **Language Server Protocol** — diagnostics, hover, completion, go-to,
  references, rename, code actions/lens, and inlay hints, configured per language.
- **Focus & navigation** — **Zen mode** hides the docks and status bar; an
  optional **breadcrumb bar** shows `file ▸ symbol`.
- **Themes** — Dark, Light, and more ship bundled, plus your can add your own.
- **Internationalization** — the whole UI is translatable into many languages.
- **Keymaps** — switch the keyboard bindings among Apple, macOS VSCode, Emacs,
  Vi, Spacemacs, IntelliJ (macOS/Windows), and Eclipse.
- **Split panes** — nested horizontal/vertical splits up to a 2×2 grid.
- **Keyboard help** — press `F1` for an overlay of every shortcut.

## Install & run

Requires a Rust toolchain (1.95+).

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

## Configuration

Settings are stored with [`confy`] as TOML in the platform configuration
directory (e.g. `~/.config/vix/config.toml` on Linux). Custom themes live
alongside in `~/.config/vix/themes/*.json`. See
[`docs/configuration/index.md`](docs/configuration/index.md) for every key.

## Examples

```sh
cargo run --example headless_edit   # open, edit, and save a file with no TUI
cargo run --example list_commands   # print every command-palette command
```

## License

Licensed under either of Apache-2.0 or MIT at your option.
