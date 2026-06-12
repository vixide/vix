# Changelog

All notable changes to Vix are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project aims to
follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **Spellcheck autodetects Hunspell dictionaries** from the platform's standard
  locations (`/usr/share/hunspell`, `/Library/Spelling`,
  `/opt/homebrew/share/hunspell`, `$XDG_DATA_HOME/hunspell`, and `hunspell -D`).
  The `dictionaries_dir` setting is replaced by `dictionary_path` (an extra
  directory to search; empty = autodetect only); both the standard
  `<name>.{aff,dic}` and wooorm `<name>/index.{aff,dic}` layouts are accepted.
- **Bottom-dock scrollback is configurable** via the `scrollback` setting
  (default 1000 lines, down from a hard-coded 5000); the oldest lines are dropped
  past the limit.
- **Redo shortcut is now `Ctrl+Shift+Z`** (was `Ctrl+Y`), matching the common
  undo/redo pairing.

### Added

- **Project Dashboard** (Tools → Project Dashboard): a live overlay showing the
  project folder name, disk usage (`du`), file count, and git commit count, each
  computed asynchronously and filled in as it completes. New internal
  `vix-project-dashboard-panel` crate.
- **Case transforms** (Edit → Case): convert the selection to Upper, Lower,
  Title, Kebab (`foo-bar`), Snake (`foo_bar`), Camel (`fooBar`), or Pascal
  (`FooBar`).
- **Project search path filters**: the project-wide search/replace panel gains
  **Include path** and **Exclude path** regex fields that narrow the searched
  files by their project-relative path (`Tab` cycles to them).
- **Git integration** via the new `vix-git` crate, shelling out to the `git`
  CLI. The status bar shows the current branch and a dirty dot; the file explorer
  shows colored M/A/?/D/R/U badges on changed files; the editor draws a colored
  diff gutter (added/modified/deleted) against HEAD. The **Git** menu offers a
  **Changes…** panel to stage/unstage files (`Space`/`s`/`u`) and commit (`c`),
  **Switch Branch…**, and **Pull / Push / Fetch** (streamed to the bottom dock).
- **Spell checking** (View → Editor → Toggle Spellcheck): underlines misspelled
  words in comments and string literals in red, using Hunspell dictionaries from
  the `dictionaries/<locale>/` directory (`dictionaries_dir` setting) via the new
  pure-Rust `vix-spellcheck` crate. The language follows the UI locale; code-like
  tokens (acronyms, camelCase identifiers) are skipped. Off by default. With the
  cursor on a misspelled word, **`Ctrl+;`** opens a suggestions popup with
  replace, add-to-dictionary, and ignore actions.
- **System Information panel** (Tools → System Information): a scrolling,
  read-only snapshot of the host — OS, CPU, memory, swap, disks, uptime, and
  environment (via the `sysinfo` crate). Enter or a click inserts the highlighted
  value into the editor; Esc closes. Lives in the new internal
  `vix-system-information-panel` crate.
- **Unsaved-changes prompt.** Closing a tab or quitting with unsaved changes now
  raises a modal asking to **(s)ave**, **(d)on't save**, or **(c)ancel**. Quit
  walks every dirty tab in turn before exiting. Vim `:q!` still force-quits
  without prompting.
- **ASCII panel** (Tools → ASCII): a scrolling overlay of the 128 ASCII codes
  showing each code's decimal, hexadecimal, and character representation. Arrow
  keys / PageUp / PageDown / Home / End move the highlight; Enter or a click
  inserts the highlighted character into the active editor; Esc closes. Lives in
  the new internal `vix-ascii-panel` crate.
- **View → Layout submenu.** The dock and status-bar toggles (Show/Hide Left
  Dock, Right Dock, Bottom Dock, Bottom Status) now live under a **Layout**
  submenu, alongside the existing **Editor** submenu.
- **Menu type-ahead.** With a menu open, typing a letter jumps to the next item
  whose label starts with it, cycling — e.g. in File, `S` → Save, `S` → Save As.
  Works inside an open submenu too.
- **Search in Project → Dock** (Edit → Find submenu, or the palette): search
  every project file for a term and list the hits in the bottom dock as
  `path:line:col` lines — each one click-to-jumps to the match. In the prompt,
  `Alt+C` toggles case-sensitivity and `Alt+R` toggles regex.
- **Run Command** (Tools → Run Command…, or the palette): prompt for a shell
  command, run it in the project root in a **background thread**, and **stream**
  its merged stdout/stderr into the bottom dock (shown automatically) line by
  line, with a `$ command` header and an `[exit N]` footer. The UI stays
  responsive; **Cancel Command** (Tools menu / palette) kills a running command.
- **Resizable bottom dock.** The bottom dock is pinned directly above the status
  bar, and its top edge is draggable to grow or shrink it (persisted in the
  `bottom_dock_height` setting), matching the draggable left/right docks.
- **Bottom-dock scrolling, focus & click-to-jump.** Click the bottom dock to
  focus it (its border brightens); then `↑`/`↓`, `PgUp`/`PgDn`, and `Home`/`End`
  scroll its buffer. The mouse wheel scrolls it any time; `Esc` returns focus to
  the editor. Clicking a line that names a `path:line[:col]` location (a build
  error, grep hit, …) opens that file there, making Run Command output
  actionable.
- **Bottom dock** (View → Show/Hide Bottom Dock, or the palette;
  `show_bottom_dock` setting, default off): a full-width scrollable line buffer at
  the bottom of the body for log messages, command/terminal output, data views,
  etc. State lives in the new `vix-bottom-dock` crate (line buffer + scroll).
- **Calendar month-nav arrows.** The calendar box's month header shows
  `◀ Month Year ▶`; the arrows are clickable (and mirror the `←`/`→` keys), and a
  bottom help line shows `◀ ▶ month   Esc close`.
- **Many more UI languages.** Added Italian, Korean, Turkish, Dutch, Vietnamese,
  Indonesian, Thai, Persian, Ukrainian, and Greek — plus **Klingon** (`tlh`) and
  **Sindarin** (`sjn`) — for 27 selectable languages. The full menu bar is
  translated into the 15 primary locales; other keys fall back to English.
- **Calendar click-to-insert.** In the calendar box, clicking one of the
  date-time lines (local date-time, UTC ISO instant, ISO week date) inserts that
  string into the editor; clicking a day in the month grid inserts that date
  formatted per the active locale. The box stays open for repeated inserts; a
  click outside closes it.
- **Nested submenus** in the menu bar. **View → Editor** groups the editor
  display toggles (line numbers, visible whitespace, scroll bar); **Edit → Find**
  groups the find-related items (Find, Find Next, Find Previous, Find Selection,
  Find & Replace). Arrow keys / clicks open and navigate submenus (Right or a
  click opens, Left or Esc backs out).
- **Show/Hide Editor Scroll Bar** (View → Editor, or the palette): toggle the
  editor's right-side scroll bar; the text reclaims the column when hidden.
  Persists in the `show_scrollbar` setting (default on).
- **Reopen Closed Tab** (`Ctrl+Shift+T`, File menu, or the palette): reopen the
  most recently closed file (remembers a stack of recently closed paths).
- **Close All Tabs** (File menu, after Close, or the palette): close every open
  buffer, leaving a single empty untitled buffer.
- **Find Next / Find Previous / Find Selection** in the Edit menu (after Find).
  Find Next (`Ctrl+G`) and Find Previous (`Ctrl+Shift+G`) repeat the last search
  — and now keep working **after the find box is closed** (the last pattern is
  remembered; `F3` / `Shift+F3` repeat it too). Find Selection jumps to the next
  occurrence of the selection (`Alt+N`).
- **Toggle Bottom Status** (View menu / palette / `view.status_bar`): show or
  hide the bottom status bar; the editor body reclaims the row when it is hidden.
  Persists in the `show_status_bar` setting (default on).
- **Editing comforts.** **Select All** (`Ctrl+A`, Edit menu, or the palette),
  **Duplicate Line** (`Ctrl+D` or the palette), **Move Line Up/Down**
  (`Alt+↑`/`Alt+↓` or the palette), and **Jump to Matching Bracket** (`Ctrl+]` or
  the palette). Auto-indent on Enter (carry the previous line's leading
  whitespace) was already present and is now covered by tests.
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
- **Themes.** All themes are **JSON themes** with per-region RGB colors (menu
  bar, status bar, left/right dock, editor) and optional editor cursor and syntax
  colors. Dark and Light ship bundled; more are bundled too, and users can add
  their own in `~/.config/vix/themes/*.json`. Chosen live in **View → Theme…**.
  See `docs/themes.md`.
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
  `vix-keyway-chooser`, `vix-keyboard-shortcut-panel`,
  `vix-date-time-calendar-panel`, `vix-nerd-font-palette`, and `vix-find-panel`
  (the find / find-and-replace box state).
- New docs: `docs/themes.md`, `docs/i18n.md`, `docs/configuration.md`,
  `index.md`, `AGENTS.md` (+ `AGENTS/`), and this changelog.

### Changed

- **Docks and status bar extracted to internal crates.** The left dock (file
  explorer) moved to `vix-left-dock`, the right dock (message drawer) to
  `vix-right-dock`, and the status-bar segment formatting to
  `vix-status-bar-panel`. The app re-exports them; behavior is unchanged.
- The main panes use a lighter border frame: the left and right docks keep only
  their inner (top + side-facing-the-editor) borders, the center editor keeps
  only its top border, and the bottom status bar gains a full-width top border
  that separates it from the body.
- The editor widget crate was renamed `ratatui-code-editor` →
  `vix-editor` and made **theme-aware** (configurable text,
  line-number, selection, and cursor styles, and a settable syntax palette).
- The calendar logic moved into `vix-date-time-calendar-panel` and gained
  month navigation (Left/Right while the calendar is open).
- **Every theme is now a JSON theme.** The hardcoded monochrome Dark/Light
  *modes* were removed; **Dark** and **Light** are now ordinary bundled themes
  (`themes/dark.json` / `themes/light.json`, soft `[215,215,215]` on `[40,40,40]`
  and its inverse) loaded like any other. The chooser lists every theme
  (including Dark and Light) sorted by name, and the persisted `theme` setting is
  the theme's name.
- Settings moved from hand-rolled JSON to `confy` TOML.
- All public items are documented (`#![deny(missing_docs)]`); the crate forbids
  `unsafe`.

### Fixed

- Panel border lines now use each pane's own foreground color (via
  `region_title`) instead of the global editor foreground, so borders match the
  pane under themes whose regions use different colors.
- In the find / replace box, clicking the Find or Replace field now focuses it
  (previously the box swallowed all mouse input, so the Replace field was only
  reachable with `Tab`, which the hint never mentioned). The hint now states
  `Tab / click: switch field` in replace mode.
- In the file explorer, `←` (Left) no longer expands a collapsed folder. It now
  collapses an expanded folder, or jumps to the parent folder when the selection
  is already collapsed — it never opens a folder.
- Duplicating the last line of a buffer with no trailing newline (`Ctrl+D`) now
  produces a real second line instead of concatenating the copy onto the
  original. Line-boundary detection at end-of-buffer was off by one, which also
  affected `Ctrl+K` (delete line) and triple-click line selection on the last
  line.
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
