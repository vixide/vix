# DB — the database workbench

The **DB** menu (mnemonic `Alt+D`) opens a full-screen database workbench:
saved connections, a schema tree, a SQL query editor with autocomplete and
live highlighting, and a results grid with sorting, filtering, and export.

It works with **SQLite** files and **PostgreSQL** / **MySQL** servers through
embedded drivers (the [sqlx] crate — bundled SQLite, pure-Rust
Postgres/MySQL over rustls), so nothing else needs to be installed. Each
workbench connection holds one **persistent connection**, which means
session state — temporary tables, `search_path`, and above all
`BEGIN` / `COMMIT` / `ROLLBACK` transactions — carries across statements.

[sqlx]: https://crates.io/crates/sqlx

## Connections

**DB → Connections…** (or the command palette, *DB: Connections*) lists your
saved connections.

- **Enter** connects · **a** adds · **e** edits · **d** deletes ·
  **Esc**/**q** closes.
- The add/edit form: **↑/↓** pick a field, type to edit it, **Space** cycles
  the engine kind (sqlite → postgres → mysql), **Enter** saves.
- Server engines prompt for a **password** on connect. It is masked, kept
  only in memory for the session, and **never written to disk** — saved
  connections persist in your settings file without any password field.
- For SQLite, only the *File* field matters; a fresh path creates the file.

## The workbench

Three panes — schema tree, query editor, results — cycled with **Tab** /
**Shift+Tab**. **Esc** returns to the connections list. The hint line at the
bottom always shows the keys that matter where you are.

### Schema tree (left)

Schemas expand into **Tables / Views / Functions** folders.

- **↑/↓** (or **j/k**) move · **←/→**/**Space** collapse/expand ·
  **Enter** on a table shows its **columns**.
- **i** indexes · **f** foreign keys · **g** triggers · **x** constraints —
  each report opens in the results grid.
- **p** previews the table's data (`SELECT * … LIMIT 200`).
- **/** searches the tree live (matches show regardless of collapse state);
  **Enter** keeps the filter, **Esc** clears it.
- **r** refreshes the schema after DDL changes.

### Query editor (right, top)

Type SQL; multiple statements separated by `;` are fine.

- **Ctrl+Enter** or **F5** executes only the statement under the cursor;
  **F9** executes all statements in order (stopping at the first error).
- **F6** shows the query plan (`EXPLAIN`; `EXPLAIN QUERY PLAN` on SQLite);
  **F7** runs `EXPLAIN ANALYZE`. If the plan contains a full table scan, the
  message line suggests considering an index.
- Statements that **write** (`INSERT`, `UPDATE`, `DELETE`, `DROP`, … — even
  hidden inside a `WITH` CTE) ask for confirmation first: **Enter/y** runs,
  **Esc/n** cancels. `EXPLAIN ANALYZE` of a write asks too, since it really
  executes.
- **Transactions just work**: run `BEGIN`, some statements, then `COMMIT` or
  `ROLLBACK` — the persistent connection keeps the transaction open between
  executions. Savepoints too.
- **Alt+Shift+F** re-formats the statement at the cursor: keywords
  uppercased, major clauses on their own lines, `AND`/`OR` indented.
- Autocomplete pops up as you type: SQL keywords, table names from the
  connected database, and `table.` column completion. **↑/↓** choose,
  **Tab** accepts, **Esc** dismisses.
- Highlighting follows your Vix™ theme's `syntax` colors (keywords, strings,
  numbers, comments), the same ones the code editor uses.

### Results grid (right, bottom)

- **↑/↓** (or **j/k**), **PgUp/PgDn**, **Home/End** move; **←/→** (or
  **h/l**) select a column; the title shows the row count.
- **s** cycles a sort on the selected column: ascending ▲ → descending ▼ →
  off. Numbers compare numerically.
- **/** filters rows live (any cell, case-insensitive).
- **y** copies the selected cell, **Y** the whole row (tab-separated).
- **v** opens the cell viewer for long content — **p** toggles JSON
  pretty-printing, **y** copies, **Esc** closes.
- **e** exports the current view (filter and sort applied): **←/→** choose
  CSV, TSV, JSON, NDJSON, Markdown, or SQL `INSERT`s; **Tab** switches
  between writing a file and copying to the clipboard; **Enter** exports.

### Editing a table

Press **p** on a table in the tree to preview it. If the table has a primary
key, the preview is editable; nothing is written until you commit.

- **i** edits the selected cell. For a boolean, an enum or a column with a
  `CHECK (col IN (…))` constraint, **↑/↓** pick from its allowed values.
  Generated columns are read only.
- A value starting with `=` is an SQL expression the server computes when
  you commit: `=now()`, `=upper('x')`, `=NULL`. Type `\=` to store text that
  starts with `=`.
- **a** adds a new row (blank cells take the column default), **d** marks the
  selected row for deletion (press it again to restore the row), and **u**
  discards a row's changes. Changed cells are yellow, new rows green, and
  deleted rows red and struck through.
- **W** commits everything in one transaction (write mode, **F8**, must be
  on). If a row changed since you loaded it, or any statement would affect
  anything other than exactly one row, the whole commit is rolled back and
  your changes stay staged.

### Searching a table on the server

**/** filters only the rows already loaded (a preview loads at most 200).
To search the whole table, press **F** on a table preview: choose a column
and an operator with **↑/↓** and **←/→**, type a value, and press **Enter**.
The operators are contains (case-insensitive), `=`, `<>`, `<`, `<=`, `>`,
`>=`, between (`low..high`), `IN` (`a, b, c`), `IS NULL`, `IS NOT NULL`, and
a raw SQL condition written as is. Vix quotes the column and the value for
your database, runs the query on the server, and the results stay
editable. **Delete** in the search form clears the search.

## Query history and saved queries

- **Ctrl+R** opens the **history**: every executed statement, newest first,
  deduplicated, kept across sessions (capped at 200). **Enter** inserts an
  entry back into the editor as a new statement; **d** deletes one.
- **Ctrl+S** saves the statement at the cursor under a name; **Ctrl+B**
  browses the **saved queries** with the same keys.
- Both live in your config directory (`db_history.toml`, `db_queries.toml`),
  separate from settings.

All of this is also reachable from the **DB** menu and the command palette.
See the specification at [`crates/vix-db/spec/index.md`](../../crates/vix-db/spec/index.md) and the
session design notes at [`crates/vix-db/spec/session.md`](../../crates/vix-db/spec/session.md).

---

Vix™ and Vix IDE™ are trademarks.
