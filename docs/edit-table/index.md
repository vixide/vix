# Edit Table

Edit Table opens the active buffer as a spreadsheet-like grid of rows and columns,
so you can view and edit delimited data — **CSV** and **TSV** — without counting
commas. It is one of Vix™'s *edit surfaces*: a full-screen overlay over the editor
that owns its own keys and saves back to the buffer.

## Opening

Open it from **Edit → Mode → Table…** or the command palette (*Edit Table*). The
delimiter is chosen from the file extension — `.tsv` is tab-separated, everything
else is parsed as CSV (RFC 4180 quoting). The first row is treated as the header
and is pinned while you scroll.

## Moving around

- **Arrow keys** or **h / j / k / l** move the cell cursor.
- **PageUp / PageDown** move by a screen; **Home / End** jump to the first / last
  column; **g / G** jump to the first / last row.

## Editing

- **Enter** or **F2** edits the selected cell; type to change it, then **Enter**
  commits and moves down, **Tab** commits and moves right, **Esc** cancels.
- **Delete** clears the current cell.
- **Alt + ↑ / ↓** insert a row above / below; **Alt + ← / →** insert a column to
  the left / right. **Alt + Delete** deletes the row; **Alt + Backspace** deletes
  the column. At least one row and one column always remain.
- **s** / **S** sort the data rows by the current column, ascending / descending
  (numeric cells compare numerically; the header stays put).
- **/** starts a search; type a query and **Enter** jumps to the next match;
  **n** repeats it.
- **u** undoes, **Ctrl + R** redoes.

## Saving and closing

**Ctrl + S** serializes the grid back to delimited text in the active buffer and
saves it (the normal Save / Save As flow applies). **Esc** or **q** closes the
editor. A `*` in the title marks unsaved edits.

See also the [Edit Outline](../edit-outline/index.md) prose outliner and the
specification at `crates/vix-editor/spec/edit-table/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
