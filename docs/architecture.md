# STRIDE Architecture

## Crate shape: library + binary

STRIDE is a library crate (`src/lib.rs`) with a thin binary (`src/main.rs`) on
top. The binary owns only the terminal lifecycle and the event loop; everything
else is a public module in the library. This keeps the editing logic
terminal-independent, so it can be unit-tested and driven from examples without
spinning up a real TTY (see `tests/integration.rs` and `examples/`).

## Modules

| Module     | Responsibility                                                  |
| ---------- | -------------------------------------------------------------- |
| `app`      | The `App` struct: all state, event routing, and action dispatch |
| `editor`   | `Editor` and `Tab`: buffers over `ratatui-code-editor`; open/save/goto |
| `explorer` | `Explorer`: a lazily-expanded directory tree, flattened to rows |
| `menu`     | Static menu definitions and `Menu` dropdown state               |
| `palette`  | `Palette`, mode detection, fuzzy matching, `path:line:col` parse|
| `search`   | `SearchBar`: query/replace/toggles; builds the regex pattern    |
| `messages` | `Messages`: the notifications drawer model                      |
| `datetime` | `jiff`-based clock/ISO/week formatting and the month grid       |
| `settings` | serde-backed `Settings` persisted to `~/.config/stride`         |
| `theme`    | Colors and Nerd Font icon constants                            |
| `ui`       | Pure rendering: lays out the frame and draws each pane/overlay  |

## Event flow

```
main()                         ui::draw(&app, frame)         App::on_key(event)
  │                                   ▲                              │
  ├── ratatui::init() ────────────────┤                              │
  │                                   │                              ▼
  └── loop:                           │                    ┌─ modal layers ─┐
        terminal.draw(draw) ──────────┘                    │ prompt          │
        if should_quit: break                              │ palette         │
        if poll(500ms):                                    │ search          │
            on_key(read()) ─────────────────────────────▶  │ menu            │
                                                           ├─ global shortcuts
                                                           └─ focused pane:
                                                              editor/explorer/
                                                              messages
```

`App::on_key` resolves input in strict priority order. Modal overlays (the help
screen, Open/Save prompt, command palette, find/replace toolbar, and menu
dropdown) each consume input while open. With no modal active, global shortcuts
run first, then the event is routed to the focused pane.

Mouse events arrive as `Event::Mouse` and go to `App::on_mouse`, which
hit-tests against the pane rectangles that `ui::draw` records each frame
(`app.layout`). Editor clicks/drag/wheel are forwarded to the code editor's own
`mouse` handler; explorer/messages/tab/menu clicks are mapped to the
corresponding row or item.

Menu items and palette `>`-commands share a single set of **action identifiers**
(strings like `file.save`, `edit.replace`, `view.explorer`). Both paths funnel
through `App::run_action`, so a command has exactly one implementation regardless
of how it is invoked.

The event loop polls with a 500 ms timeout rather than blocking, so the calendar
clock keeps ticking while the editor is idle.

## Rendering

`ui::draw` lays out three vertical bands — menu bar, body, status bar — then
splits the body horizontally into explorer / editor / messages according to which
drawers are visible. The editor band is itself split into a tab bar and the
text area plus a `Scrollbar`. Overlays (calendar, menu dropdown, search, palette,
prompt) are drawn last, each clearing its rectangle with the `Clear` widget
before painting a bordered box.

## Dependency version pinning (one ratatui only)

The `ratatui` widget ecosystem is **not** cross-version compatible: a widget
built against one `ratatui` major cannot be rendered into another's `Frame`,
because the `Widget`/`Buffer`/`Rect` types differ. So every ratatui-based crate
in the tree must agree on the version. The editor widget,
`ratatui-code-editor`, tracks `ratatui` **0.30** (via `ratatui-core`), which
pins the stack:

- `ratatui = 0.30`, `crossterm = 0.29` (the version both `ratatui` 0.30 and
  `ratatui-code-editor` use, so the `KeyEvent`/`MouseEvent` types passed into the
  editor match ours exactly).
- The file explorer, scrollbar, popups, menu, and command palette are
  implemented directly on `ratatui` primitives (`List`, `Scrollbar`, `Clear`,
  `Tabs`) rather than pulling additional widget crates.
- Date/time uses `jiff` and the month grid is rendered in-house, so STRIDE
  depends on exactly one date library.

Historical note: the first cut used `tui-textarea` (which targets `ratatui`
0.29). Moving the editor to `ratatui-code-editor` brought Tree-sitter syntax
highlighting and built-in mouse handling, and lifted the whole stack to 0.30.

## Testing strategy

Because the logic is terminal-independent, `tests/integration.rs` constructs an
`App`, feeds it synthetic `crossterm` `KeyEvent`s, and asserts on the resulting
state — covering typing, open/save round trips, tab lifecycle, go-to-line, fuzzy
matching, the search-pattern builder, end-to-end regex replace with capture
groups, and the ISO/date formatting. The render path is exercised separately by
launching the binary inside a sized pseudo-terminal.
