# STRIDE: Simple Terminal Rust IDE

Goal: Create a Simple Terminal Rust Integrated Development Environment. It opens
text files, edits them, saves them.

Nerd Font icons, monospace.

## Crates

STRIDE is built on a deliberately small, version-compatible crate set. The whole
`ratatui` widget ecosystem must agree on one `ratatui` version (0.30); the editor
widget pins that.

| Name                | Purpose                                              | URL                                         |
| ------------------- | ---------------------------------------------------- | ------------------------------------------- |
| serde               | Settings (de)serialization                           | https://crates.io/crates/serde              |
| serde_json          | Settings file format                                 | https://crates.io/crates/serde_json         |
| ratatui             | Terminal UI (layout, widgets)                        | https://crates.io/crates/ratatui            |
| ratatui-code-editor | Center editing widget (Tree-sitter syntax, history)  | https://crates.io/crates/ratatui-code-editor |
| ratatui-image       | In-terminal image viewing (png/jpg/…)                | https://crates.io/crates/ratatui-image      |
| image               | Image decoding for the viewer                        | https://crates.io/crates/image              |
| crossterm           | Cross-platform terminal backend / events / mouse     | https://crates.io/crates/crossterm          |
| regex               | Regular expressions for find/replace                 | https://crates.io/crates/regex              |
| jiff                | Date & time (local, UTC, ISO week)                   | https://crates.io/crates/jiff               |

The center editing area uses **`ratatui-code-editor`** (Tree-sitter syntax
highlighting, undo/redo history, selection, system clipboard, and built-in mouse
handling), which tracks `ratatui` 0.30. The file explorer, scrollbar, command
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
in `Cargo.toml` and the vendored `vendor/ratatui-code-editor`), so the binary
only links the parsers selected at build time. The default set is Rust, Markdown,
JSON, and TOML.

The application root is the current working directory; the explorer and the
command-palette file finder operate within it.

## UI

- Top menu bar (open with `F10`, or `Alt+F/E/T/H`; arrows navigate; `Enter`
  runs; `Esc` closes)
  - File menu
    - New
    - Open...
    - Save
    - Save As...
    - Close
    - Quit
  - Edit menu
    - Undo
    - Redo
    - Cut
    - Copy
    - Paste
    - Find
    - Find & Replace
  - Tools menu
    - Calendar
    - Command Palette
    - Toggle Line Numbers
    - Toggle Explorer
    - Toggle Messages
  - Help menu
    - Keyboard Shortcuts (also `F1`)
    - Website
    - Email Us
    - About STRIDE
- Left drawer file browser (in-house tree; `Ctrl+B` toggle, `Ctrl+E` focus)
- Center editing area using `ratatui-code-editor` (Tree-sitter syntax
  highlighting, undo/redo, selection, system clipboard)
  - Top tab bar: each tab is one text file; preview tabs render in italics
  - Show/hide line numbers (`Tools ▸ Toggle Line Numbers`)
  - Right-side scroll bar (`ratatui::widgets::Scrollbar`)
  - Opening an image file (png/jpg/gif/bmp/webp/…) shows it in a read-only
    image tab via `ratatui-image` (needs a graphics-capable terminal)
- Right drawer message browser
  - List of advice and notifications; each item shows a close `x`
    (dismiss with `x`, `Delete`, or `Enter` while the drawer is focused)
- Bottom status bar
  - File path and dirty indicator, plus the latest status message
  - Line number : Column number
  - Calendar icon; toggle the calendar box from `Tools ▸ Calendar`
- Mouse: click to place the cursor or focus a pane, drag to select, wheel to
  scroll; click a tab to switch, click a message's `x` to dismiss it, click a
  menu name to open it
- Keyboard shortcut help: press `F1` (or `Help ▸ Keyboard Shortcuts`) for an
  overlay of every binding

Calendar box:

- Current local time (`HH:MM:SS`)
- Current global time in ISO 8601 format `YYYY-MM-DDTHH:MM:SSZ`
- Current ISO 8601 commercial week date `YYYY-Www-D` — the ISO week-numbering
  year (which may occasionally differ from the Gregorian year), `Www` the week
  number `01..53`, and `D` the day of week `1` (Monday) .. `7` (Sunday)
- Calendar month view: an in-house Monday-first day grid computed with `jiff`,
  highlighting today

## Architecture

The crate exposes a library (`src/lib.rs`) plus a thin binary (`src/main.rs`).
Splitting it this way keeps all editing logic terminal-independent and unit
testable.

| Module     | Responsibility                                            |
| ---------- | --------------------------------------------------------- |
| `app`      | Central state, event routing, action dispatch             |
| `editor`   | Tabs/buffers wrapping `ratatui-code-editor`; open/save/goto |
| `explorer` | Left-drawer directory tree                                |
| `menu`     | Menu-bar definitions and dropdown state                   |
| `palette`  | Command palette (file/`>`/`#`/`:` modes) + fuzzy matching |
| `search`   | Find / find-and-replace toolbar state                     |
| `messages` | Right-drawer notifications                                |
| `datetime` | Local/UTC/ISO formatting and the month grid (via `jiff`)  |
| `settings` | serde-backed settings at `~/.config/stride/settings.json` |
| `theme`    | Colors and Nerd Font icons                                |
| `ui`       | All rendering; lays out the frame and draws each pane     |

Event flow: `main` runs the loop, calling `ui::draw(&mut app)` (which records
each pane's rectangle for mouse hit-testing) then feeding each `crossterm` event
to `App::on_key` or `App::on_mouse`. `on_key` resolves modal layers in priority
order — help, prompt, palette, search, menu — before global shortcuts and,
finally, the focused pane (editor / explorer / messages). Menu items and palette
commands share one set of action identifiers dispatched by `App::run_action`.

## Implementation status

Shipped: menu bar, tabbed editor with Tree-sitter syntax highlighting, file
explorer, message drawer, status bar, calendar box, command palette (4 modes),
incremental find, find & replace (regex, capture groups, escapes, case/word
toggles), interactive query-replace (`Ctrl+Alt+R`, `y/n/!/q`), settings
persistence, the `path:line:col` open syntax, mouse interactions
(click-to-position, drag-select, wheel scroll, tab/pane/menu clicks), in-terminal
image viewing, and the `F1` keyboard-shortcut help overlay.

Also shipped: explorer file clipboard (copy/cut/paste with conflict prompt),
multi-selection, delete-with-confirm, and buffers that follow file moves.

Roadmap (designed in the sibling spec files, not yet built): LSP "go to
definition", position history, project-wide search & replace, and the live
go-to-line preview. Each sibling spec marks its own status.
