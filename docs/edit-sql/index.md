# Edit SQL

`vix-edit-sql` mode gives a `.sql` file a tidy, statement-at-a-time view. Open it
from **Edit → Mode → SQL…** or the command palette (*Edit SQL*).

Vix splits the buffer into individual SQL statements (it knows not to split on
semicolons inside strings or comments) and lists them, each tagged with its kind
(`SELECT`, `INSERT`, `CREATE`, …) and a one-line preview.

From the list you can:

- **Move** with `↑`/`↓` (or `k`/`j`), `PgUp`/`PgDn`, `Home`/`End`.
- **Reorder** a statement with `Alt+↑`/`Alt+↓` (or `K`/`J`).
- **Delete** a statement with `d`.
- **Format** keywords to uppercase with `f` (selected) or `F` (all).
- **Undo / redo** with `u` / `Ctrl+R`.
- **Save** back to the buffer with `Ctrl+S`; **Esc** or `q` closes.

It's a practical way to reorganize and clean up SQL scripts — not a full SQL
parser. See the specification at [`spec/edit-sql/index.md`](../../spec/edit-sql/index.md).
