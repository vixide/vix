# Changelog

All notable changes to Vix are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project aims to
follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Nerd Font Palette** (Tools → Nerd Font Palette…, crate
  `vix-nerd-font-palette`): a character picker showing a grid of curated Nerd
  Font glyphs. Browse with the arrow keys or the mouse; Enter or a click inserts
  the highlighted glyph into the active editor and leaves the palette open so
  several can be picked in a row; Esc closes it.
- **Menu separators.** Dropdowns group related items with non-selectable
  divider lines: in File (before Open, Close, Quit), Edit (before Cut, Toggle
  Comment, Find), and View (before Toggle Left Dock, Toggle Editor Line Numbers).
  Keyboard navigation, hover, and clicks skip separators.
- **Bracket matching.** When the cursor is on (or just after) a bracket
  `()[]{}`, its matching partner is highlighted. No auto-insertion of pairs.
- **Richer status bar.** The status bar now shows the language, line ending
  (LF/CRLF), encoding (UTF-8), and — when text is selected — the selected
  character and line count, alongside the existing line:column.
- **Fully-custom editor widget (`vix-editor`) with soft wrap.** The editor was
  migrated from the vendored `vix-code-editor-panel` fork to an in-house widget:
  the Tree-sitter highlighting + buffer + undo/redo engine is reused, while the
  editor state, input, mouse, and renderer are owned by Vix. The renderer now
  supports **soft wrap** — toggle with **View → Toggle Soft Wrap** (or the
  palette); the `soft_wrap` setting persists. Long lines wrap across screen rows
  with cursor, scroll, and mouse all wrap-aware. (Also fixed a latent panic when
  jumping to a line past the end of the buffer.)

- **Internationalization** via `rust-i18n`. The entire UI is translatable; 15
  languages are selectable (English, Spanish, French, German, Welsh fully
  translated; Irish, Scottish Gaelic, Polish, Portuguese, Russian, Arabic, Hindi,
  Bengali, Chinese, Japanese with menu/theme coverage and English fallback).
  Language is chosen with `--locale`, the `locale` setting, or **View → Locale…**
  (a live chooser). English is the fallback. See `docs/i18n.md`.
- **Themes.** Two built-in monochrome themes (Dark, Light) plus **custom JSON
  themes** loaded from `~/.config/vix/themes/*.json`, with per-region RGB colors
  (menu bar, status bar, left/right dock, editor) and optional editor cursor and
  syntax colors. Chosen live in **View → Theme…**. See `docs/themes.md`.
- **Configuration** via `confy`, stored as TOML in the platform config directory.
  New `theme` and `locale` settings. See `docs/configuration.md`.
- **Command-line interface** via `clap`: positional files (with optional
  `path:line:col`) and a `--locale` flag; `--help` / `--version`.
- **Vix menu** (first in the bar) with **About Vix** (shows `Vix <version>`),
  **Website**, and **Email** — each opens a modal dialog with an **Ok** button.
  The Website/Email dialogs show the text in a selectable text field (drag or
  arrow-select, `Ctrl+C` to copy).
- **Keyways** (**View → Keyway…**, crate `vix-keyway-chooser`): choose the
  keyboard navigation style, which changes how keys are dispatched. The choice
  persists (`keyway` setting); Apple is the default.
  - **Apple** — modifier shortcuts (e.g. `Ctrl+O` open, `Ctrl+Q` quit).
  - **Emacs** — `Ctrl` chords and the `Ctrl+X` prefix: `Ctrl+X Ctrl+F` open,
    `Ctrl+X Ctrl+S` save, `Ctrl+X Ctrl+C` quit, `Ctrl+X k` close; cursor motion
    with `Ctrl+F/B/N/P/A/E/V`, `Ctrl+D` delete, `Ctrl+S` find, `Ctrl+G` cancel.
  - **Vim** — modal: a Normal mode (`h/j/k/l`, `0`, `$`, `x`, `i/a/o/O` to enter
    Insert, `Esc` back to Normal) and a `:` command line (`:w`, `:q`, `:q!`,
    `:wq`/`:x`, `:Ex`). The status bar shows the current mode.
- **View menu** with theme, locale, and keyway choosers and the drawer/line-number
  toggles.
- **Indentation settings** — `indent_style` (`"spaces"` / `"tabs"`) and
  `tab_width` control what the Tab key inserts (default: 4 spaces), overriding the
  editor widget's per-language default.
- **Live go-to-line preview** — in the palette's `:` mode the cursor now follows
  the line number as you type (scrolling it into view); `Enter` commits (recording
  the original position in the jump history) and `Esc` reverts. (Also fixes a
  latent panic when jumping to a line past the end of the buffer.)
- **Find occurrence of selection** (`Alt+N` / `Alt+P`, or the palette): jump to
  the next/previous occurrence of the current selection — or the word under the
  cursor when there is no selection — without opening the search bar.
- **Smart Home** — `Home` jumps to the first non-blank character of the line;
  pressing it again jumps to column 0 (toggling between the two).
- **On-save normalization** — two settings (`trim_trailing_whitespace`,
  `ensure_final_newline`, both default on) strip trailing spaces/tabs from each
  line and append a final newline when saving. (Making the previously
  always-on final-newline behavior configurable.)
- **Toggle Comment** (`Ctrl+/`, the Edit menu, or the palette): comment or
  uncomment the cursor line or every line in the selection, using the language's
  comment token (`//`, `#`, `--`), as a single undoable edit. The editor widget's
  comment-token map gained TOML/YAML (`#`) and SQL (`--`).
- **Go to Symbol in File** — a new command-palette mode (`@` prefix, or the
  "Go to Symbol in File" command) listing the current file's declarations
  (functions, types, classes, traits, modules, `#define`s, …) to fuzzy-filter
  and jump to. A fast, offline, language-agnostic heuristic — no language server.
- **Open Recent** (`File → Open Recent…`, `Ctrl+Shift+O`, or the palette): a
  chooser of recently opened files. The list (most-recent first, de-duplicated,
  capped at 15) persists in the `recent_files` setting.
- **Toggle Editor Visible Whitespace** (View menu / palette / `view.whitespace`):
  render dim glyphs for space (`·`), tab (`→`), carriage return (`␍`), and line
  ending (`¶`). Off by default; persists in the `show_whitespace` setting.
- **Dock toggle icons** in the menu bar (clickable explorer/messages toggles;
  bright when open, dim when closed).
- A visible **block cursor** in the editor, themeable via a custom theme's
  `cursor` color.
- Custom themes can set per-region **`font-style`** (`normal`/`italic`) and
  **`font-weight`** (`normal`/`bold`); the editor also applies a custom theme's
  syntax token colors.
- **Editor scrollbar drag**: press and drag the scrollbar thumb/track to scroll.
- **Resizable docks**: drag the explorer's right edge or the message drawer's left
  edge to resize them. The widths persist (`explorer_width` / `messages_width`).
- A collection of themes **bundled into the binary** (Darker, Darkest, Lighter,
  Lightest, Matrix, Turbo, Solarized Dark/Light, Dracula, Nord, Gruvbox Dark,
  Monokai, One Dark, Tokyo Night) that appear in **View → Theme…** with no
  installation. A same-named theme in `~/.config/vix/themes/` overrides a
  bundled one.
- New internal crates: `vix-theme-chooser`, `vix-locale-chooser`,
  `vix-keyway-chooser`, `vix-keyboard-shortcut-panel`, and
  `vix-date-time-calendar-panel`.
- New docs: `docs/themes.md`, `docs/i18n.md`, `docs/configuration.md`,
  `index.md`, `AGENTS.md` (+ `AGENTS/`), and this changelog.

### Changed

- The editor widget crate was renamed `ratatui-code-editor` →
  `vix-editor` and made **theme-aware** (configurable text,
  line-number, selection, and cursor styles, and a settable syntax palette).
- The calendar logic moved into `vix-date-time-calendar-panel` and gained
  month navigation (Left/Right while the calendar is open).
- The theme chooser lists all themes (built-in modes and JSON themes together)
  sorted alphabetically by canonical name.
- The theme system is monochrome by default (one foreground, one background;
  emphasis via dim and full intensity; **no bold or italic** in the built-in
  themes; reversed video only for selections and the cursor).
- Settings moved from hand-rolled JSON to `confy` TOML.
- All public items are documented (`#![deny(missing_docs)]`); the crate forbids
  `unsafe`.

### Fixed

- Menu dropdown items keep at least one space between the label and the
  right-aligned keyboard shortcut (the widest item used to let them touch).
- Keyboard-only modal overlays (the calendar box, find, query-replace, project
  search, confirm, paste-conflict) now swallow mouse clicks instead of letting
  them fall through to the editor underneath.
- Menu mouseover now moves the selection: with a dropdown open, hovering a row
  highlights it and hovering another top-level name switches menus (any-motion
  mouse tracking is enabled for this; other panes ignore button-less motion).
- The theme, locale, and keyway choosers now respond to the mouse: clicking a
  row highlights (and, for theme/locale, live-previews) that entry instead of
  being ignored.
- The active editor tab keeps the theme background (marked with an underline)
  instead of reversed video, which showed a light background under a dark theme.
- Search-hit marks render monochrome (underline) instead of a hard-coded color.
- Overlays paint the theme background so they read correctly in the light theme.
- The menu dropdown no longer shows its raw i18n key as a title.
- Clicking an item in an open menu dropdown now runs it (and clicking away
  closes the menu).
- The tab bar paints the editor's theme background instead of resetting to the
  terminal default (it no longer shows white under a dark theme).
- Removed the gray app-name label from the right of the menu bar.
