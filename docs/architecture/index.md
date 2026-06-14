# Vix Architecture

## Workspace shape: application crate + internal crates

Vix is a Cargo workspace. The `vix` crate is the application: a library
(`src/lib.rs`) with a thin binary (`src/main.rs`) on top. The binary owns only
the CLI, the terminal lifecycle, and the event loop; everything else is a public
module in the library. This keeps the editing logic terminal-independent, so it
can be unit-tested and driven from examples without a real TTY (see
`tests/integration.rs` and `examples/`).

Several self-contained concerns are split into their own small internal crates,
each independently testable and free of UI-framework or i18n coupling unless it
genuinely needs it:

| Crate                            | Responsibility                                                        |
| -------------------------------- | --------------------------------------------------------------------- |
| `vix-editor`          | Vix's fully-custom center editor widget: Tree-sitter highlighting, history, selection, clipboard, mouse, **soft wrap**, and **themeable** text/line-number/selection/cursor styles. The highlighting engine was adapted from `ratatui-code-editor`; grammars are gated behind features. |
| `vix-calendar-panel`   | Calendar date/time strings and the navigable Monday-first month grid (owns the `jiff` dependency). |
| `vix-theme-chooser`              | The theme model: monochrome Dark/Light modes, the ratatui styles derived from them, **custom JSON themes** (per-region RGB), and chooser state. |
| `vix-locale-chooser`             | The list of available UI languages and chooser state.                 |
| `vix-keymap-chooser`             | The keyboard navigation styles (Apple / Emacs / Vim) and chooser state. |
| `vix-keyboard-shortcut-panel`    | The keyboard-help rows (key combo + i18n description key).            |

The pattern for the panel/chooser crates is **data and logic in the crate, host
renders**: each exposes pure state and helpers; `src/ui.rs` draws them. The crates
return i18n *keys* (not translated text) so they need no localization dependency
— the host translates. The one exception is `vix-theme-chooser`, which depends on
`ratatui-core` because its job is to produce ratatui `Style`s; those types are the
same ones `ratatui` re-exports, so they flow straight into the app's rendering.

## Application modules (`src/`)

| Module           | Responsibility                                                       |
| ---------------- | -------------------------------------------------------------------- |
| `app`            | The `App` struct: all state, event routing, and action dispatch      |
| `editor`         | `Editor` and `Tab`: buffers over the editor widget; open/save/goto   |
| `explorer`       | `Explorer`: a lazily-expanded directory tree, flattened to rows      |
| `menu`           | Menu-bar definitions (i18n-keyed) and `Menu` dropdown state          |
| `palette`        | `Palette`, mode detection, fuzzy matching, `path:line:col` parsing   |
| `search`         | `SearchBar`: query/replace/toggles; builds the regex pattern         |
| `workspace_search` | `WorkspaceSearch`: the workspace-wide search/replace panel state         |
| `query`          | `QueryReplace`: interactive step-through replace session             |
| `messages`       | `Messages`: the notifications drawer model                           |
| `fileops`        | Filesystem helpers for explorer copy/cut/paste/delete                |
| `settings`       | confy-backed `Settings`; the custom-themes directory                 |
| `theme`          | Nerd Font icon constants + re-export of `vix-theme-chooser`          |
| `ui`             | Pure rendering: lays out the frame and draws each pane/overlay       |

## Event flow

```
main()                         ui::draw(&app, frame)         App::on_key(event)
  │                                   ▲                              │
  ├── Cli::parse() (clap)             │                              │
  ├── Settings::load() (confy)        │                              │
  ├── rust_i18n::set_locale(...)      │                              ▼
  ├── ratatui::init()                 │                    ┌─ modal layers ─┐
  │                                   │                    │ help            │
  └── loop:                           │                    │ calendar nav    │
        terminal.draw(draw) ──────────┘                    │ theme chooser   │
        if should_quit: break                              │ locale chooser  │
        if poll(500ms):                                    │ query-replace   │
            on_key(read()) ─────────────────────────────▶  │ workspace search  │
                                                           │ confirm / paste │
                                                           │ prompt          │
                                                           │ palette         │
                                                           │ search          │
                                                           │ menu            │
                                                           ├─ keymap dispatch
                                                           └─ focused pane:
                                                              editor/explorer/
                                                              messages
```

`App::on_key` resolves input in strict priority order: each modal overlay
consumes input while open (the theme, locale, and keymap choosers are overlays in
this chain). With no modal active, the active **keymap** dispatches the key —
Apple modifier shortcuts, Emacs `Ctrl` chords, or Vim modal motions, all routing
through `run_action` and the editor's own handling — and anything it does not
consume routes to the focused pane.

Mouse events arrive as `Event::Mouse` and go to `App::on_mouse`, which hit-tests
against the pane rectangles `ui::draw` records each frame (`app.layout`). Editor
clicks/drag/wheel are forwarded to the editor widget's own `mouse` handler;
explorer/messages/tab/menu clicks map to the corresponding row or item, and the
menu bar's right-edge dock-toggle icons toggle the drawers.

Menu items and palette `>`-commands share one set of **action identifiers**
(strings like `file.save`, `view.theme`, `view.locale`, `view.keymap`). Both funnel through
`App::run_action`, so a command has exactly one implementation regardless of how
it is invoked.

The event loop polls with a 500 ms timeout rather than blocking, so the calendar
clock keeps ticking while the editor is idle.

## Rendering

`ui::draw` first paints the whole frame with the theme background, then lays out
three vertical bands — menu bar, body, status bar — and splits the body
horizontally into explorer / editor / messages according to which drawers are
visible. The editor band is itself split into a tab bar and the text area plus a
`Scrollbar`. The text area is handed to the **`vix-editor`** widget, which renders
itself — including syntax highlighting, the block cursor, visible-whitespace
glyphs, **bracket matching**, and **soft wrap** (a shared visual-row layout drives
its renderer, cursor scroll, and mouse hit-testing). The status bar shows the
keymap mode indicator (Vim's `-- NORMAL --` / `-- INSERT --` / `:` line, or
Emacs's pending `Ctrl+X-` prefix) plus the buffer's language, line ending
(LF/CRLF), encoding, selection char/line count, and line:column.
Overlays (calendar, menu dropdown, search, palette, prompt, dialogs, and the
theme / locale / keymap / recent choosers, …) are drawn last, each clearing its
rectangle with `Clear` and painting a bordered box in the theme background so it
reads correctly in either light or dark mode.

## Theming

The theme model lives in `vix-theme-chooser`. Two built-in modes are strictly
monochrome (one foreground, one background; emphasis via dim and full intensity,
no bold or italic; reversed video only for selections and the cursor). A
process-global holds the active mode so the static style helpers (`fg`, `bg`,
`base`, `title`, `selected`, `dim`) need no threading.

Custom themes are JSON files providing **per-region** RGB colors (menu bar,
status bar, left/right dock, editor) plus optional editor cursor and syntax
colors. When a custom theme is active, `region_fg`/`region_bg`/`region_base`
return its colors (falling back to the monochrome default for any unspecified
channel), and the editor widget is given the custom syntax palette and cursor
color. The theme chooser lists the two built-ins followed by every discovered
custom theme. See [themes.md](themes.md).

## Internationalization

User-facing text is looked up at render time with `rust_i18n`'s `t!` macro
against `locales/app.yml` (one file, all languages, keyed by a dotted name).
English is the fallback. The macro is initialized once in `src/lib.rs`. Data
crates (menus, palette, theme, keyboard help) store i18n *keys*; the host
translates. The active locale is a process-global set via `rust_i18n::set_locale`
— resolved at startup from `--locale` or the `locale` setting, and switchable
live in **View → Locale…**. See [i18n.md](i18n.md).

## Configuration

`Settings` is a serde struct persisted with `confy` as TOML under the platform
config directory. `main` loads it before building the `App` so the saved theme
and language apply before any UI text is produced. A `--locale` flag overrides
the saved language for one run without persisting. See [configuration.md](configuration.md).

## Dependency version pinning (one ratatui only)

The `ratatui` widget ecosystem is **not** cross-version compatible: a widget
built against one `ratatui` major cannot be rendered into another's `Frame`,
because the `Widget`/`Buffer`/`Rect` types differ. So every ratatui-based crate
in the tree must agree on the version. The editor widget tracks `ratatui` **0.30**
(via `ratatui-core`), which pins the stack:

- `ratatui = 0.30`, `crossterm = 0.29` (the version both `ratatui` 0.30 and the
  editor widget use, so the `KeyEvent`/`MouseEvent` types match exactly).
- The explorer, scrollbar, popups, menu, and palette are built directly on
  `ratatui` primitives (`List`, `Scrollbar`, `Clear`, `Tabs`).
- Date/time uses `jiff` only, and the month grid is rendered in-house.

Historical note: the first cut used `tui-textarea` (`ratatui` 0.29). Moving the
editor to the Tree-sitter widget brought syntax highlighting and built-in mouse
handling and lifted the whole stack to 0.30.

## Testing strategy

Because the logic is terminal-independent, `tests/integration.rs` constructs an
`App`, feeds it synthetic `crossterm` `KeyEvent`s, and asserts on the resulting
state — typing, open/save round trips, tab lifecycle, go-to-line, fuzzy matching,
the search-pattern builder, end-to-end regex replace with capture groups,
theme/locale switching, and date formatting. A couple of unit tests render into a
sized in-memory `TestBackend` to check panel output. Each internal crate adds its
own focused unit tests. Run everything with `cargo test --workspace`.
