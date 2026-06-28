# Vix: Simple Terminal Rust IDE

Goal: Create a Simple Terminal Rust Integrated Development Environment. It opens
text files, edits them, saves them.

Nerd Font icons, monospace.

## Crates

Vix is built on a deliberately small, version-compatible crate set. The whole
`ratatui` widget ecosystem must agree on one `ratatui` version (0.30); the editor
widget pins that.

| Name          | Purpose                                               | URL                                    | Debian equivalent?                                                                             | Debian unstable version | Debian 14 Forky version |
| ------------- | ----------------------------------------------------- | -------------------------------------- | ---------------------------------------------------------------------------------------------- | ----------------------- | ----------------------- |
| serde         | Settings + theme (de)serialization                    | https://crates.io/crates/serde         | librust-serde-dev                                                                              | ?                       | ?                       |
| serde-json    | Serialize/Deserialize JSON (and Vix config files)     | https://crates.io/crate/serde_json     | librust-serde-json-dev                                                                         | ?                       | ?                       |
| serde-yaml    | Serialize/Deserialize YAML                            | https://crates.io/crate/serde-yaml     | librust-serde-yaml-dev                                                                         | ?                       | ?                       |
| ratatui       | Terminal UI (layout, widgets)                         | https://crates.io/crates/ratatui       | librust-ratatui-dev                                                                            | ?                       | ?                       |
| ratatui-image | In-terminal image viewing (png/jpg/…)                 | https://crates.io/crates/ratatui-image |                                                                                                | ?                       | ?                       |
| image         | Image decoding for the viewer                         | https://crates.io/crates/image         | librust-image-dev, librust-image+default-dev                                                   | 0.25.x                  |
| crossterm     | Cross-platform terminal backend / events / mouse      | https://crates.io/crates/crossterm     | librust-crossterm-dev                                                                          | ?                       | ?                       |
| regex         | Regular expressions for find/replace; Unicode feature | https://crates.io/crates/regex         | librust-regex-dev, librust-regex+unicode-dev                                                   | ?                       | ?                       |
| jiff          | Date & time (local, UTC, ISO week)                    | https://crates.io/crates/jiff          | librust-jiff-dev                                                                               | ?                       | ?                       |
| rust-i18n     | Internationalization YAML files                       | https://crates.io/crates/rust-i18n     | librust-regex-dev                                                                              | ?                       | ?                       |
| confy         | Configuration                                         | https://crates.io/crates/confy         | librust-confy-dev                                                                              | ?                       | ?                       |
| clap          | Command Line Argument Parsing                         | https://crates.io/crates/clap          | librust-clap-dev, librust-clap-complete-dev, librust-clap-derive-dev, librust-clap-builder-dev | 4.6.1                   |
| mimalloc      | MiMalloc custom memory allocator for MUSL             | https://crates.io/crates/mimalloc      |                                                                                                | ?                       | ?                       |
| include_dir   | Embed bundled theme JSON files into the binary        | https://crates.io/crates/include_dir   |                                                                                                | ?                       | ?                       |
| tree-sitter   | Rust bindings to the Tree-sitter parsing library      | https://crates.io/crates/tree-sitter   | librust-tree-sitter-dev                                                                        | ?                       | ?                       |
| sysinfo       | Host system snapshot (System Information panel)       | https://crates.io/crates/sysinfo       | librust-sysinfo-dev                                                                            | ?                       | ?                       |
| spellbook     | Pure-Rust Hunspell spell checker                      | https://crates.io/crates/spellbook     |                                                                                                | ?                       | ?                       |
| similar       | Text diffing for the git diff gutter                  | https://crates.io/crates/similar       | librust-similar-dev                                                                            | ?                       | ?                       |
| evalexpr      | Evaluate expression (solely for calculator tool)      | https://crates.io/crates/evalexpr      | librust-evalexpr-dev                                                                           | ?                       | ?                       |
| rand          | Randomness functionality, number generators           | https://crates.io/crates/rand          | librust-rand-dev                                                                               | ?                       | ?                       |
| markdown      | Markdown parser & converter                           | https://crates.io/crate/markdown       | librust-markdown-dev                                                                           |
| csv           | Comma Separated Values                                | https://crates.io/crate/csv            | librust-csv-dev                                                                                |
| toml          | Tom's Obvious Minimal Language                        | https://crates.io/crate/toml           | librust-toml-dev                                                                               |

The center editing area uses **`editor_core`** — Vix's fully-custom code-editor
widget (Tree-sitter syntax highlighting, undo/redo history, selection, system
clipboard, mouse handling, theme-aware styles, and soft wrap), which tracks
`ratatui` 0.30. The file explorer, scrollbar, command
palette, popups, menu bar, and calendar box are implemented in-house on
`ratatui` primitives (`List`, `Scrollbar`, `Clear`, `Tabs`). The month grid is
computed with `jiff`, so the workspace depends on one date library only.

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
in `Cargo.toml` and the internal `editor_core` crate), so the binary
only links the parsers selected at build time. The default set is Rust, Markdown,
JSON, and TOML.

The application root is the current working directory; the explorer and the
command-palette file finder operate within it.

- Top menu (see `menus.md`)
- Left drawer file browser (in-house tree; `Ctrl+B` toggle, `Ctrl+E` focus)
- Center editing area using `editor_core` (Tree-sitter syntax
  highlighting, undo/redo, selection, system clipboard, block cursor)
  - Top tab bar: each tab is one text file; preview tabs render dimmed
  - Show/hide line numbers, whitespace, scroll bar, soft wrap (`View ▸ Editor`)
  - Editing comforts: select all (`Ctrl+A`), duplicate line (`Ctrl+D`), delete
    line (`Ctrl+K`), move line up/down (`Alt+↑`/`Alt+↓`), jump to the matching
    bracket (`Ctrl+]`), and auto-indent on Enter (see `editor_core/spec/index.md`)
  - Right-side scroll bar (`ratatui::widgets::Scrollbar`)
  - Opening an image file (png/jpg/gif/bmp/webp/…) shows it in a read-only
    image tab via `ratatui-image` (needs a graphics-capable terminal)
- Right drawer message browser
  - List of advice and notifications; each item shows a close `x`
    (dismiss with `x`, `Delete`, or `Enter` while the drawer is focused)
- Bottom dock (toggle with `View ▸ Show/Hide Bottom Dock`; see
  `bottom_dock/spec/index.md`) — a full-width, resizable, scrollable line panel pinned
  above the status bar for logs/output/data
  - **Run Command** (`Tools ▸ Run Command…`) streams a shell command's output
    here; **Cancel Command** kills it
  - **Search in Workspace → Dock** (`Edit ▸ Find`) lists `path:line:col` hits here
  - Lines that name a `path:line[:col]` location are **click-to-jump**; the dock
    can be focused (click) and scrolled, and follows new output only at the bottom
- Bottom status bar (toggle with `View ▸ Show/Hide Bottom Status`)
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

| Module             | Responsibility                                                 |
| ------------------ | -------------------------------------------------------------- |
| `app`              | Central state, event routing, action dispatch, keymap dispatch |
| `editor`           | Tabs/buffers wrapping `editor_core`; open/save/goto             |
| `explorer`         | Left-drawer directory tree                                     |
| `menu`             | Menu-bar definitions (i18n-keyed) and dropdown state           |
| `palette`          | Command palette (file/`>`/`#`/`:`/`@` modes) + fuzzy matching  |
| `search`           | Find / find-and-replace toolbar state                          |
| `workspace_search` | Workspace-wide search/replace panel state                      |
| `query`            | Interactive query-replace session                              |
| `messages`         | Right-drawer notifications                                     |
| `fileops`          | Explorer copy/cut/paste/delete filesystem helpers              |
| `settings`         | confy-backed settings (TOML) at `~/.config/vix/config.toml`    |
| `theme`            | Nerd Font icons + re-export of `theme_model`             |
| `ui`               | All rendering; lays out the frame and draws each pane          |

The calendar date/time logic, theme model, locale list, keymap (keyboard
navigation style) list, keyboard-help rows, Nerd Font glyph set, and find /
replace box state live in the internal crates `calendar_panel`,
`theme_model`, `locale_model`, `keymap_model`,
`keyboard_shortcut_panel`, `nerd_font_picker`, `find_panel`,
`left_dock` (explorer), `right_dock` (messages), `bottom_dock`, and
`status_bar_panel`. Bundled themes are embedded in the binary with
`include_dir`. See `docs/architecture.md`.

Event flow: `main` runs the loop, calling `ui::draw(&mut app)` (which records
each pane's rectangle for mouse hit-testing) then feeding each `crossterm` event
to `App::on_key` or `App::on_mouse`. `on_key` resolves modal layers in priority
order — help, dialog, calendar, theme/locale/keymap/recent choosers, Nerd Font
palette, query-replace, workspace search, confirm, paste-conflict, prompt, palette,
search, menu — before
the active **keymap** dispatch (Apple shortcuts / Emacs chords / Vim modal; see
`keymap_model/spec/index.md`) and, finally, the focused pane (editor / explorer /
messages / bottom dock). Each loop iteration also drains any streamed
command output into the bottom dock. Menu items and palette commands share one
set of action identifiers dispatched by `App::run_action`.

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

Also shipped: workspace-wide search & replace (`Ctrl+Shift+F`, searches open
buffers in their unsaved state), position history (`Alt+Left`/`Alt+Right`), and
"go to definition" (`F12`) — a fast offline heuristic over declaration-style
lines rather than a semantic LSP.

Also shipped: **internationalization** (`rust-i18n`; 27 selectable languages
including Klingon and Sindarin, English fallback, `--locale` flag + `locale`
setting + live **View → Locale…**), **themes** (every theme is a JSON theme;
Dark, Light, and more ship bundled, plus user-installed; live **View → Theme…**;
see `theme_model/spec/index.md`), **keymaps** (Apple / Emacs / Vim keyboard
navigation styles, live **View → Keymap…**; see `keymap_model/spec/index.md`),
**configuration** (`confy` TOML), a **CLI** (`clap`), the **Vix menu**
(About / Website / Email modal dialogs), **resizable docks** (drag a dock's inner
edge), **dock toggle icons** in the menu bar, **Open Recent**, **go-to-symbol**
(palette `@`), **comment toggle** (`Ctrl+/`), and **on-save normalization**
(`trim_trailing_whitespace` / `ensure_final_newline` settings).

Also shipped (editor widget): the center editor is now **`editor_core`**, Vix's
fully-custom widget (replacing the vendored fork; see `editor_core/spec/index.md`), with
**soft wrap** (**View → Editor → Show/Hide Soft Wrap**, the `soft_wrap` setting),
**bracket matching** (highlight the partner of the bracket at the cursor; no
auto-insert), **indentation settings** (`indent_style` / `tab_width` drive what
Tab inserts), **Smart Home** (`Home` → first non-blank, then column 0),
**find occurrence of selection** (`Alt+N` / `Alt+P`), **live go-to-line preview**
(the cursor follows the number typed in palette `:` mode), **visible whitespace**
(**View → Editor → Show/Hide Whitespace**), and a **richer status bar**
(language, line ending, encoding, selection char/line count).

Also shipped: **menu separators** grouping dropdown items (File/Edit/View);
**Nerd Font Palette** (Tools → a glyph picker, `nerd_font_picker`);
**Show/Hide Bottom Status** (`View → Show/Hide Bottom Status`, `show_status_bar`
setting); more **editing comforts** — Select All (`Ctrl+A`), Duplicate Line
(`Ctrl+D`), Move Line Up/Down (`Alt+↑`/`Alt+↓`), Jump to Matching Bracket
(`Ctrl+]`), and auto-indent on Enter; the find / replace box state extracted to
`find_panel` with **click-to-focus** fields; and **borderless screen edges**
(the left/right docks drop their outer border and the editor its left/right
borders).

Also shipped: the left/right docks and the status bar were extracted to internal
crates (`left_dock`, `right_dock`, `status_bar_panel`); a new
**bottom dock** (`bottom_dock`, `View → Show/Hide Bottom Dock`) — resizable
(drag its top edge), scrollable (sticky-bottom), with **click-to-jump** on
`path:line` lines — fed by **Run Command** / **Cancel Command** (Tools) and
**Search in Workspace → Dock** (Edit → Find; `Alt+C` case / `Alt+R` regex). Also:
nested **submenus** (View → Editor, Edit → Find), **menu type-ahead** (type a
letter to jump to the next matching item), **Close All Tabs**, **Reopen Closed
Tab** (`Ctrl+Shift+T`), **Find Next/Previous** (`Ctrl+G`/`Ctrl+Shift+G`, remember
the last search), and the **calendar** gained click-to-insert and clickable
month-nav arrows.

Also shipped: an **unsaved-changes prompt** on tab close / quit (Save / Don't
Save / Cancel); the **ASCII panel** (Tools → ASCII; `ascii_character_picker`); the
**System Information panel** (Tools → System Information; `system_information_panel`,
via `sysinfo`); **case transforms** (Edit → Case: upper/lower/title/kebab/snake/
camel/pascal; `src/case.rs`); a configurable bottom-dock **scrollback** setting;
and **workspace-search path filters** (Include/Exclude path regex).

Also shipped: **spell checking** (`spellcheck`, via `spellbook`) — red
underline of misspellings in comments/strings (View → Editor → Toggle
Spellcheck), `Ctrl+;` suggestions popup (replace / add to dictionary / ignore),
Hunspell dictionaries autodetected from standard locations (`dictionary_path`
setting); see `spellcheck.md` and `dictionaries.md`.

Also shipped: **git integration** (`git`, shelling out to the `git` CLI) —
branch + dirty indicator in the status bar, M/A/?/D/R/U badges in the explorer, a
colored diff gutter against HEAD, and a **Git** menu with a Changes panel
(stage/unstage/commit), Switch Branch, and Pull/Push/Fetch; see
`git-integration.md`.

Roadmap (designed in the sibling spec files, not yet built): a real LSP client
(semantic go-to-definition, completions, diagnostics), display tab width
(literal tabs as `tab_width` columns), and git conflict resolution. Each sibling
spec marks its own status.
