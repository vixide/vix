# Vix: Simple Terminal Rust IDE

Goal: Create a Simple Terminal Rust Integrated Development Environment. It opens
text files, edits them, saves them.

Nerd Font icons, monospace.

## Specification-driven development

The specification is the **single source of truth**. Vix is built
specification-first: each behavior is described in a spec, and the code
implements the spec. When code and spec disagree, decide which is correct and
make them match — edit the spec when intent changes, edit the code when it
drifted.

Specs are **per crate**. Vix is a Cargo workspace: a thin App shell (root package
`vix`, `src/`) over 104 `vix-*` member crates under `crates/`. Each member crate
owns its spec at `crates/<crate>/spec/index.md` (multi-topic crates add
`spec/<topic>/index.md` sub-specs), so a crate and its specification travel
together. This repo-root `spec/` holds only the cross-cutting / app-level and
build/meta specs that no single crate owns — this overview, `navigation`,
`comparisons`, `emacs-menus`, `tools`, `license`, `trademarks`,
`rust-clippy-pedantic`, `rust-cargo-fmt`, `test`, `ci`, `debian`, and the like.
A spec that describes one crate's behavior belongs with that crate; `scripts/check-docs`
enforces that every crate owns a `spec/index.md`.
See [`AGENTS.md`](../../AGENTS.md) and
[`agents/workflow.md`](../../agents/workflow.md) for the workflow, and
[`agents/share/crate-map.md`](../../agents/share/crate-map.md) for the full map.

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
| similar       | Text diffing (git diff gutter; word-level compare, `inline` feature) | https://crates.io/crates/similar | librust-similar-dev                                                                            | ?                       | ?                       |
| ureq          | HTTP client for the REST tool (pure-Rust rustls TLS)  | https://crates.io/crates/ureq          | librust-ureq-dev                                                                               | ?                       | ?                       |
| evalexpr      | Evaluate expression (solely for calculator tool)      | https://crates.io/crates/evalexpr      | librust-evalexpr-dev                                                                           | ?                       | ?                       |
| rand          | Randomness functionality, number generators           | https://crates.io/crates/rand          | librust-rand-dev                                                                               | ?                       | ?                       |
| markdown      | Markdown parser & converter                           | https://crates.io/crate/markdown       | librust-markdown-dev                                                                           |
| csv           | Comma Separated Values                                | https://crates.io/crate/csv            | librust-csv-dev                                                                                |
| toml          | Tom's Obvious Minimal Language                        | https://crates.io/crate/toml           | librust-toml-dev                                                                               |

`Cargo.toml` is authoritative for the full dependency set; the table above lists
the load-bearing ones. Also linked, grouped by why:

| Why | Crates |
| --- | ------ |
| Editor engine | `ropey` (rope buffer), `unicode-segmentation`, `unicode-width`, `streaming-iterator`, `ratatui-core`, `rust-embed` (Tree-sitter queries in `langs/`) |
| Clipboard | `arboard`, behind `vix-clipboard`'s process-wide lock and opt-in |
| Database workbench | `sqlx` (`Any` driver: bundled SQLite, pure-Rust Postgres/MySQL), `tokio`, `futures-util` |
| Terminal panel | `portable-pty`, `vt100`, `nix` (suspend/resume) |
| Hashes & ids | `sha2`, `md-5`, `crc32fast`, `uuid`, `getrandom`, `base64`, `percent-encoding` |
| Markup | `pulldown-cmark`, `htmd` (HTML → Markdown), `qrcode` |
| Files | `ignore` (gitignore-aware walking), `anyhow` |

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
  - Editing comforts: select all (`Ctrl+A`), duplicate line (`Ctrl+Shift+D`),
    delete line (`Ctrl+K`), forward delete (`Ctrl+D`, the macOS binding), move
    line up/down (`Alt+↑`/`Alt+↓`), jump to the matching bracket (`Ctrl+]`), and
    auto-indent on Enter (see `crates/vix-editor-core/spec/index.md`)
  - Right-side scroll bar (`ratatui::widgets::Scrollbar`)
  - Opening an image file (png/jpg/gif/bmp/webp/…) shows it in a read-only
    image tab via `ratatui-image` (needs a graphics-capable terminal)
- Right drawer message browser
  - List of advice and notifications; each item shows a close `x`
    (dismiss with `x`, `Delete`, or `Enter` while the drawer is focused)
- Bottom dock (toggle with `View ▸ Show/Hide Bottom Dock`; see
  `crates/vix-bottom-dock/spec/index.md`) — a full-width, resizable, scrollable line panel pinned
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

The root package exposes a library (`src/lib.rs`) plus a thin binary
(`src/main.rs`). Splitting it this way keeps all editing logic
terminal-independent and unit-testable: the binary owns the terminal, and
nothing else does.

The App shell is deliberately small — everything that can be a crate is one:

| Shell module          | Responsibility                                                  |
| --------------------- | ---------------------------------------------------------------- |
| `main.rs`             | CLI (`clap`), locale resolution, terminal setup/teardown, the event loop, suspend/resume |
| `lib.rs`              | Crate root: lint posture, `i18n!` catalog, and the `pub use vix_* as …` re-exports |
| `app.rs`              | `App` state, `on_key` / `on_mouse` / `on_paste`, `run_action`, overlay routing, feature wiring |
| `ui.rs`               | All rendering: frame layout and every pane/overlay draw function |
| `explorer.rs`         | The directory tree flattened into rows                          |
| `messages.rs`         | The notification drawer model                                   |
| `search.rs`, `workspace_search.rs` | Search bar helpers; workspace-wide search/replace state |
| `edit_table.rs`, `edit_outline.rs`, `column_view.rs` | Overlay editors that live in the shell because they drive the active buffer directly |

Everything else — the editor widget, menu, palette, find/replace, settings,
themes, locales, keymaps, docks, panels, pickers, Git, LSP/DAP, Org, the
database workbench, and every pure text tool — is a `vix-*` member crate.
`src/lib.rs` re-exports each under a short name (`pub use vix_git as git;`), so
shell code says `crate::git`, `crate::menu`, `crate::db`. See
[`agents/share/crate-map.md`](../../agents/share/crate-map.md) for the full map
and [`docs/architecture/index.md`](../../docs/architecture/index.md) for the
narrative version.

### Event flow

`main` runs the loop: draw, then poll one `crossterm` event.
`ui::draw(&mut app)` records each pane's rectangle so mouse events can be
hit-tested. Keys go to `App::on_key`, mouse to `App::on_mouse`, and a bracketed
paste to `App::on_paste` (see
[`crates/vix-editor/spec/bracketed-paste/index.md`](../../crates/vix-editor/spec/bracketed-paste/index.md)).

`on_key` resolves, in strict priority order:

1. Key-release events (dropped) and, on macOS, the `Command` → `Control` fold
   ([`crates/vix-editor/spec/command-key/index.md`](../../crates/vix-editor/spec/command-key/index.md)).
2. Jump-label mode, the LSP completion popup, and a pending hover tooltip.
3. Modal overlays — welcome, help, dialogs, tool dialogs, calendar/clock, then
   the panel layer (terminal, database, edit surfaces, choosers, query-replace,
   workspace search, confirm, prompt, palette, search, menu). `App::overlay_capturing_keys`
   mirrors this chain for callers that need to ask "is the editor reachable?"
   without dispatching; the two must be changed together.
4. Org-table context keys, when the cursor is inside a pipe table.
5. The active **keymap** (Apple, VSCode macOS/Windows, Emacs, Vi, Spacemacs,
   IntelliJ macOS/Windows, Eclipse, Sublime Text — see
   [`crates/vix-keymap-model/spec/index.md`](../../crates/vix-keymap-model/spec/index.md)),
   which translates keys into `run_action` ids and editor motions rather than
   duplicating behavior.
6. The focused pane: editor, explorer, messages, or bottom dock.

Each loop iteration also drains background work — streamed command output, LSP
and DAP messages, async database results, HTTP responses, file-change reloads,
auto-save — into the state the next draw reads. Menu items, palette commands,
and keymap chords all share one set of action ids dispatched by
`App::run_action`.

## Implementation status

Vix is **shipped and in use**; the feature set below is built, specified, and
tested. Each crate's own `spec/index.md` is authoritative for its area and marks
anything still in design.

| Area | State |
| ---- | ----- |
| Editing core | Tabs, undo **tree** (branch-preserving, persisted per file), soft wrap, bracket matching, indent guides, rainbow brackets, sticky scroll, minimap, multiple cursors, column selection, structural selection, read-only lock |
| Text transforms | Case, sort/dedupe/shuffle/reverse, squeeze blanks, line endings, ROT13, align, surround, transpose, delete-by-unit, wrap/fill, increment/toggle values, Emmet |
| Files & explorer | Tree, preview tabs, copy/cut/paste with conflict prompts, multi-select, delete, buffers that follow moves, File → Open browser, recent files, sessions and workspaces |
| Search | Incremental find, find/replace with regex + capture groups, smart case, workspace-wide search/replace, interactive query-replace, TODO finder |
| Navigation | Position history, go-to definition/symbol/line/percent/byte, jump labels, matching tag, breadcrumbs, outline panel |
| Language support | Tree-sitter highlighting (feature-gated grammars), LSP (diagnostics, hover, completion, references, call hierarchy, rename, code actions/lens, inlay hints), DAP debugging |
| Version control | Git status/diff/blame, per-hunk stage/unstage/revert, branch/stash/amend, merge-conflict resolver, diff gutter, word-level diff |
| Data & tools | Database workbench (SQLite/Postgres/MySQL), HTTP client, test runner, task runner, integrated terminal, converters, generators, pickers, info panels |
| Org mode | Headlines, TODO/checkbox cookies, agenda, capture, refile, footnotes, id links, tables (`TBLFM`), column view, Org-roam, dailies, backlinks, export |
| Presentation | Monochrome bundled themes plus custom JSON themes, base16 palettes, Zen mode, split panes (up to 2×2), 27 UI languages, ten keymaps |
| Terminal integration | Mouse (click/drag/wheel/hover), in-terminal images, bracketed paste, macOS `Command` folded to `Control`, suspend/resume |

### Quality gates

- `cargo clippy --workspace --all-targets -- -D warnings`, at `clippy::pedantic`,
  with `#![forbid(unsafe_code)]` and `#![deny(missing_docs)]` in every crate
  ([`spec/rust-clippy-pedantic/index.md`](../rust-clippy-pedantic/index.md)).
- `cargo test` — unit, integration, and doc tests, none of which need a TTY.
- `cargo fuzz` targets over the pure text/parse cores, and `cargo bench`
  (Criterion) over the hot paths ([`spec/test/index.md`](../test/index.md)).
- `scripts/check` runs the whole gate locally; `scripts/check-docs` checks that
  this documentation still matches the tree it describes.
- CI runs the same gate on all three forges ([`spec/ci/index.md`](../ci/index.md)).
