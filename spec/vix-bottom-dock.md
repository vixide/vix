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
- When shown, it takes a fixed-height strip at the bottom of the body; the
  explorer / editor / messages share the remaining space above it.
- Shows the newest lines (pinned to the bottom) or a `(no output yet)` hint when
  empty.

## State (`vix-bottom-dock`)

- `BottomDock` — the `lines` buffer (oldest first, capped at 5,000) and the
  `scroll` offset.
  - `push` appends a line and pins the view to the bottom.
  - `clear` empties the buffer.
  - `scroll_up`/`scroll_down` move the viewport.
  - `visible(height)` returns the lines for a `height`-row viewport.

## Roadmap

- Producers (run a command and stream stdout/stderr here; route diagnostics or a
  data view into it) — the buffer API is ready; wiring is future work.
- Keyboard/mouse scrolling and focus.
