# Edit Table

The **Edit Table** command opens a spreadsheet-like grid for viewing and editing
delimited data — **CSV** and **TSV** — as rows and columns. Open it with
**Tools → Edit Table…** or the command palette (*Edit Table*); it parses
the active buffer (CSV by default, TSV when the file extension is `.tsv`). Its
logic lives in the `edit_table` module; the host (`app`/`ui`) renders the grid,
syncs the scroll window, and persists saves.

It is inspired by the most-common capabilities of grid data viewers such as
`tabiew`, kept to a focused view-and-edit feature set rather than full parity
(no SQL queries, multi-format ingest, plotting, or themes).

## Model

- The buffer is parsed into a rectangular grid of string cells (short rows are
  padded with empty cells). **Row 0 is the header** and is pinned while scrolling.
- A **cell cursor** tracks the selected row and column. The grid keeps an
  undo/redo history and a dirty flag.
- Saving serializes the grid back to delimited text (CSV with RFC 4180 quoting,
  or TSV) via the shared `convert_tabular` helpers and writes it through the
  normal file-save flow.

## Layout

- A bordered, near-full-screen overlay titled `Edit Table` (with a `*` when
  there are unsaved edits).
- A pinned **header row**, a scrolling **body** of data rows with the selected
  cell reverse-highlighted, and a bottom **status/hint** line showing the cursor
  position (`r/R c/C`) and the current hint, find query, or edit notice.
- Columns are sized to their widest cell, clamped to a sensible range, and scroll
  horizontally to keep the selected column visible.

## Navigation

- **Arrows** or **h / j / k / l** move the cell cursor.
- **PageUp / PageDown** move by a screen of rows; **Home / End** jump to the
  first / last column; **g / G** jump to the first / last row.

## Editing

- **Enter** or **F2** edits the current cell (seeded with its contents); type to
  change it, **Enter** commits and moves down, **Tab** commits and moves right,
  **Esc** cancels. **Delete** clears the current cell.
- **Alt + ↑ / ↓** insert a row above / below; **Alt + ← / →** insert a column to
  the left / right. **Alt + Delete** deletes the current row; **Alt + Backspace**
  deletes the current column. At least one row and one column always remain.
- **s** / **S** sort the data rows by the current column ascending / descending
  (the header stays fixed; numeric cells compare numerically).
- **/** starts a search; type a query and **Enter** jumps to the next matching
  cell (case-insensitive, wrapping). **n** repeats the search.
- **u** undoes and **Ctrl + r** redoes the last change.

## Save and close

- **Ctrl + S** writes the grid back into the active buffer and saves it (the
  standard Save / Save As flow applies for untitled buffers).
- **Esc** or **q** closes the editor.

## As implemented in Vix

`edit_table::Grid` owns the cells, cursor, scroll offsets, edit/find buffers,
and undo/redo history. `Grid::handle_key` interprets key events itself and
returns an `Outcome` (`Consumed` / `Close` / `Save`) so the host stays thin:
`app` routes keys to the open grid, maps the outcome, and on save copies the
serialized text into the active editor buffer before invoking `file.save`. `ui`
renders the pinned header, the scrolling body, and the status line, and records
the body rectangle for paging.
