# Crate and file map

Vix is a **single Cargo crate** (no workspace members) on **edition 2024**. Every
former `vix-*` subcrate is now a module under `src/`; the custom editor widget is
the private-API-rich `editor_core` module. Shared reference for where things live.

## Lint posture

The crate root (`src/lib.rs`) sets `#![forbid(unsafe_code)]`, `#![deny(missing_docs)]`,
and `#![warn(clippy::pedantic)]`, and **every module file** repeats
`#![warn(clippy::pedantic)]`. There is **no** blanket `#![allow(clippy::pedantic)]`
or `#![allow(missing_docs)]` anywhere — findings are fixed in code. Sanctioned
allows are only a few **targeted** ones: `#[allow(clippy::struct_excessive_bools)]`
on genuine state structs (`App`, `Settings`, `SearchBar`, `WorkspaceSearch`, and
`editor_core`'s `Editor`) and a handful of `#[allow(clippy::too_many_lines)]` /
`too_many_arguments` on specific functions that resist further extraction.
`cargo clippy --all-targets -- -D warnings` is clean. See
[[rust-clippy-pedantic]] / `spec/rust-clippy-pedantic`.

## `editor_core` — the code-editor widget (`src/editor_core/`)

A `pub mod` whose engine (reused) modules carry `#[allow(clippy::all, clippy::pedantic)]`;
the Vix-owned modules are held to pedantic. Reached from the host via
`editor::CodeEditor` (a re-export of `editor_core::editor::Editor`).

| Module             | Kind       | Owns                                                            |
| ------------------ | ---------- | --------------------------------------------------------------- |
| `code`             | engine     | Rope buffer + Tree-sitter parse/highlight + edit batches.       |
| `history`          | engine     | Undo/redo stacks.                                               |
| `selection`, `utils` | engine   | `Selection` type; grapheme/width/indent/comment helpers.        |
| `actions`          | engine     | Editing operations (insert/delete/move/indent/comment/…).       |
| `named`            | engine     | `snake_case` named-action methods on `Editor`.                  |
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
`rust-embed` (`#[folder = "langs/"]`); grammars are gated behind the crate's
`lang-*` features (`syntax-common` by default, `syntax-all` for everything).

## Core application modules (`src/`)

| File                  | Responsibility                                                       |
| --------------------- | -------------------------------------------------------------------- |
| `main.rs`             | clap CLI, locale resolution, terminal setup, event loop, suspend.   |
| `lib.rs`              | Crate root; lint config; `i18n!` catalog init; module declarations. |
| `app.rs`              | `App` state, `on_key`/`on_mouse`, `run_action`, overlays, behavior.  |
| `case.rs`             | Selection case transforms (upper/lower/title/kebab/snake/camel/pascal). |
| `editor.rs`           | `Editor`/`Tab`: buffers over `editor_core`; open/save/goto/folds.    |
| `explorer.rs`         | `Explorer`: directory tree flattened to rows.                       |
| `fileops.rs`          | Explorer copy/cut/paste/delete filesystem helpers.                  |
| `menu.rs`             | Menu definitions (i18n-keyed) + `Menu` 3-level dropdown state.       |
| `palette.rs`          | `Palette`, mode detection, fuzzy match, `path:line:col` parsing.     |
| `search.rs`           | Re-exports / shared search helpers.                                 |
| `find_panel.rs`       | `SearchBar`: find/replace state + the search/replace engine.        |
| `workspace_search.rs` | `WorkspaceSearch`: workspace-wide search/replace + static results.  |
| `query.rs`            | Interactive step-through replace session state.                     |
| `messages.rs`         | `Messages`: notifications drawer model.                             |
| `session.rs`          | Session save/restore.                                               |
| `settings.rs`         | confy-backed `Settings`; themes directory locator.                  |
| `theme.rs`            | Nerd Font icons + theme style helpers.                              |
| `ui.rs`               | All rendering: layout + per-pane/overlay draw functions.            |

## Feature modules (`src/`, formerly subcrates)

| Area        | Modules                                                                       |
| ----------- | ----------------------------------------------------------------------------- |
| LSP / DAP   | `lsp` (process IO + host wiring), `lsp_core` (JSON-RPC framing, builders, parsers, positions); `dap` (Debug Adapter Protocol client, reuses `lsp_core::frame`). |
| Git         | `git` (status/diff/staging via the git CLI), `conflict_tool` (merge-marker parser). |
| Spellcheck  | `spellcheck` (Hunspell via `spellbook`).                                      |
| Snippets    | `snippets` (JSON snippet files: scopes, parse, merge, picker), `snippet_tool` (tabstop engine + bundled snippets). |
| Media types | `media_type` (the MIME catalog parsed from `spec/media-types/media-types.tsv`; text/binary base, extension lookup, picker). |
| Org mode    | `org` (headline structure, TODO/checkbox, Markdown/HTML export), `affix` (prefix/suffix add/drop/toggle helpers), `roam` (Org-roam nodes/backlinks/dailies/transclusion), `org_contacts` (contact parsing + vCard). |
| Run / test  | `tasks` (named `tasks.toml` runner), `test_runner` (parse test output into a pass/fail tree), `terminal` (integrated shell), `diff_view` (compare-with-file). |
| Config      | `editorconfig` (`.editorconfig` parsing), `macros` (persisted keyboard macros), `pane_tree` (nested split layout), `workspace` (`.toml` workspace: folders + files). |
| AI          | `ai_panel` (chat panel), `ai_diff` (AI diff review).                          |
| Text tools  | `format_tool`, `jwt_tool`, `base_tool`, `base64_tool`, `url_tool`, `uuid_tool`, `zid_tool`, `checksum_tool`, `regex_tool`, `snippet_tool`, `markdown_preview`, `convert_tabular`, `convert_from_*_into_*_tool` (12). |
| Pure text ops | `align` (align lines on a delimiter), `textops` (line-ending convert / squeeze blanks / ROT13), `emmet` (abbreviation → HTML), `tags` (HTML/XML matching-tag jump). Pure `text → text` / offset helpers with unit tests, driven from Edit/Go/Tools actions. |
| Networking  | `http_client` (`.http`-buffer parser + blocking `ureq` send; response into a tab). |
| Undo store  | `undo_store` (persist/restore the undo tree per file under `<config>/undo/`, content-hash guarded). |
| Themes      | `base16` (bundled base16 color themes). |
| Edit surfaces | `edit_table` (CSV/TSV spreadsheet, `Grid`), `edit_outline` (prose hierarchy, `Tree`), `edit_value` (JSON/YAML tree, `Tree` + `Format`), `edit_bytes` (hex/ASCII byte editor, `Hex`), `edit_sql` (SQL statement list, `Editor`). Overlay editors with their own `handle_key`/`Outcome`, under **Edit → Mode**. |
| Generators  | `qr_tool` (QR code via the `qrcode` crate, Unicode renderer), `lorem` (deterministic lorem-ipsum text). |
| Tool dialogs| `calculator_tool`, `color_converter_tool`, `unit_converter_tool`, `pomodoro_tool`. |
| Info panels | `text_information_panel`, `file_information_panel`, `system_information_panel`, `workspace_dashboard_panel`, `outline_panel`, `welcome_panel`. |
| Pickers     | `ascii_character_picker`, `html_character_picker`, `nerd_font_picker`, `x11_color_picker`. |
| Boxes       | `calendar_panel` (re-exported `calendar`), `clock_panel` (re-exported `clock`). |
| Contacts    | `vcard_parser` (RFC 6350), `vcard_panel`, `contact_panel`.                     |
| Docks       | `left_dock` (explorer), `right_dock` (messages), `bottom_dock` (output buffer). |
| Models      | `keymap_model`, `locale_model`, `theme_model`, `time_zone_model`.              |
| Help        | `keyboard_shortcut_panel`, `status_bar_panel`.                                |

## Other top-level paths

| Path            | Contents                                                            |
| --------------- | ------------------------------------------------------------------- |
| `langs/`        | Tree-sitter highlight queries (`<lang>/highlights.scm`), embedded.  |
| `locales/`      | `app.yml` — rust-i18n translations (English fallback).              |
| `dictionaries/` | Hunspell dictionaries — gitignored; see `spec/dictionaries`.        |
| `themes/`       | Bundled JSON color themes.                                          |
| `spec/`         | Specification (source of truth).                                    |
| `docs/`         | Architecture, keybindings, themes, i18n, LSP, configuration, panels. |
| `examples/`     | `headless_edit.rs`, `list_commands.rs`.                             |
| `tests/`        | `integration.rs` — terminal-independent tests.                      |
