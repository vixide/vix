# Vix IDE™

Vix IDE is a high-speed high-security text editor featuring an integrated development environment. Vix™ provides a file explorer, keymaps, tools, locales, themes, and much more.

Vix looks like this:

```txt
Vix  File  Edit  View  Go  Run  AI  DB  Git  Org  Tools  Help
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

- **Menus** — Vix File Edit View Go Run AI DB Git Org Tools Help.
- **Editor** - Tabs, Undo/Redo, tree-sitters, syntax highlighting, etc.
 **soft wrap**, **bracket matching**, absolute or **relative line numbers**,
  visible whitespace, **indent guides**, **rainbow brackets**, **sticky scroll**,
  a code-overview **minimap**, a **read-only** lock, and configurable indentation
  (`indent_style` / `tab_width`).
- **Editing comforts** — Smart Home (`Home` → first non-blank, then col 0),
  comment toggle (`Ctrl+/`) and **comment banners**, **surround** selection with
  brackets/quotes, **align** lines on a delimiter, **increment/decrement** and
  **toggle** the value under the cursor, **transpose** chars/words, **Emmet**
  expansion, find next/previous occurrence of the selection (`Alt+N`/`Alt+P`),
  live go-to-line preview (palette `:`), and on-save trim-trailing-whitespace /
  ensure-final-newline / **format-on-save** / **auto-save** / auto-reload.
- **Line & text transforms** — sort, dedupe, shuffle, reverse, **squeeze blank
  lines**, **convert line endings** (LF/CRLF), case conversions, and **ROT13**,
  applied to the selection or whole buffer.
- **Clipboard history** — a kill-ring of recent copies/cuts with a
  paste-from-history picker.
- **Persistent undo** — the branch-preserving undo tree is saved per file and
  restored on reopen (content-hash guarded).
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
  **Smart-case** / Whole-Word / Regex toggles, capture groups (`$1`, `${name}`)
  and escapes; workspace-wide search/replace and interactive query-replace.
- **Go to definition** (`F12`) — heuristic, language-agnostic jump to a symbol's
  likely definition across the workspace.
- **Navigation** — **jump to line by label** (leap-style), **matching tag**
  (HTML/XML), go to **percent**/**byte**, and **structural selection** that
  expands to the enclosing syntax node (Tree-sitter offline, or LSP).
- **Highlight word occurrences** — passively mark every occurrence of the word
  under the cursor.
- **Which-key** — a popup of candidate keys while a chorded prefix is pending
  (Emacs `Ctrl+X`, Spacemacs `Space`).
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
  checkbox toggle, fold cycling, export to Markdown/HTML, and **Org-roam**: nodes,
  `[[`-completion, a **dailies calendar**, and a **live backlinks** panel.
- **TODO finder** (Tools) — scan the project for TODO/FIXME/HACK/XXX/BUG/NOTE
  comment tags into a jump-list.
- **HTTP client** (Tools → Send HTTP Request) — send a request from a `.http`
  buffer and open the response in a tab.
- **Scratch buffer** (File) — a throwaway, unsaved buffer for quick notes.
- **Media types** (Tools → Media Types) — a searchable MIME catalog (text/binary)
  with insert and extension lookup.
- **Test runner** (Tools → Run Tests) — parses `cargo test`/pytest-style output
  into a pass/fail panel with jump-to-failure.
- **Debugger** (Run menu) — a DAP client: breakpoints, stepping, call stack,
  variables, watches, and an evaluate REPL.
- **Integrated terminal & tasks** — a shell in a panel, plus named `tasks.toml`
  commands and a compare-with-file diff.
- **Multi-cursor & column editing** — a caret on every match of the selection
  (select-all-occurrences), or a rectangular block (`Alt+Shift+↑/↓`).
- **Git** — status / diff / blame, **stage / unstage / revert per hunk**, diff
  navigation, branch switch & merge, stash, amend, and a merge-conflict resolver.
- **Database workbench** (DB menu) — connect to SQLite / PostgreSQL / MySQL over
  embedded drivers (no client tools needed) and browse a schema tree, run
  queries in a syntax-highlighted editor with autocomplete, and read results in
  a filterable grid. **Read-only by default**; async execution with `Ctrl+C`
  cancel and streamed large results; transactions with a `TX` badge; a natural
  language → SQL **AI assistant** (schema-only, EXPLAIN-validated); query log,
  Mermaid **ER diagram**, CSV/TSV import, `:name` parameters, staged cell edits,
  an ASCII result chart, SSH tunnels, and an OS-keyring credential waterfall.
- **Language Server Protocol** — diagnostics, hover, completion, go-to,
  references, **call hierarchy**, rename, code actions/lens, and inlay hints,
  configured per language.
- **Focus & navigation** — **Zen mode** hides the docks and status bar; an
  optional **breadcrumb bar** shows `file ▸ symbol`.
- **Themes** — Dark, Light, and more ship bundled, plus your can add your own.
- **Internationalization** — the whole UI is translatable into many languages.
- **Keymaps** — switch the keyboard bindings among Apple, VSCode (macOS/Windows),
  Emacs, Vi, Spacemacs, IntelliJ (macOS/Windows), Eclipse, and Sublime Text.
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

---

Vix™ and Vix IDE™ are trademarks.
