# Crate and file map

Vix is a Cargo workspace. Shared reference for where everything lives.

## Workspace crates

| Crate                          | Path                            | Owns                                                                |
| ------------------------------ | ------------------------------- | ------------------------------------------------------------------- |
| `vix`                          | `./` (`src/`)                   | The application: state, routing, rendering, i18n, settings.         |
| `vix-editor`                   | `vix-editor/`                   | Fully-custom center editor widget (Tree-sitter, history, soft wrap, themeable styles). |
| `vix-date-time-calendar-panel` | `vix-date-time-calendar-panel/` | Calendar date/time + navigable month grid (owns `jiff`).            |
| `vix-theme-chooser`            | `vix-theme-chooser/`            | Theme model, ratatui styles, custom JSON themes, chooser state.     |
| `vix-locale-chooser`           | `vix-locale-chooser/`           | Available UI languages + chooser state.                             |
| `vix-keyway-chooser`           | `vix-keyway-chooser/`           | Keyboard navigation styles (Apple/Emacs/Vim) + chooser state.       |
| `vix-keyboard-shortcut-panel`  | `vix-keyboard-shortcut-panel/`  | Keyboard-help rows (key combo + i18n description key).              |
| `vix-nerd-font-palette`        | `vix-nerd-font-palette/`        | Curated Nerd Font glyph set + character-picker grid state.          |
| `vix-ascii-panel`              | `vix-ascii-panel/`              | ASCII reference table (dec/hex/char) + row-selection state.         |
| `vix-system-information-panel` | `vix-system-information-panel/` | Host OS/CPU/memory/disk snapshot (via `sysinfo`) + row state.       |
| `vix-find-panel`               | `vix-find-panel/`               | Find/replace box state + the search/replace engine (matches, replace_all, unescape) + path filters. |
| `vix-spellcheck`               | `vix-spellcheck/`               | Hunspell spell checking (via `spellbook`): dictionary discovery, check/suggest, misspelling tokenizer. |
| `vix-git`                      | `vix-git/`                      | Git status/diff/staging via the `git` CLI; diff marks via `similar`. |
| `vix-left-dock`                | `vix-left-dock/`                | Left-dock file-explorer tree state (lazy expand, selection).        |
| `vix-right-dock`               | `vix-right-dock/`               | Right-dock message-drawer state (advice/notifications + selection). |
| `vix-bottom-dock`              | `vix-bottom-dock/`              | Bottom-dock scrollable line buffer with configurable scrollback.    |
| `vix-status-bar-panel`         | `vix-status-bar-panel/`         | Status-bar left/right/git segment formatting.                       |

## `vix-editor` modules (`vix-editor/src/`)

The crate root carries `#![warn(clippy::pedantic)]`, so **Vix-owned** modules are
held to pedantic by default. The reused **engine** modules keep their upstream
style and carry `#[allow(clippy::all, clippy::pedantic)]` (both are listed because
`clippy::all` does not include `pedantic`).

| Module                 | Kind        | Owns                                                       |
| ---------------------- | ----------- | ---------------------------------------------------------- |
| `code`                 | engine      | Rope buffer + Tree-sitter parse/highlight + edit batches.  |
| `history`              | engine      | Undo/redo stacks.                                          |
| `selection`, `utils`   | engine      | Selection type; grapheme/width/indent/comment helpers.     |
| `actions`              | engine      | Editing operations (insert/delete/move/indent/comment/…).  |
| `editor`               | engine+Vix  | `Editor` state, public API, input, mouse (`cursor_from_mouse`). |
| `render`               | engine      | Non-wrap renderer + the render dispatch.                   |
| `wrap`                 | **Vix**     | Soft-wrap visual-row layout + wrapped renderer (pedantic). |
| `brackets`             | **Vix**     | Bracket matching (`matching_bracket`) (pedantic).          |
| `lines`                | **Vix**     | Move-line up/down (`move_line_up`/`down`) (pedantic).      |
| `editor_crossterm`     | engine      | `KeyEvent` → actions mapping (behind the `crossterm` feature). |
| `theme`                | engine      | Token-name → style helpers.                                |

## `vix` application modules (`src/`)

| File                | Responsibility                                                       |
| ------------------- | -------------------------------------------------------------------- |
| `main.rs`           | clap CLI, locale resolution, terminal setup, event loop.            |
| `lib.rs`            | Crate root; `i18n!` catalog init; module declarations; re-exports.  |
| `app.rs`            | `App` state, `on_key`/`on_mouse`, `run_action`, all behavior (incl. spellcheck, git, overlays). |
| `case.rs`           | Selection case transforms (upper/lower/title/kebab/snake/camel/pascal). |
| `editor.rs`         | `Editor`/`Tab`: buffers over the editor widget; open/save/goto.     |
| `explorer.rs`       | `Explorer`: directory tree flattened to rows.                       |
| `menu.rs`           | Menu definitions (i18n-keyed) + `Menu` dropdown state.              |
| `palette.rs`        | `Palette`, mode detection, fuzzy match, `path:line:col` parsing.    |
| `search.rs`         | `SearchBar`: find/replace toolbar state + pattern builder.          |
| `project_search.rs` | `ProjectSearch`: project-wide search/replace panel state.          |
| `query.rs`          | `QueryReplace`: interactive step-through session state.            |
| `messages.rs`       | `Messages`: notifications drawer model.                            |
| `fileops.rs`        | Explorer copy/cut/paste/delete filesystem helpers.                |
| `settings.rs`       | confy-backed `Settings`; themes directory locator.                |
| `theme.rs`          | Nerd Font icons + re-export of `vix-theme-chooser`.               |
| `ui.rs`             | All rendering: layout + per-pane/overlay draw functions.          |

## Other top-level paths

| Path          | Contents                                                  |
| ------------- | -------------------------------------------------------- |
| `locales/`    | `app.yml` — rust-i18n translations (27 languages, English fallback). |
| `dictionaries/` | Hunspell spell-check dictionaries — gitignored; see `spec/dictionaries.md`. |
| `spec/`       | Specification (source of truth).                         |
| `docs/`       | Architecture, keybindings, themes, i18n, configuration.  |
| `examples/`   | `headless_edit.rs`, `list_commands.rs`.                  |
| `tests/`      | `integration.rs` — terminal-independent tests.           |
