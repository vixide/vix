# Crate and file map

Vix is a **Cargo workspace** (`[workspace] members = ["crates/*"]`) on **edition
2024**. The root package `vix` (`src/`) is the thin **App shell** — CLI, event
loop, `App` state, rendering, and the explorer — and it depends on the 112
`vix-*` **member crates** under `crates/` that hold every feature plus the custom
editor widget (`vix-editor-core`). Shared reference for where things live.

**Specs are per-crate.** Each member crate owns its source-of-truth spec at
`crates/<crate>/spec/index.md` (multi-topic crates keep `spec/<topic>/index.md`
sub-specs). The top-level `spec/` keeps only cross-cutting / app-level and
build/meta docs (`index`, `navigation`, `comparisons`, `emacs-menus`, `license`,
`trademarks`, `debian`, `homebrew-tap-token`, `ci`, `rust-cargo-*`,
`rust-clippy-pedantic`, `main-rs-and-lib-rs-boilerplate`, `test`, `tools`).

## Lint posture

Every crate root sets `#![forbid(unsafe_code)]`, `#![deny(missing_docs)]`, and
`#![warn(clippy::pedantic)]`, and **every module file** repeats
`#![warn(clippy::pedantic)]`. There is **no** blanket `#![allow(clippy::pedantic)]`
or `#![allow(missing_docs)]` anywhere — findings are fixed in code. Sanctioned
allows are only a few **targeted** ones: `#[allow(clippy::struct_excessive_bools)]`
on `App` and `Settings` (T149 converted `SearchBar`/`WorkspaceSearch`/
`DblockParams`/both `Editor`s to a `bitflags`-based `Flags` field instead,
removing their allows — prefer that over a new allow for a struct whose
bools are independent, freely-combinable toggles) and a handful of
`#[allow(clippy::too_many_lines)]` / `too_many_arguments` on specific
functions that resist further extraction.
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
| `click`            | **Vix**    | `ClickTracker`/`ClickKind`: single/double/triple-click classifier. |
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
| `app.rs`              | `App` state, `on_key`/`on_mouse`, `run_action`, overlays, behavior — dispatches most of its logic into `src/app/*.rs` (below). |
| `ui.rs`               | All rendering: frame layout — dispatches most per-pane/overlay draw functions into `src/ui/*.rs` (below). |

`src/app/*.rs` (T141) holds most of `App`'s actual logic, one file per feature
area: `keymap.rs` (`on_key` + the ten keymap dispatchers), `git.rs` (git/jj),
`scripts.rs`, `command_palette.rs`, `session.rs`, `lsp_dap.rs`, `roam.rs`,
`org_table.rs`, `org.rs`, `insert_tools.rs`, `picker_panels.rs`,
`info_panels.rs`. `src/ui/*.rs` (T142, done) holds `ui.rs`'s draw functions,
one file per overlay/panel family, across 22 submodules (`ui.rs` itself down
to 713 lines: the dispatch chain plus shared rendering primitives).

Everything else the shell used to own now lives in a member crate, reached
through the workspace dependency graph (e.g. `vix-editor`, `vix-menu`,
`vix-palette`, `vix-find-panel`, `vix-session`, `vix-settings`,
`vix-theme`, `vix-fileops`, `vix-case`) — `src/` is down to exactly
`main.rs`/`lib.rs`/`app.rs`/`ui.rs` now (T152, done): `explorer`/
`messages`/`search` are thin `pub use` aliases in `lib.rs` for
`vix-left-dock`/`vix-right-dock`/`vix-find-panel`, and
`workspace_search`/`edit_outline`/`edit_table`/`column_view` alias four new
crates the same way (`vix-workspace-search`, `vix-edit-outline`,
`vix-edit-table`, `vix-column-view`, below) — none of the seven has a
`src/*.rs` file of its own anymore.

## Feature crates (`crates/vix-*`)

| Area        | Crates                                                                        |
| ----------- | ----------------------------------------------------------------------------- |
| Editor      | `vix-editor` (the `CodeEditor` host wrapper), `vix-editor-core` (the widget, above). |
| LSP / DAP   | `vix-lsp` (process IO + host wiring), `vix-lsp-core` (JSON-RPC framing, builders, parsers, positions); `vix-dap` (Debug Adapter Protocol client, reuses `lsp_core::frame`). |
| Git         | `vix-git` (status/diff/staging via the git CLI), `vix-conflict-tool` (merge-marker parser). |
| Spellcheck  | `vix-spellcheck` (Hunspell via `spellbook`).                                  |
| Snippets    | `vix-snippets` (JSON snippet files: scopes, parse, merge, picker), `vix-snippet-tool` (tabstop engine + bundled snippets). |
| Media types | `vix-media-type` (the MIME catalog parsed from `crates/vix-media-type/spec/media-types.tsv`; text/binary base, extension lookup, picker). |
| Org mode    | `vix-org` (headline structure, TODO/checkbox, the column-view *spec* — format string, resolution, Markdown/HTML export), `vix-column-view` (T152: the interactive Column View *overlay* — cursor, edit modes, key handling — over `vix-org`'s resolved spec), `vix-org-table` (the built-in table editor: structural edits + `TBLFM` formulas), `vix-org-capture` (capture templates + placeholder expansion), `vix-affix` (prefix/suffix add/drop/toggle helpers), `vix-roam` (Org-roam nodes/backlinks/dailies/transclusion), `vix-org-contacts` (contact parsing + vCard). |
| Run / test  | `vix-tasks` (named `tasks.toml` tasks, project-type lifecycle commands, task discovery, monorepo subprojects, test-at-point — Project menu), `vix-test-runner` (parse test output into a pass/fail tree), `vix-terminal` (integrated shell), `vix-diff-view` (compare-with-file), `vix-coverage` (T210: LCOV/Cobertura XML report parsing for the coverage gutter — Tools → Load Coverage File…). |
| Config      | `vix-editorconfig` (`.editorconfig` parsing), `vix-macros` (persisted keyboard macros), `vix-workspace` (`.toml` workspace: folders + files + split pane tree), `vix-settings` (confy-backed `Settings`), `vix-session` (save/restore). |
| Scripting   | `vix-script` — Rhai user scripting (`crates/vix-script/spec/index.md`); a plain (non-optional) dependency, wired into the App shell as of T103: scripts load at startup and on `script.reload`, registered commands appear in the command palette (`script:<stem>:<id>`) and Tools → Scripts → Run…, `prompt`/`message`/`error` use the real prompt overlay and message drawer. **T104** (wiring `bind_key` into the real keymap) is done as of T104j — see the Keybindings row below. Only **T105** (sample scripts + docs) remains open. |
| Modal editing | `vix-modal` — the real modal-editing engine for the Vi/Spacemacs keymaps: modes, operator × motion composition, counts, registers, text objects, dot-repeat (`crates/vix-modal/spec/index.md`); replaces `vim_normal_key`'s ad hoc binding table in `src/app.rs`. **Design-only** as of T111: no functional code until T112. |
| Keybindings | `vix-keybindings` — an exhaustive, queryable registry of every built-in keybinding across all 10 keymap ids, plus the user (`keybindings.toml`)/script (`bind_key`) override layer built on it (`crates/vix-keybindings/spec/index.md`). **Epic complete, T104a–j.** **All 10 keymap ids converted** (Emacs T104a, Vi/Spacemacs T104b, VS Code T104c, `IntelliJ` T104d, Eclipse T104e, Sublime Text T104f, Apple T104g): `emacs_key`, `vim_normal_key`, `spacemacs_leader_key`, `vscode_ctrl_key`, `intellij_key`, `eclipse_key`, `sublime_key`, and `apple_ctrl_key` all dispatch through the registry now (`Binding`/`ChordContext`/`KeymapTable`/`lookup`/`shortcuts_for`, plus `lookup_sequence` for Spacemacs's leader-style sequence matching); `intellij-macos`/`intellij-windows` are two genuinely different tables (unlike VS Code's one shared table); Apple genuinely mixes Shift-guarded and Shift-agnostic letters in one keymap, the latter via IntelliJ's "faithfully preserve an unguarded quirk" duplicate-row technique applied more broadly. `global_shared_key` (called identically by every keymap, not itself keyed on one) is backed by a separate keymap-agnostic `SHARED`/`lookup_shared` list for its unconditional bindings. **T104h**: `Settings::keybindings_path()` + `user_bindings::{UserBinding, load, upsert}` round-trip `keybindings.toml` for real, mirroring `macros.toml`'s pattern. **T104i**: the `on_key` choke point is real — `overrides::{Source, Override, Conflict, Shadow, Resolved, resolve}` resolves override requests against each other (two claiming one token → both rejected, reported as an error) and against a keymap's built-ins (shadowing → not a conflict, reported once informationally); `App::override_key` (inserted between `org_table_key` and every keymap's own dispatch) consults the resolved `key_overrides` map ahead of all 10 keymaps; a new `keybindings.reload` action (+ Tools-menu leaf) mirrors `script.reload`. **T104j**: `App::resolve_key_overrides` (renamed from the narrower `load_key_overrides`) now also walks every loaded script's `bindings` (action id `format!("script:{stem}:{command_id}")`) into the *same* combined resolution — a script and a persisted override (or two scripts) claiming one key really do both get rejected now, not just in theory; `script.reload` re-resolves too, alongside `keybindings.reload`. Closes `crates/vix-script/spec/index.md`'s "Key bindings" conflict-handling contract for real — the *original* T104 ask, opened at the very start of this whole epic. **T204, done**: `vix-keybinding-editor-panel` (`crates/vix-keybinding-editor-panel/spec/index.md`) is a sibling crate holding `Source`/`Row`/`Column`/`Panel` for the editable **Vix → Keybindings…** overlay (rebind by typing a token, reset a user override) — the data/table layer, App-side row-building (`src/ui.rs`'s `draw_keybinding_editor`), and dispatch wiring (`src/app/keymap.rs`'s `panel!(keybinding_editor, keybinding_editor_key)`) are all in place. |
| AI          | `vix-ai-panel` (chat panel), `vix-ai-diff` (AI diff review).                  |
| Database    | `vix-db` (the **DB** menu workbench, `crates/vix-db/spec`): a full-screen overlay over embedded sqlx `Any` drivers (bundled SQLite, pure-Rust Postgres/MySQL over rustls). Submodules: `session` (one persistent connection per workbench on a worker thread; blocking `run` + async `send`/`poll` streaming `Chunk`s + `restart`), `connect` (saved-connection model + URLs), `catalog` (schema tree + per-engine metadata/EXPLAIN/DDL SQL), `editor`/`highlight`/`complete`/`format` (SQL editor: statement split, write detection, JOIN-aware autocomplete, beautify), `results` (grid: filter/sort/select/append), `store` (history + saved queries + session query log), `export` (6 formats), `ai` (schema-only NL→SQL, `spawn_ai` bridge), `chart` (ASCII bars), `erd` (Mermaid ER diagram), `import` (CSV/TSV → table), `params` (`:name` binds), `secret` (credential waterfall: `password_command` + OS keyring), `tunnel` (SSH `-L` forward). |
| Text tools  | `vix-format-tool`, `vix-jwt-tool`, `vix-base-tool`, `vix-base64-tool`, `vix-url-tool`, `vix-uuid-tool`, `vix-zid-tool`, `vix-checksum-tool`, `vix-regex-tool`, `vix-markdown-preview`, `vix-convert-tabular`, `vix-convert-from-*-into-*-tool` (12). |
| Pure text ops | `vix-align` (align lines on a delimiter), `vix-textops` (line-ending convert / squeeze blanks / ROT13 / hard wrap, plus cursor-relative rewrites: increment number, smart toggle, transpose chars/words/lines/sentences/paragraphs/sections, wrap paragraph, `sentence_starts`, `tag_column`), `vix-case` (selection case transforms), `vix-emmet` (abbreviation → HTML), `vix-tags` (HTML/XML matching-tag jump). Pure `text → text` / offset helpers with unit tests, driven from Edit/Go/Tools actions. |
| Pure list ops | `vix-list-state` (T144: `up`/`down`/`page_up`/`page_down`/`select_index`/`ensure_visible` — shared `selected`/`scroll` arithmetic for a scrollable single-selection list; plain functions over `usize`s, not a struct panels adopt, so every panel keeps its own field names — see `crates/vix-list-state/spec/index.md`). |
| Networking  | `vix-http-client` (`.http`-buffer parser + blocking `ureq` send; response into a tab). |
| Undo store  | `vix-undo-store` (persist/restore the undo tree per file under `<config>/undo/`, content-hash guarded). |
| Clipboard   | `vix-clipboard` (process-wide serialized clipboard access; the platform pasteboard is opt-in through `use_system`, so a test run never touches it). |
| Themes      | `vix-theme` (Nerd Font icons + theme style helpers), `vix-theme-model` (the JSON theme model + serialization), `vix-base16` (bundled base16 color themes), `vix-theme-editor-panel` (T202: **Vix → Theme → Edit Theme…** — list a theme's 15 color slots, edit each via the existing X11 color picker, live preview, Save As). |
| Edit surfaces | `vix-edit-value` (JSON/YAML tree, `Tree` + `Format`), `vix-edit-bytes` (hex/ASCII byte editor, `Hex`), `vix-edit-sql` (SQL statement list, `Editor`), `vix-edit-outline` (T152: prose-hierarchy outline, `Tree` + `Outcome`), `vix-edit-table` (T152: CSV/TSV spreadsheet grid, `Grid` + `Outcome`). Overlay editors with their own `handle_key`/`Outcome`, under **Edit → Mode**. (`vix-column-view`, above, is this family's Org-specific sibling.) |
| Generators  | `vix-qr-tool` (QR code via the `qrcode` crate, Unicode renderer), `vix-lorem` (deterministic lorem-ipsum text). |
| Tool dialogs| `vix-calculator-tool`, `vix-color-converter-tool`, `vix-unit-converter-tool`, `vix-pomodoro-tool`. |
| Info panels | `vix-text-information-panel`, `vix-file-information-panel`, `vix-system-information-panel`, `vix-status-bar-panel`, `vix-workspace-dashboard-panel`, `vix-outline-panel`, `vix-welcome-panel`. |
| Menu / find | `vix-menu` (3-level dropdown + command mode), `vix-palette` (command palette / fuzzy), `vix-find-panel` (find/replace state + engine), `vix-workspace-search` (T152: workspace-wide search/replace panel state, across every file under the workspace root — `App` drives the actual scan), `vix-structural-replace` (T201: `$X`/`$$X`-hole pattern matching, token/bracket-based rather than regex or tree-sitter). Interactive step-through query-replace (`Decision`, `QueryReplace`) lives directly in `src/app.rs` (T151: folded in from a former single-purpose crate whose sole consumer was always the App shell — see `spec/find-and-replace/index.md`); its structural sibling `StructuralReplace` lives there too. |
| Pickers     | `vix-ascii-character-picker`, `vix-html-character-picker`, `vix-nerd-font-picker`, `vix-x11-color-picker`. |
| Boxes       | `vix-calendar-panel`, `vix-clock-panel`.                                       |
| Contacts    | `vix-vcard-parser` (RFC 6350), `vix-vcard-panel`, `vix-contact-panel`.         |
| Docks       | `vix-left-dock` (explorer), `vix-right-dock` (messages), `vix-bottom-dock` (output buffer). |
| Files / ops | `vix-fileops` (explorer copy/cut/paste/delete filesystem helpers), `vix-file-browser-panel` (File → Open… browser: walkdir listing + fuzzy/glob/ext search, sort, filters). |
| Models      | `vix-keymap-model`, `vix-locale-model`, `vix-theme-model`, `vix-time-zone-model`. |
| Help        | `vix-keyboard-shortcut-panel`, `vix-action-catalog` (T147: `(action id -> i18n title key)` catalog for every `run_action` id titled nowhere else — no `vix-menu` leaf and no `palette::COMMANDS` entry. `App::action_title` (F1 help, the keybinding editor) falls back to it before the raw id; the command palette's `>` mode appends it directly to `COMMANDS`; `shortcuts_for`-based UI, once any exists, can pair its results with it the same way). |
| i18n        | `vix-i18n` (embedded rust-i18n catalog).                                      |

## Other top-level paths

| Path            | Contents                                                            |
| --------------- | ------------------------------------------------------------------- |
| `crates/`       | The 112 `vix-*` workspace member crates (each with its own `spec/`).|
| `langs/`        | Tree-sitter highlight queries (`<lang>/highlights.scm`), embedded.  |
| `locales/`      | `app.yml` — rust-i18n translations (English fallback).              |
| `dictionaries/` | Hunspell dictionaries — gitignored; see `crates/vix-spellcheck/spec/dictionaries`. |
| `themes/`       | Bundled JSON color themes.                                          |
| `spec/`         | Cross-cutting / app-level and build/meta specs (per-crate specs live in `crates/<crate>/spec/`). |
| `docs/`         | Architecture, keybindings, themes, i18n, LSP, configuration, panels. |
| `examples/`     | `headless_edit.rs`, `list_commands.rs`.                             |
| `tests/`        | `tests/integration/main.rs` (+ 15 area submodules: `catalog`, `db`, `editing`, `find`, `git`, `keybindings`, `keymaps`, `lsp`, `menu`, `org`, `palette`, `panels`, `scripting`, `workspace`, `common`), `db_smoke.rs`, `lsp_smoke.rs`, `i18n_keys.rs`, `example_scripts.rs`, `snapshots.rs` (+ `snapshots/*.snap`) — terminal-independent tests. |
| `fuzz/`         | `cargo-fuzz` targets over the pure text/parse cores.                |
| `benches/`      | Criterion benchmarks (`cargo bench`).                               |
| `scripts/`      | `check` (the CI-parity gate) and `check-docs` (documentation integrity). |
| `vixide.github.io/` | The public GitHub Pages site (SvelteKit + Lily Design System), a monorepo subproject — see `spec/monorepo-github-pages/index.md`. Maintained here; published to the read-only sibling repo `git@github.com:vixide/vixide.github.io.git` via `git subtree push`, never edited there directly. |
