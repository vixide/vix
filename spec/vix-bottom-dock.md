# Bottom dock

The bottom dock is a scrollable line buffer for **log messages, terminal/command
output, data views, and similar** — a full-width panel that sits at the bottom of
the body, above the status bar.

**Status:** Shipped (state + panel). The state lives in the internal
`vix-bottom-dock` crate (`BottomDock`); the host renders it (`src/ui.rs`) and
routes the toggle (`src/app.rs`). Pure data, no dependencies.

## Behavior

- Toggle with **View → Show/Hide Bottom Dock** (`view.bottom_dock`), the command
  palette, or the `show_bottom_dock` setting (default off). The choice persists.
- When shown, it takes a strip at the bottom of the body, **pinned directly above
  the status bar**; the explorer / editor / messages share the space above it.
- Its height is **draggable**: press the dock's top edge and drag up/down to grow
  or shrink it (kept between a 3-row minimum and leaving 3 rows for the body). The
  height persists in the `bottom_dock_height` setting.
- Shows the newest lines (pinned to the bottom) or a `(no output yet)` hint when
  empty.

## State (`vix-bottom-dock`)

- `BottomDock` — the `lines` buffer (oldest first, capped at 5,000) and the
  `scroll` offset.
  - `push` appends a line and pins the view to the bottom.
  - `clear` empties the buffer.
  - `scroll_up`/`scroll_down` move the viewport.
  - `visible(height)` returns the lines for a `height`-row viewport.

## Producers

- **Search in Project → Dock** (Edit → Find submenu) prompts for a term, scans
  every project file (case-insensitive), and lists hits as `relpath:line:col:
  text` — each click-to-jumps to the match.
- **Run Command** (Tools → Run Command…) runs a shell command in the project root
  in a background thread, **streaming** a `$ command` header, the merged
  stdout/stderr lines, and an `[exit N]` footer into the dock (showing it). The
  event loop drains the output each frame (polling faster while a command runs),
  so the UI stays responsive. **Cancel Command** kills it (and adds
  `[cancelled]`). Only one command runs at a time.

## Focus and scrolling

- Click the dock to focus it (its border brightens); `Esc` returns focus to the
  editor.
- While focused: `↑`/`↓` scroll a line, `PgUp`/`PgDn` a page, `Home`/`End` to the
  top/bottom. The mouse wheel scrolls it whether or not it is focused.
- **Click-to-jump:** clicking a line that names a `path:line[:col]` location (a
  build error, grep hit, etc.) opens that file there — so Run Command output is
  actionable.

## Roadmap

- Route diagnostics or a data view into the dock.
