# Edit Bytes

The **Edit Bytes** command shows the active buffer's bytes as a classic hex dump
— an offset column, sixteen hex byte pairs, and an ASCII gutter — and lets the
user move a byte cursor and overwrite bytes by typing hex digits. Open it with
**Tools → Edit Bytes…** or the command palette. The logic lives in the
`edit_bytes` module; the host (`app`/`ui`) renders the dump, syncs the scroll
window, and persists saves.

## Model

- The editor holds the buffer's bytes (`tab.text().into_bytes()`), a byte cursor,
  and an undo history. Sixteen bytes are shown per row.
- Editing is **overwrite-only** (no insert/delete), so the length stays stable.
  Typing a hex digit sets the current byte's high nibble, then the low nibble,
  then advances. Moving the cursor resets the nibble.
- Saving decodes the bytes to UTF-8 *lossily* for the text buffer and writes via
  the normal save flow. (A full binary round-trip for non-UTF-8 files is a future
  refinement; today the surface targets text buffers' bytes.)

## Keys

- **↑/↓/←/→** (or `k`/`j`/`h`/`l`): move the byte cursor. **Home/End** jump to the
  row ends; **PageUp/PageDown** page.
- A **hex digit** (`0`–`9`, `a`–`f`): overwrite the current byte's high then low
  nibble.
- **u** / **Ctrl+R**: undo / redo. **Ctrl+S**: save. **Esc** / **q**: close.

## As implemented in Vix

`edit_bytes::Hex` owns the bytes, cursor, nibble state, and undo history, and
interprets keys itself, returning an `Outcome` (`Consumed`/`Close`/`Save`). `app`
opens it from the active buffer's bytes, routes keys, and on save writes the
(lossily decoded) bytes back through `file.save`. `ui` renders each row as
`offset  hex pairs  |ascii|`, highlighting the cursor byte.
