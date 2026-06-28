# Edit SQL

A statement-oriented edit surface for `.sql` buffers (`vix-edit-sql` mode). Open
it with **Edit → Mode → SQL…** (`tools.edit_sql`) or the command palette. The
logic lives in the pure `crate::edit_sql` module; the host renders the overlay
and saves back through the normal file-save flow.

## What it does

The buffer is parsed into individual SQL **statements** by splitting on top-level
semicolons — semicolons inside single/double-quoted strings, line comments
(`-- …`), and block comments (`/* … */`) are ignored. Each statement is shown as
a row with a **kind** label (its leading keyword: `SELECT`, `INSERT`, `CREATE`,
…) and a one-line preview.

From the overlay you can:

- **Navigate** — `↑`/`↓` (or `k`/`j`), `PgUp`/`PgDn`, `Home`/`End`.
- **Reorder** — `Alt+↑`/`Alt+↓` (or `K`/`J`) move the statement among its peers.
- **Delete** — `d` (or `Delete`) removes the statement.
- **Format** — `f` uppercases SQL keywords in the selected statement; `F` does
  every statement. Keywords inside strings/comments are left alone.
- **Undo / redo** — `u` / `Ctrl+R`.
- **Save** — `Ctrl+S` serializes the statements back into the buffer (one per
  block, `;`-terminated) and saves via the normal flow. **Esc** / `q` closes.

This is a pragmatic helper for organizing and tidying SQL files, not a full SQL
parser or query engine. The `edit_sql` module is unit-tested
(`split_statements`, `format_sql`, kind detection, reorder/delete/undo).
