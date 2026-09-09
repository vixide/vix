# Table Editor

**Status:** Shipped. The table editor: a spreadsheet-like grid for viewing
and editing delimited data (CSV/TSV) as rows and columns.

Vix's Tools menu offers an *Edit Table* command that parses the active
buffer as CSV or TSV (per the file extension) into a rectangular grid. The
first row is treated as the header. The user navigates a cell cursor with
the arrow keys (or `h`/`j`/`k`/`l`), edits a cell in place, inserts and
deletes rows and columns, sorts by the current column, and searches across
all cells. Saving serializes the grid back to delimited text.

This crate is self-contained and host-agnostic: it owns the grid data, the
cursor, the edit/find buffers, and an undo/redo history, and it interprets
key events itself (returning an `Outcome` telling the host when to close or
save). The host (`App` in the root `vix` crate) only renders the grid,
syncs the visible scroll window, and acts on the returned outcome.

## Contents (`vix_edit_table`)

- `Grid` — the table state: `rows` (the first is the header), the cell
  `row`/`col` cursor, the current `Mode` (`Normal`/`Edit`/`Find`), edit and
  find buffers, dirty flag, and undo/redo history (capped at 200 steps).
  `from_text(text, tsv)` parses a buffer in (via `vix_convert_tabular`'s
  `parse_csv`/`parse_tsv`); `to_text()` serializes back out
  (`write_csv`/`write_tsv`); `handle_key` is the single entry point for
  input.
- `Outcome` — what the host does after a key: `Consumed` (nothing further),
  `Close` (Esc/`q` in normal mode), or `Save` (Ctrl+S — the host persists
  the buffer).

## Roadmap

- Nothing open.
