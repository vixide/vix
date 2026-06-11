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
| `vix-find-panel`               | `vix-find-panel/`               | Find / find-and-replace box state + effective-pattern builder.      |

## `vix-editor` modules (`vix-editor/src/`)

The crate keeps a reused **engine** (allowed `clippy::all`, upstream style) and
**Vix-owned** modules (held to `clippy::pedantic` via their own inner attributes).

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
| `app.rs`            | `App` state, `on_key`/`on_mouse`, `run_action`, all behavior.       |
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
| `locales/`    | `app.yml` — rust-i18n translations (en/es/fr/de/cy).     |
| `spec/`       | Specification (source of truth).                         |
| `docs/`       | Architecture, keybindings, themes, i18n, configuration.  |
| `examples/`   | `headless_edit.rs`, `list_commands.rs`.                  |
| `tests/`      | `integration.rs` — terminal-independent tests.           |
