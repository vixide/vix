# Multiple Cursors

Vix supports editing at several places at once with multiple cursors ("carets").
There is always one **primary** cursor (the usual cursor + selection); extra
carets are added on top of it.

## Adding and removing carets

- **`Ctrl+D`** — select the word at the cursor; press again to select the **next
  occurrence** and add a caret there (the new match becomes primary). Repeat to
  keep adding matches; it wraps around the buffer. With an existing selection,
  `Ctrl+D` searches for that selection's text.
- **`Alt`+click** — add an extra caret at the clicked position.
- **`Esc`** — drop all extra carets, keeping the primary.
- A plain click collapses back to a single cursor.

## Editing with multiple carets

While extra carets are active, these apply at **every** caret simultaneously, as
a single undo step:

- Typing text (each caret inserts it; a caret with a selection replaces it).
- **Backspace** / **Delete**.
- **Arrow keys** move every caret (Shift extends each selection).

## Notes

- `Ctrl+Shift+D` duplicates the current line/selection (the old `Ctrl+D`).
- Carets are de-duplicated and kept sorted; the lowest is the primary.
- All carets and their selections are rendered (in both normal and soft-wrap
  modes).

## As implemented in Vix

`vix-editor`'s `Editor` holds extra `carets: Vec<Caret { pos, anchor }>` beyond
its primary `cursor`/`selection` (see `multicursor.rs`). `multi_insert` /
`multi_delete` edit the rope once, processing carets in ascending order with a
running offset shift so each edit's coordinates stay valid; `multi_move` drives
the single-cursor move logic per caret; `add_next_occurrence` implements
`Ctrl+D`. The renderer (`render.rs`, `wrap.rs`) draws every caret and selection
via `caret_positions()` / `caret_selections()`. The host adds `Alt`+click
(`add_caret_at`) and collapses on a plain click.
