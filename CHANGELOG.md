# Changelog

All notable changes to Vix are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project aims to
follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Internationalization** via `rust-i18n`. The entire UI is translatable; 15
  languages are selectable (English, Spanish, French, German, Welsh fully
  translated; Irish, Scottish Gaelic, Polish, Portuguese, Russian, Arabic, Hindi,
  Bengali, Chinese, Japanese with menu/theme coverage and English fallback).
  Language is chosen with `--locale`, the `locale` setting, or **View → Locale…**
  (a live chooser). English is the fallback. See `docs/i18n.md`.
- **Themes.** Two built-in monochrome themes (Dark, Light) plus **custom JSON
  themes** loaded from `~/.config/vix/themes/*.json`, with per-region RGB colors
  (menu bar, status bar, left/right dock, editor) and optional editor cursor and
  syntax colors. Chosen live in **View → Themes…**. See `docs/themes.md`.
- **Configuration** via `confy`, stored as TOML in the platform config directory.
  New `theme` and `locale` settings. See `docs/configuration.md`.
- **Command-line interface** via `clap`: positional files (with optional
  `path:line:col`) and a `--locale` flag; `--help` / `--version`.
- **Vix menu** (first in the bar) with **About Vix** (shows `Vix <version>`),
  **Website**, and **Email** — each opens a modal dialog with an **Ok** button.
  The Website/Email dialogs show the text in a selectable text field (drag or
  arrow-select, `Ctrl+C` to copy).
- **View menu** with theme and locale choosers and the drawer/line-number toggles.
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
  Monokai, One Dark, Tokyo Night) that appear in **View → Themes…** with no
  installation. A same-named theme in `~/.config/vix/themes/` overrides a
  bundled one.
- New internal crates: `vix-theme-chooser`, `vix-locale-chooser`,
  `vix-keyboard-shortcut-panel`, and `vix-date-time-calendar-panel`.
- New docs: `docs/themes.md`, `docs/i18n.md`, `docs/configuration.md`,
  `index.md`, `AGENTS.md` (+ `AGENTS/`), and this changelog.

### Changed

- The editor widget crate was renamed `ratatui-code-editor` →
  `vix-code-editor-panel` and made **theme-aware** (configurable text,
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

- Search-hit marks render monochrome (underline) instead of a hard-coded color.
- Overlays paint the theme background so they read correctly in the light theme.
- The menu dropdown no longer shows its raw i18n key as a title.
- Clicking an item in an open menu dropdown now runs it (and clicking away
  closes the menu).
- The tab bar paints the editor's theme background instead of resetting to the
  terminal default (it no longer shows white under a dark theme).
- Removed the gray app-name label from the right of the menu bar.
