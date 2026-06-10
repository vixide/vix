# Crate and file map

Vix is a Cargo workspace. Shared reference for where everything lives.

## Workspace crates

| Crate                          | Path                            | Owns                                                                |
| ------------------------------ | ------------------------------- | ------------------------------------------------------------------- |
| `vix`                          | `./` (`src/`)                   | The application: state, routing, rendering, i18n, settings.         |
| `vix-code-editor-panel`        | `vix-code-editor-panel/`        | The center editor widget (Tree-sitter, history, themeable styles).  |
| `vix-date-time-calendar-panel` | `vix-date-time-calendar-panel/` | Calendar date/time + navigable month grid (owns `jiff`).            |
| `vix-theme-chooser`            | `vix-theme-chooser/`            | Theme model, ratatui styles, custom JSON themes, chooser state.     |
| `vix-locale-chooser`           | `vix-locale-chooser/`           | Available UI languages + chooser state.                             |
| `vix-keyway-chooser`           | `vix-keyway-chooser/`           | Keyboard navigation styles (Apple/Emacs/Vim) + chooser state.       |
| `vix-keyboard-shortcut-panel`  | `vix-keyboard-shortcut-panel/`  | Keyboard-help rows (key combo + i18n description key).              |

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
