# Right dock

The right dock is the **message drawer**: advice and notifications, each
individually dismissable.

**Status:** Shipped. The state lives in the internal `vix-right-dock` crate
(`Messages`, `Message`, `Level`); the host renders it (`src/ui.rs`) and routes
keys/clicks (`src/app.rs`). Pure data, no dependencies.

## State (`vix-right-dock`)

- `Level` — `Info`, `Advice`, `Warn`, `Error` (selects the row icon).
- `Message` — `level` + `text`.
- `Messages` — the `items` list (oldest first) and the `selected` row.
  - `push`/`info`/`advice`/`warn`/`error` append a message.
  - `up`/`down` move the selection.
  - `close_selected` dismisses the highlighted message (the close `x`), keeping
    the selection in range.

The drawer is toggled with **View → Show/Hide Right Dock** (and the menu-bar dock
icon); its width is draggable and persists in `messages_width`.
