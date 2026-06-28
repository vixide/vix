# Edit Bytes

Edit Bytes shows the active buffer's bytes as a classic hex dump — an offset
column, sixteen hex byte pairs, and an ASCII gutter — and lets you move a byte
cursor and overwrite bytes by typing hex digits.

## Opening

Open it from **Edit → Mode → Bytes…** or the command palette (*Edit Bytes*). The
view loads the buffer's bytes; sixteen are shown per row, with the cursor byte
highlighted in both the hex and ASCII columns.

## Moving and editing

- **Arrow keys** or **h / j / k / l** move the byte cursor; **Home / End** jump to
  the row ends; **PageUp / PageDown** page.
- Type a **hex digit** (`0`–`9`, `a`–`f`) to overwrite the current byte: the first
  digit sets the high nibble, the second sets the low nibble and advances. Moving
  the cursor resets the nibble.
- Editing is **overwrite-only** (no insert or delete), so the file length stays
  stable.
- **u** undoes, **Ctrl + R** redoes.

## Saving and closing

**Ctrl + S** writes the bytes back to the buffer and saves it. **Esc** or **q**
closes.

Note: the bytes are decoded to UTF-8 (lossily) for the text buffer, so this
surface targets the bytes of text files; a full binary round-trip is a future
refinement. See the specification at `spec/edit-bytes/index.md`.
