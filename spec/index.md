# Vix: Simple Terminal Rust IDE

Goal: Create a Simple Terminal Rust Integrated Development Environment. It opens
text files, edits them, saves them.

Nerd Font icons, monospace.

## Crates

Vix is built on a deliberately small, version-compatible crate set. The whole
`ratatui` widget ecosystem must agree on one `ratatui` version (0.30); the editor
widget pins that.

| Name          | Purpose                                               | URL                                    | Debian equivalent?                                                                             | Debian unstable version |
| ------------- | ----------------------------------------------------- | -------------------------------------- | ---------------------------------------------------------------------------------------------- | ----------------------- |
| serde         | Settings + theme (de)serialization                    | https://crates.io/crates/serde         | librust-serde-dev                                                                              |
| serde_json    | Custom theme JSON files                               | https://crates.io/crates/serde_json    | librust-serde-json-dev                                                                         |
| ratatui       | Terminal UI (layout, widgets)                         | https://crates.io/crates/ratatui       | librust-ratatui-dev                                                                            |
| ratatui-image | In-terminal image viewing (png/jpg/…)                 | https://crates.io/crates/ratatui-image |                                                                                                |                         |
| image         | Image decoding for the viewer                         | https://crates.io/crates/image         | librust-image-dev, librust-image+default-dev                                                   | 0.25.x                  |
| crossterm     | Cross-platform terminal backend / events / mouse      | https://crates.io/crates/crossterm     | librust-crossterm-dev                                                                          |
| regex         | Regular expressions for find/replace; Unicode feature | https://crates.io/crates/regex         | librust-regex-dev, librust-regex+unicode-dev                                                   |
| jiff          | Date & time (local, UTC, ISO week)                    | https://crates.io/crates/jiff          | librust-jiff-dev                                                                               |
| rust-i18n     | Internationalization YAML files                       | https://crates.io/crates/rust-i18n     | librust-regex-dev                                                                              |
| confy         | Configuration                                         | https://crates.io/crates/confy         | librust-confy-dev                                                                              |
| clap          | Command Line Argument Parsing                         | https://crates.io/crates/clap          | librust-clap-dev, librust-clap-complete-dev, librust-clap-derive-dev, librust-clap-builder-dev | 4.6.1                   |
| mimalloc      | MiMalloc custom memory allocator for MUSL             | https://crates.io/crates/mimalloc      |                                                                                                |
| include_dir   | Embed bundled theme JSON files into the binary        | https://crates.io/crates/include_dir   |                                                                                                |                         |
| tree-sitter   | Rust bindings to the Tree-sitter parsing library      | https://crates.io/crates/tree-sitter   | librust-tree-sitter-dev                                                                        |

The center editing area uses **`vix-editor`** — Vix's fully-custom code-editor
widget (Tree-sitter syntax highlighting, undo/redo history, selection, system
clipboard, mouse handling, theme-aware styles, and soft wrap), which tracks
`ratatui` 0.30. The file explorer, scrollbar, command
palette, popups, menu bar, and calendar box are implemented in-house on
`ratatui` primitives (`List`, `Scrollbar`, `Clear`, `Tabs`). The month grid is
computed with `jiff`, so the project depends on one date library only.

## Build and run

```sh
cargo run                 # open in the current directory
cargo run -- src/main.rs  # open one or more files
cargo run -- file.rs:42:7 # open and jump to line 42, column 7
cargo test                # run the logic + doc tests
cargo build --release     # optimized binary (~4.9M, common grammars)
cargo build --release --no-default-features                  # ~3.0M, no syntax grammars
cargo build --release --no-default-features --features syntax-all  # ~18M, all grammars
```

Tree-sitter grammars are gated behind Cargo features (see the `[features]` table
in `Cargo.toml` and the internal `vix-editor` crate), so the binary
only links the parsers selected at build time. The default set is Rust, Markdown,
JSON, and TOML.

The application root is the current working directory; the explorer and the
command-palette file finder operate within it.

- Top menu (see `menus.md`)
- Left drawer file browser (in-house tree; `Ctrl+B` toggle, `Ctrl+E` focus)
- Center editing area using `vix-editor` (Tree-sitter syntax
  highlighting, undo/redo, selection, system clipboard, block cursor)
  - Top tab bar: each tab is one text file; preview tabs render dimmed
  - Show/hide line numbers (`View ▸ Toggle Line Numbers`)
  - Right-side scroll bar (`ratatui::widgets::Scrollbar`)
  - Opening an image file (png/jpg/gif/bmp/webp/…) shows it in a read-only
    image tab via `ratatui-image` (needs a graphics-capable terminal)
- Right drawer message browser
  - List of advice and notifications; each item shows a close `x`
    (dismiss with `x`, `Delete`, or `Enter` while the drawer is focused)
- Bottom status bar
  - File path and dirty indicator, plus the latest status message
  - Language, line ending (LF/CRLF), encoding (UTF-8), and the selected
    character/line count when text is selected
  - Line number : Column number
  - Calendar icon; toggle the calendar box from `Tools ▸ Calendar`
- Mouse: click to place the cursor or focus a pane, drag to select, wheel to
  scroll; click a tab to switch, click a message's `x` to dismiss it, click a
  menu name to open it
- Keyboard shortcut help: press `F1` (or `Help ▸ Keyboard Shortcuts`) for an
  overlay of every binding

## Architecture

The crate exposes a library (`src/lib.rs`) plus a thin binary (`src/main.rs`).
Splitting it this way keeps all editing logic terminal-independent and unit
testable.

| Module           | Responsibility                                                 |
| ---------------- | -------------------------------------------------------------- |
| `app`            | Central state, event routing, action dispatch, keyway dispatch |
| `editor`         | Tabs/buffers wrapping `vix-editor`; open/save/goto  |
| `explorer`       | Left-drawer directory tree                                     |
| `menu`           | Menu-bar definitions (i18n-keyed) and dropdown state           |
| `palette`        | Command palette (file/`>`/`#`/`:`/`@` modes) + fuzzy matching  |
| `search`         | Find / find-and-replace toolbar state                          |
| `project_search` | Project-wide search/replace panel state                        |
| `query`          | Interactive query-replace session                              |
| `messages`       | Right-drawer notifications                                     |
| `fileops`        | Explorer copy/cut/paste/delete filesystem helpers              |
| `settings`       | confy-backed settings (TOML) at `~/.config/vix/config.toml`    |
| `theme`          | Nerd Font icons + re-export of `vix-theme-chooser`             |
| `ui`             | All rendering; lays out the frame and draws each pane          |

The calendar date/time logic, theme model, locale list, keyway (keyboard
navigation style) list, keyboard-help rows, and Nerd Font glyph set live in the
internal crates `vix-date-time-calendar-panel`, `vix-theme-chooser`,
`vix-locale-chooser`, `vix-keyway-chooser`, `vix-keyboard-shortcut-panel`, and
`vix-nerd-font-palette`. Bundled themes are embedded in the binary with
`include_dir`. See `docs/architecture.md`.

Event flow: `main` runs the loop, calling `ui::draw(&mut app)` (which records
each pane's rectangle for mouse hit-testing) then feeding each `crossterm` event
to `App::on_key` or `App::on_mouse`. `on_key` resolves modal layers in priority
order — help, dialog, calendar, theme/locale/keyway/recent choosers, Nerd Font
palette, query-replace, project search, confirm, paste-conflict, prompt, palette,
search, menu — before
the active **keyway** dispatch (Apple shortcuts / Emacs chords / Vim modal; see
`keyway-chooser.md`) and, finally, the focused pane (editor / explorer /
messages). Menu items and palette commands share one set of action identifiers
dispatched by `App::run_action`.

## Implementation status

Shipped: menu bar, tabbed editor with Tree-sitter syntax highlighting, file
explorer, message drawer, status bar, calendar box, command palette (5 modes),
incremental find, find & replace (regex, capture groups, escapes, case/word
toggles), interactive query-replace (`Ctrl+Alt+R`, `y/n/!/q`), settings
persistence, the `path:line:col` open syntax, mouse interactions
(click-to-position, drag-select, wheel scroll, tab/pane/menu clicks), in-terminal
image viewing, and the `F1` keyboard-shortcut help overlay.

Also shipped: explorer file clipboard (copy/cut/paste with conflict prompt),
multi-selection, delete-with-confirm, and buffers that follow file moves.

Also shipped: project-wide search & replace (`Ctrl+Shift+F`, searches open
buffers in their unsaved state), position history (`Alt+Left`/`Alt+Right`), and
"go to definition" (`F12`) — a fast offline heuristic over declaration-style
lines rather than a semantic LSP.

Also shipped: **internationalization** (`rust-i18n`; 15 selectable languages,
English fallback, `--locale` flag + `locale` setting + live **View → Locale…**),
**themes** (monochrome Dark/Light modes plus bundled and user-installed custom
JSON themes, live **View → Theme…**), **keyways** (Apple / Emacs / Vim keyboard
navigation styles, live **View → Keyway…**; see `keyway-chooser.md`),
**configuration** (`confy` TOML), a **CLI** (`clap`), the **Vix menu**
(About / Website / Email modal dialogs), **resizable docks** (drag a dock's inner
edge), **dock toggle icons** in the menu bar, **Open Recent**, **go-to-symbol**
(palette `@`), **comment toggle** (`Ctrl+/`), and **on-save normalization**
(`trim_trailing_whitespace` / `ensure_final_newline` settings).

Also shipped (editor widget): the center editor is now **`vix-editor`**, Vix's
fully-custom widget (replacing the vendored fork; see `code-editor.md`), with
**soft wrap** (**View → Toggle Soft Wrap**, the `soft_wrap` setting),
**bracket matching** (highlight the partner of the bracket at the cursor; no
auto-insert), **indentation settings** (`indent_style` / `tab_width` drive what
Tab inserts), **Smart Home** (`Home` → first non-blank, then column 0),
**find occurrence of selection** (`Alt+N` / `Alt+P`), **live go-to-line preview**
(the cursor follows the number typed in palette `:` mode), **visible whitespace**
(**View → Toggle Editor Visible Whitespace**), and a **richer status bar**
(language, line ending, encoding, selection char/line count).

Roadmap (designed in the sibling spec files, not yet built): a real LSP client
(semantic go-to-definition, completions, diagnostics), display tab width
(literal tabs as `tab_width` columns), and the unbuilt menu items (Select All,
Zoom, the keyboard _browser_ in `keyboard.md`). Each sibling spec marks its own
status.
