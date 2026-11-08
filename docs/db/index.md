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
- **Paste a connection string** into the form to fill it: a `postgres://`,
  `postgresql://`, `mysql://` or `sqlite:` URL, or a libpq `key=value`
  string (`host=db port=5432 dbname=shop user=me sslmode=require`, values
  may be single-quoted). A password in the string is held in memory for the
  next connect and never saved.
- The **Environment** row cycles `local` (default) → `development` →
  `testing` → `staging` → `production`, shown as a badge in the list and the
  editor title. On **production** every write shows its SQL and asks, even
  inside a transaction. It is saved in your settings file as a
  `[db_environments]` table (`name = "production"`).
- Server engines prompt for a **password** on connect. It is masked, kept
  only in memory for the session, and **never written to disk** — saved
  connections persist in your settings file without any password field.
- For SQLite, only the *File* field matters; a fresh path creates the file.

## The workbench

Three panes — schema tree, query editor, results — cycled with **Tab** /
**Shift+Tab**. **Esc** returns to the connections list. The hint line at the
bottom shows the most common keys on one line; this page lists all of them.

These keys work in any pane:

- **F5** / **Ctrl+Enter** run the statement at the cursor, **F9** runs them
  all, **F6** / **F7** explain (see [Query editor](#query-editor-right-top)).
- **F8** toggles write mode for the session; connections open read-only.
- **Ctrl+C** cancels a running statement. On PostgreSQL and MySQL it asks the
  server to stop it, so temporary tables, session settings and an open
  transaction survive (a cancelled statement inside a transaction leaves it
  needing a `ROLLBACK`). On SQLite, or when the server refuses, Vix
  reconnects instead, and the status line says if a transaction was lost.
- **Ctrl+L** opens the **query log**: every statement run this session with
  its duration, row count and outcome, color-coded by latency; **Enter**
  loads a statement back into the editor.
- **Ctrl+E** builds an **ER diagram** of the live schema as Mermaid
  `erDiagram` text in a viewer; **y** copies it.
- **Ctrl+R**, **Ctrl+S**, **Ctrl+B**: [history and saved queries](#query-history-and-saved-queries).
- **Ctrl+P** runs a `.sql` file ([below](#running-a-sql-file)), **Ctrl+D**
  opens [schema compare](#schema-compare), **Ctrl+U**
  [imports a CSV or TSV file](#importing-a-csv-or-tsv-file).
- AI helpers, which see only the schema, never a row (all refused when
  `ai = "off"`):
  - **Ctrl+A** asks in plain language for SQL: type a question and press
    **Enter**; the SQL lands in the editor with its plan, and **F5** runs it.
    Start the question with `?` to get a plain-English answer about the
    schema instead.
  - **Ctrl+O** asks for a faster version of the statement at the cursor,
    sending it with its plan.
  - **Ctrl+F** fixes the last error: it sends the failed statement and the
    error message and puts the corrected query in the editor.
  - **Ctrl+K** explains the statement at the cursor in plain English,
    without running it.

### Schema tree (left)

Schemas expand into **Tables / Views / Functions** folders.

- **↑/↓** (or **j/k**) move · **←/→**/**Space** collapse/expand ·
  **Enter** on a table shows its **columns**.
- **i** indexes · **f** foreign keys · **g** triggers · **x** constraints ·
  **s** statistics (row count and size) · **D** the table's `CREATE`
  statement (DDL) — each report opens in the results grid.
- **R** shows the table's **relations**: its foreign keys to other tables,
  the foreign keys on other tables that point at it, and the views that read
  from it (on SQLite the view list is a best guess from each view's SQL).
- **p** previews the table's data (`SELECT * … LIMIT 200`).
- **A** copies the table for AI: its columns, primary key, foreign keys,
  constraints and indexes, plus up to 50 rows, as Markdown.
- **/** searches the tree live (matches show regardless of collapse state);
  **Enter** keeps the filter, **Esc** clears it.
- **r** refreshes the schema after DDL changes.
- **m** marks the table under the cursor for export (press again to unmark);
  **e** exports the marked tables — or, with none marked, the table under the
  cursor. See [Exporting whole tables](#exporting-whole-tables).

### Query editor (right, top)

Type SQL; multiple statements separated by `;` are fine.

- **Ctrl+Enter** or **F5** executes only the statement under the cursor;
  **F9** executes all statements in order (stopping at the first error).
- **F6** shows the query plan (`EXPLAIN`; `EXPLAIN QUERY PLAN` on SQLite);
  **F7** runs `EXPLAIN ANALYZE`. If the plan contains a full table scan, the
  message line suggests considering an index. On PostgreSQL the plan is shown
  as a tree, one row per step: with **F7** each step shows its share of the
  total time, the slowest steps are highlighted in red, and a step whose row
  estimate is off from the real count by 10× or more is flagged in yellow.
- Statements that **write** (`INSERT`, `UPDATE`, `DELETE`, `DROP`, … — even
  hidden inside a `WITH` CTE) ask for confirmation first, showing the SQL:
  **Enter/y** runs, **Esc/n** cancels. `EXPLAIN ANALYZE` of a write asks
  too, since it really executes. Inside an open transaction a write runs
  without asking, except `DROP`/`TRUNCATE` and any write on a
  **production** connection.
- **Transactions just work**: run `BEGIN`, some statements, then `COMMIT` or
  `ROLLBACK` — the persistent connection keeps the transaction open between
  executions. Savepoints too.
- **Alt+Shift+F** re-formats the statement at the cursor: keywords
  uppercased, major clauses on their own lines, `AND`/`OR` indented.
- Autocomplete pops up as you type: SQL keywords, table names from the
  connected database, and `table.` column completion. Aliases work too:
  after `FROM orders o` (or `AS o`, or `JOIN … c`), `o.` offers the columns
  of `orders`. **↑/↓** choose,
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
- **x** shows the selected row vertically, one column per line.
- **f** on a foreign-key cell follows it to the row it references.
- **c** charts a two-column (label, number) result as a bar chart.
- **A** copies for AI: the statement, the rows from the top of the visible
  page (at most 50, filter and sort applied) and, on a table preview, the
  column types, as Markdown to paste into a chat. It is your copy to share;
  Vix's AI assistant is still shown the schema only, never a row.
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

## Exporting whole tables

**e** in the schema tree exports whole tables, not just the rows loaded in
the grid. **←/→** choose the format:

- **CSV**, **JSON** or **NDJSON** — one file per table (`users.csv`, …) in
  the directory you type (created if missing; `schema.table.csv` when the
  tables come from more than one schema). JSON keeps `NULL`, numbers and
  booleans typed; CSV writes `NULL` as an empty field.
- **SQL** — one file of `INSERT` statements for every chosen table, inside
  `BEGIN;` … `COMMIT;`, ordered so a table comes after the tables its foreign
  keys reference. If the foreign keys form a cycle, the tables keep the order
  you chose and a comment at the top says to load with foreign-key checks
  off. At the end, sequences are reset: `setval` on PostgreSQL's serial and
  identity sequences, `ALTER TABLE … AUTO_INCREMENT` on MySQL, and
  `sqlite_sequence` on SQLite.

**Enter** starts the export. It runs in the background on a **second,
read-only connection**, inside one consistent snapshot (`REPEATABLE READ READ
ONLY` on PostgreSQL, `WITH CONSISTENT SNAPSHOT` on MySQL, one read
transaction on SQLite), and rows stream straight to disk, so even a very
large table never sits in memory. The status line shows progress, and you
can keep working meanwhile (disconnecting or closing the DB overlay stops
the export and removes its partial file). Because it is a separate connection, a
transaction you have open in the workbench is never touched — and its
uncommitted changes are not in the export. An in-memory SQLite database
cannot be seen from a second connection, so it cannot be exported this way.

## Running a `.sql` file

- **Ctrl+P** in the workbench asks for the path of a `.sql` file, loads it
  into the query editor, and runs it statement by statement, stopping at the
  first error — exactly like **F9**.
- **DB → Run Buffer in Workbench** does the same with the buffer you are
  editing (unsaved changes included).
- Writes still follow the usual rules: refused on a read-only connection,
  confirmed first otherwise.
- psql meta-commands (`\copy`, `\connect`, `\i`, `\set`, … — lines that start
  with a backslash) are not SQL. If the script has any, nothing runs: the
  message line lists their line numbers, and the script stays in the editor
  for you to fix. **F9** applies the same check to anything in the editor.

## Schema compare

**Ctrl+D** in the workbench (or **DB → Schema Compare…**) compares one schema
of the connected database with another saved connection of the same engine
(PostgreSQL or SQLite; not MySQL), and writes the migration that makes that
other database — the **target** — match the one you are connected to.

- Pick the target with **↑/↓**; type to change the schema name (`public` by
  default on PostgreSQL); **Enter** compares. The target connects like any
  other connection: password command, keyring, or a password prompt.
- The review lists every difference — tables, columns, keys, checks,
  indexes, foreign keys, views, functions, triggers, enums, sequences — with
  a checkbox. **Space** toggles one; the bottom pane shows both definitions
  and the SQL. Changes that drop a table or column (*destructive*) start
  unchecked; changes that can fail on existing rows (*risky*: `NOT NULL`, a
  unique index, a type change, a new constraint) are flagged.
- The migration is ordered so it runs: types first, then drops, then
  creates, foreign keys after their tables, views and triggers last. Views
  that use a dropped or retyped column are dropped and recreated around it.
- **Enter** shows the whole script, **y** copies it, **s** saves it as a
  `.sql` file, **o** opens it in the editor, and **r** runs it on the target
  in one transaction after a confirmation — if any statement fails, nothing
  changes. A read-only target refuses to run until **F8** allows writes.
- SQLite can only add and drop columns in place; other SQLite table changes
  show as *manual* notes in the script.

## Importing a CSV or TSV file

**Ctrl+U** asks for the path of a `.csv` or `.tsv` file (a tab for `.tsv` and
`.tab`, a comma otherwise) and loads it into a new table named after the file:
the header row becomes the columns, all of type `TEXT`, and the other rows are
inserted. It creates and writes a table, so write mode (**F8**) must be on; on
a **production** connection it shows its SQL and asks first. The statements
run in order and stop at the first error, which can leave the new table empty.

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
