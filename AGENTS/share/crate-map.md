# Crate and file map

Vix is a **Cargo workspace** (`[workspace] members = ["crates/*"]`) on **edition
2024**. The root package `vix` (`src/`) is the thin **App shell** — CLI, event
loop, `App` state, rendering, and the explorer — and it depends on the ~98
`vix-*` **member crates** under `crates/` that hold every feature plus the custom
editor widget (`vix-editor-core`). Shared reference for where things live.

**Specs are per-crate.** Each member crate owns its source-of-truth spec at
`crates/<crate>/spec/index.md` (multi-topic crates keep `spec/<topic>/index.md`
sub-specs). The top-level `spec/` keeps only cross-cutting / app-level and
build/meta docs (`index`, `navigation`, `comparisons`, `license`, `debian`,
`homebrew-tap-token`, `rust-cargo-*`, `rust-clippy-pedantic`, `test`, `tools`).

## Lint posture

Every crate root sets `#![forbid(unsafe_code)]`, `#![deny(missing_docs)]`, and
`#![warn(clippy::pedantic)]`, and **every module file** repeats
`#![warn(clippy::pedantic)]`. There is **no** blanket `#![allow(clippy::pedantic)]`
or `#![allow(missing_docs)]` anywhere — findings are fixed in code. Sanctioned
allows are only a few **targeted** ones: `#[allow(clippy::struct_excessive_bools)]`
on genuine state structs (`App`, `Settings`, `SearchBar`, `WorkspaceSearch`, and
`vix-editor-core`'s `Editor`) and a handful of `#[allow(clippy::too_many_lines)]` /
`too_many_arguments` on specific functions that resist further extraction.
`cargo clippy --all-targets -- -D warnings` is clean. See
[[rust-clippy-pedantic]] / `spec/rust-clippy-pedantic`.

## `vix-editor-core` — the code-editor widget (`crates/vix-editor-core/src/`)

The fully-custom terminal code-editor crate. Its engine (reused) modules carry
`#[allow(clippy::all, clippy::pedantic)]`; the Vix-owned modules are held to
pedantic. Reached from the host via `vix-editor`'s `CodeEditor` (a re-export of
`editor_core::editor::Editor`).

| Module             | Kind       | Owns                                                            |
| ------------------ | ---------- | --------------------------------------------------------------- |
| `code`             | engine     | Rope buffer + Tree-sitter parse/highlight + edit batches.       |
| `history`          | engine     | Undo/redo stacks.                                               |
| `selection`, `utils` | engine   | `Selection` type; grapheme/width/indent/comment helpers.        |
| `actions`          | engine     | Editing operations (insert/delete/move/indent/comment/…).       |
| `named`            | engine     | `snake_case` named-action methods on `Editor` (the `spec/actions.tsv` catalog). |
| `multicursor`      | engine     | Multiple-caret model + add/skip/spawn.                          |
| `editor`           | engine+Vix | `Editor` state, public API, marks, folds, inlay hints, mouse.   |
| `render`           | engine     | Non-wrap renderer + dispatch; fold/inlay row & column mapping.  |
| `wrap`             | **Vix**    | Soft-wrap visual-row layout + wrapped renderer.                 |
| `brackets`         | **Vix**    | Bracket matching (`matching_bracket`).                          |
| `lines`            | **Vix**    | Line transforms (move/sort/join/dedupe/trim/reverse).           |
| `editor_crossterm` | engine     | `KeyEvent` → actions mapping (always compiled).                 |

`code` also exposes `expand_to_node` (offline structural selection), `len_bytes`,
and serde on its history types (for persistent undo); `editor` adds the passive
`word_marks` channel, `relative_line_numbers`, and `comment_prefix`.

Tree-sitter highlight queries live in repo-root `langs/`, embedded with
`rust-embed` (`#[folder = "langs/"]`); grammars are gated behind `lang-*`
features (`syntax-common` by default, `syntax-all` for everything).

## App shell — root package `vix` (`src/`)

| File                  | Responsibility                                                       |
| --------------------- | -------------------------------------------------------------------- |
| `main.rs`             | clap CLI, locale resolution, terminal setup, event loop, suspend.   |
| `lib.rs`              | Crate root; lint config; `i18n!` catalog init; module declarations. |
| `app.rs`              | `App` state, `on_key`/`on_mouse`, `run_action`, overlays, behavior.  |
| `explorer.rs`         | `Explorer`: directory tree flattened to rows.                       |
| `messages.rs`         | `Messages`: notifications drawer model.                             |
| `search.rs`           | Re-exports / shared search helpers.                                 |
| `workspace_search.rs` | `WorkspaceSearch`: workspace-wide search/replace + static results.  |
| `edit_table.rs`       | CSV/TSV spreadsheet overlay (`Grid`).                               |
| `edit_outline.rs`     | Prose-hierarchy outline overlay (`Tree`).                           |
| `ui.rs`               | All rendering: layout + per-pane/overlay draw functions.            |

Everything else the shell used to own now lives in a member crate, reached
through the workspace dependency graph (e.g. `vix-editor`, `vix-menu`,
`vix-palette`, `vix-find-panel`, `vix-query`, `vix-session`, `vix-settings`,
`vix-theme`, `vix-fileops`, `vix-case`).

## Feature crates (`crates/vix-*`)

| Area        | Crates                                                                        |
| ----------- | ----------------------------------------------------------------------------- |
| Editor      | `vix-editor` (the `CodeEditor` host wrapper), `vix-editor-core` (the widget, above). |
| LSP / DAP   | `vix-lsp` (process IO + host wiring), `vix-lsp-core` (JSON-RPC framing, builders, parsers, positions); `vix-dap` (Debug Adapter Protocol client, reuses `lsp_core::frame`). |
| Git         | `vix-git` (status/diff/staging via the git CLI), `vix-conflict-tool` (merge-marker parser). |
| Spellcheck  | `vix-spellcheck` (Hunspell via `spellbook`).                                  |
| Snippets    | `vix-snippets` (JSON snippet files: scopes, parse, merge, picker), `vix-snippet-tool` (tabstop engine + bundled snippets). |
| Media types | `vix-media-type` (the MIME catalog parsed from `crates/vix-media-type/spec/media-types.tsv`; text/binary base, extension lookup, picker). |
| Org mode    | `vix-org` (headline structure, TODO/checkbox, Markdown/HTML export), `vix-affix` (prefix/suffix add/drop/toggle helpers), `vix-roam` (Org-roam nodes/backlinks/dailies/transclusion), `vix-org-contacts` (contact parsing + vCard). |
| Run / test  | `vix-tasks` (named `tasks.toml` runner), `vix-test-runner` (parse test output into a pass/fail tree), `vix-terminal` (integrated shell), `vix-diff-view` (compare-with-file). |
| Config      | `vix-editorconfig` (`.editorconfig` parsing), `vix-macros` (persisted keyboard macros), `vix-workspace` (`.toml` workspace: folders + files + split pane tree), `vix-settings` (confy-backed `Settings`), `vix-session` (save/restore). |
| AI          | `vix-ai-panel` (chat panel), `vix-ai-diff` (AI diff review).                  |
| Database    | `vix-db` (the **DB** menu workbench, `crates/vix-db/spec`): a full-screen overlay over embedded sqlx `Any` drivers (bundled SQLite, pure-Rust Postgres/MySQL over rustls). Submodules: `session` (one persistent connection per workbench on a worker thread; blocking `run` + async `send`/`poll` streaming `Chunk`s + `restart`), `connect` (saved-connection model + URLs), `catalog` (schema tree + per-engine metadata/EXPLAIN/DDL SQL), `editor`/`highlight`/`complete`/`format` (SQL editor: statement split, write detection, JOIN-aware autocomplete, beautify), `results` (grid: filter/sort/select/append), `store` (history + saved queries + session query log), `export` (6 formats), `ai` (schema-only NL→SQL, `spawn_ai` bridge), `chart` (ASCII bars), `erd` (Mermaid ER diagram), `import` (CSV/TSV → table), `params` (`:name` binds), `secret` (credential waterfall: `password_command` + OS keyring), `tunnel` (SSH `-L` forward). |
| Text tools  | `vix-format-tool`, `vix-jwt-tool`, `vix-base-tool`, `vix-base64-tool`, `vix-url-tool`, `vix-uuid-tool`, `vix-zid-tool`, `vix-checksum-tool`, `vix-regex-tool`, `vix-markdown-preview`, `vix-convert-tabular`, `vix-convert-from-*-into-*-tool` (12). |
| Pure text ops | `vix-align` (align lines on a delimiter), `vix-textops` (line-ending convert / squeeze blanks / ROT13, plus cursor-relative rewrites: increment number, smart toggle, transpose, `tag_column`), `vix-case` (selection case transforms), `vix-emmet` (abbreviation → HTML), `vix-tags` (HTML/XML matching-tag jump). Pure `text → text` / offset helpers with unit tests, driven from Edit/Go/Tools actions. |
| Networking  | `vix-http-client` (`.http`-buffer parser + blocking `ureq` send; response into a tab). |
| Undo store  | `vix-undo-store` (persist/restore the undo tree per file under `<config>/undo/`, content-hash guarded). |
| Themes      | `vix-theme` (Nerd Font icons + theme style helpers), `vix-base16` (bundled base16 color themes). |
| Edit surfaces | `vix-edit-value` (JSON/YAML tree, `Tree` + `Format`), `vix-edit-bytes` (hex/ASCII byte editor, `Hex`), `vix-edit-sql` (SQL statement list, `Editor`). Overlay editors with their own `handle_key`/`Outcome`, under **Edit → Mode**. (`edit_table`/`edit_outline` overlays live in the App shell.) |
| Generators  | `vix-qr-tool` (QR code via the `qrcode` crate, Unicode renderer), `vix-lorem` (deterministic lorem-ipsum text). |
| Tool dialogs| `vix-calculator-tool`, `vix-color-converter-tool`, `vix-unit-converter-tool`, `vix-pomodoro-tool`. |
| Info panels | `vix-text-information-panel`, `vix-file-information-panel`, `vix-system-information-panel`, `vix-status-bar-panel`, `vix-workspace-dashboard-panel`, `vix-outline-panel`, `vix-welcome-panel`. |
| Menu / find | `vix-menu` (3-level dropdown + command mode), `vix-palette` (command palette / fuzzy), `vix-find-panel` (find/replace state + engine), `vix-query` (interactive step-through replace). |
| Pickers     | `vix-ascii-character-picker`, `vix-html-character-picker`, `vix-nerd-font-picker`, `vix-x11-color-picker`. |
| Boxes       | `vix-calendar-panel`, `vix-clock-panel`.                                       |
| Contacts    | `vix-vcard-parser` (RFC 6350), `vix-vcard-panel`, `vix-contact-panel`.         |
| Docks       | `vix-left-dock` (explorer), `vix-right-dock` (messages), `vix-bottom-dock` (output buffer). |
| Files / ops | `vix-fileops` (explorer copy/cut/paste/delete filesystem helpers), `vix-file-browser-panel` (File → Open… browser: walkdir listing + fuzzy/glob/ext search, sort, filters). |
| Models      | `vix-keymap-model`, `vix-locale-model`, `vix-theme-model`, `vix-time-zone-model`. |
| Help        | `vix-keyboard-shortcut-panel`.                                                |
| i18n        | `vix-i18n` (embedded rust-i18n catalog).                                      |

## Other top-level paths

| Path            | Contents                                                            |
| --------------- | ------------------------------------------------------------------- |
| `crates/`       | The ~98 `vix-*` workspace member crates (each with its own `spec/`).|
| `langs/`        | Tree-sitter highlight queries (`<lang>/highlights.scm`), embedded.  |
| `locales/`      | `app.yml` — rust-i18n translations (English fallback).              |
| `dictionaries/` | Hunspell dictionaries — gitignored; see `crates/vix-spellcheck/spec/dictionaries`. |
| `themes/`       | Bundled JSON color themes.                                          |
| `spec/`         | Cross-cutting / app-level and build/meta specs (per-crate specs live in `crates/<crate>/spec/`). |
| `docs/`         | Architecture, keybindings, themes, i18n, LSP, configuration, panels. |
| `examples/`     | `headless_edit.rs`, `list_commands.rs`.                             |
| `tests/`        | `integration.rs`, `db_smoke.rs` — terminal-independent tests.       |
