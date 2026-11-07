# The DB Workbench

The **DB** menu (mnemonic `Alt+D`, right after **AI**) opens a full-screen
database workbench: saved connections, a schema tree, a SQL editor with
autocomplete and live highlighting, and a results grid with sort, filter,
and export — plus an AI SQL assistant, an ASCII chart view, a Mermaid ER
diagram, and CSV/TSV import. It talks to **SQLite** files and **PostgreSQL**
/ **MySQL** servers through embedded [sqlx] drivers, so nothing external
needs installing.

[sqlx]: https://crates.io/crates/sqlx

This tutorial is example-driven, working entirely against the demo
workspace's seeded SQLite database. For the exhaustive key-by-key reference,
see [`docs/db/index.md`](../../db/index.md) and the full specification at
[`crates/vix-db/spec/index.md`](../../../crates/vix-db/spec/index.md).

Open the demo workspace:

```sh
cd tui/examples/demo-workspace
vix .
```

`db/demo.sqlite` (regenerated from `db/seed.sql`) has two tables:

- **`people`** — `id`, `name`, `role`, `score`: Alice (engineer, 92), Bob
  (designer, 81), Carol (engineer, 88), Dave (manager, 75).
- **`tasks`** — `id`, `person_id` (references `people.id`), `title`, `done`:
  four seeded rows, one or two per person, Bob has none.

## Connecting

Open **DB → Connections…** (or the command palette, `Ctrl+P` then `>`, *DB:
Connections*). The list starts empty.

1. Press **a** to add a connection. **↑/↓** move between fields, **Space**
   cycles the engine kind — leave it on **sqlite**.
2. Set **File** to `db/demo.sqlite` (relative to the workspace root you
   opened Vix in).
3. Press **Enter** to save the entry, then **Enter** again on it in the list
   to connect. You land in the three-pane workbench: schema tree, SQL
   editor, results grid, cycled with **Tab** / **Shift+Tab**; **Esc** returns
   to the connections list.

A fresh connection opens **read-only** — the editor pane title shows a
`read-only` badge. **F8** toggles write mode for the session (you'll need it
later, for CSV import). `Esc`/`q` from the connections list closes the
workbench entirely.

### Server connections and the credential waterfall

`demo.sqlite` needs no password, but for a PostgreSQL/MySQL entry the same
add/edit form gains a **Password cmd** field and a **Save to keyring**
toggle. On connect, Vix resolves the password through a waterfall before
ever prompting you: first `password_command` (any command that prints the
secret — `pass show db/prod`, `op read …`), then the OS keyring
(`security find-generic-password` on macOS, `secret-tool lookup` on Linux).
Only on a miss does it prompt — masked, held in memory for the session only,
and (with **Save to keyring** on) written back to the keyring so the next
connect skips the prompt. Saved connections never persist a password field
to disk. Server engines default to `writable = false`, same as SQLite, so
the read-only/write-mode split above applies there too.

Instead of filling the fields one by one you can **paste a connection
string** into the form: `postgres://me@db.example.com:5432/shop?sslmode=require`,
`mysql://…`, `sqlite:db/demo.sqlite`, or a libpq string such as `host=db
dbname=shop user=me`. Vix fills the engine, host, port, user, database and
TLS mode (and the name, if it is still empty). A password inside the string
is never written to a field or to disk: it is held in memory for the next
connect, exactly like one typed on the prompt.

### Environments

The form's last row, **Environment**, marks what the database is: `local`
(the default), `development`, `testing`, `staging` or `production`. The
connections list and the editor title show it as a badge. On a
**production** connection every write shows its SQL and asks first, even
inside a `BEGIN … COMMIT` transaction (elsewhere a transaction skips the
question, since `ROLLBACK` can undo it). `DROP` and `TRUNCATE` ask on every
connection.

## The schema tree

The left pane groups each schema into **Tables / Views / Functions**
folders. **↑/↓** (or **j/k**) move, **←/→**/**Space** collapse/expand.

Expand `main` → `Tables` and land on `people`:

- **Enter** shows its columns (`id`, `name`, `role`, `score`) in the results
  grid.
- **i** / **f** / **g** / **x** show indexes / foreign keys / triggers /
  constraints for the selected table — try **f** on `tasks` to see its
  `person_id → people.id` foreign key.
- **s** shows row-count/size statistics; **D** shows the table's `CREATE`
  statement.
- **p** previews the table's data (`SELECT * FROM people LIMIT 200`) without
  writing a query.
- **/** searches the tree live (matches show regardless of collapse state);
  **r** refreshes the catalog after DDL changes.

## The SQL editor: autocomplete and format

Switch to the editor pane (**Tab**) and type a join across the seeded
tables:

```sql
SELECT people.name, people.role, tasks.title, tasks.done
FROM people
JOIN tasks ON tasks.person_id = people.id
ORDER BY people.name;
```

While typing, autocomplete pops up over SQL keywords, table names from the
connected database, and — type `people.` — its column names; right after
`JOIN` it suggests `tasks ON …` clauses built from the foreign-key graph.
**↑/↓** choose, **Tab** accepts, **Esc** dismisses. Highlighting (keywords,
strings, numbers, comments) follows your active theme's `syntax` colors.

Mistype the indentation or casing and press **Alt+Shift+F** (**DB → Format
Query**) — it re-formats the statement at the cursor: keywords uppercased,
major clauses on their own lines, `AND`/`OR` indented. It only touches the
statement your cursor is in, so multiple `;`-separated statements in one
buffer format independently.

## Running the query and reading the results

With the cursor on the statement, press **Ctrl+Enter** or **F5** (**DB →
Execute at Cursor**) — **F9** (**Execute All**) instead runs every statement
in the buffer in order, stopping at the first error. The query runs
asynchronously; the editor title shows an elapsed-time indicator while it's
in flight. **Ctrl+C** cancels: on PostgreSQL and MySQL it asks the server
to stop the statement and keeps your session and any open transaction (which
is then aborted, so roll it back); on SQLite, or if the server cancel fails,
it reconnects and the open transaction is lost. When PostgreSQL reports where
in the statement an error is, the cursor moves there. Results stream in as they
arrive rather than waiting for the full set — batches append to the grid
progressively, capped at 20,000 rows with a truncation note if a result set
is that large.

The join returns four rows — Alice's two tasks, Carol's and Dave's one each,
Bob absent since he has none:

```
Alice | engineer | Fix word_counts lowercasing bug | 0
Alice | engineer | Review api.http examples        | 1
Carol | engineer | Write demo.sqlite seed data      | 1
Dave  | manager  | Plan next tutorial chapter        | 0
```

In the results pane:

- **↑/↓**/**j/k**, **PgUp/PgDn**, **Home/End** move; **←/→**/**h/l** select a
  column — the title shows the row count.
- **s** cycles a sort on the selected column (ascending ▲ → descending ▼ →
  off, numbers compared numerically). **/** filters rows live against any
  cell, case-insensitive — try filtering on `engineer`.
- **x** expands the selected row vertically (handy once a query has more
  columns than fit); **f** on a foreign-key cell follows it to its parent
  row (a `tasks.person_id` cell jumps to that row in `people`).
- **y**/**Y** yank the cell/row (tab-separated) to the clipboard; **v** opens
  a full-content cell viewer with a JSON pretty-print toggle (**p**).
- **A** (*copy for AI*) copies the statement and up to 50 rows from the top
  of the visible page as Markdown, ready to paste into a chat; on a table
  preview it adds each column's type. **A** on a table in the tree copies
  its columns, keys, constraints and indexes with its rows. Only you decide
  where that goes: Vix's own AI assistant is still never shown a row.
- On a table preview that has a primary key (press **p** on `people` in the
  tree, not this join), **i** stages an edit to the selected cell, **a** a
  new row and **d** a row's deletion, and **W** commits them all in one
  transaction, with conflict detection against anyone else's changes in
  between. A value like `=now()` is computed by the database. **F** searches
  the whole table on the server by column and operator, where **/** only
  filters the rows already loaded.

## EXPLAIN and EXPLAIN ANALYZE

With the join statement still under the cursor, press **F6** (**DB →
Explain**, `EXPLAIN QUERY PLAN` on SQLite) to see how it runs without
executing it for real:

```
QUERY PLAN
|--SCAN tasks
|--SEARCH people USING INTEGER PRIMARY KEY (rowid=?)
`--USE TEMP B-TREE FOR ORDER BY
```

`people.id` is the primary key, so the `people` side is a fast indexed
`SEARCH`; `tasks.person_id` has no index, so the planner has to `SCAN` the
whole `tasks` table. The workbench's plan-doctor heuristic flags exactly
this — an unindexed `SCAN`, as opposed to a `SEARCH … USING INDEX` — and the
message line suggests considering an index (`CREATE INDEX ON
tasks(person_id)` would turn it into a `SEARCH`). **F7** (**Explain
Analyze**) runs `EXPLAIN ANALYZE` where the engine supports it — on SQLite
that's still `EXPLAIN QUERY PLAN`, since SQLite has no separate ANALYZE
form. Statements that actually write ask for confirmation first
(**Enter**/**y** runs, **Esc**/**n** cancels), showing the SQL in full —
`EXPLAIN ANALYZE` of a write does too, since it really executes.

## Exporting results

With the join's results still on screen, press **e** (**DB → Export
Results…**). **←/→** choose the format — CSV, TSV, JSON, NDJSON, Markdown,
or SQL `INSERT`s — **Tab** switches between writing a file and copying to
the clipboard, and **Enter** exports. The export applies your current filter
and sort, not the raw query result — filter to `engineer` first and the CSV
only has Alice's and Carol's rows.

To export **whole tables** instead, go to the schema tree: **m** marks
`people` and `tasks`, and **e** opens the table export (with nothing marked
it takes the table under the cursor). Pick **SQL** with **←/→** and press
**Enter**: Vix reads both tables from one consistent snapshot on a second,
read-only connection and writes one file of `INSERT`s with `people` before
`tasks` (parents before children). A table with an `AUTOINCREMENT` counter
(or a PostgreSQL sequence, a MySQL `AUTO_INCREMENT`) gets it reset at the end
of the file; these two have none.
CSV, JSON and NDJSON write one file per table into a directory instead. Rows
stream straight to disk, so the 20,000-row grid cap does not apply, and an
open transaction in the workbench is left alone.

## Running a `.sql` file

**Ctrl+P** asks for the path of a `.sql` file, loads it into the editor and
runs it like **F9** — statement by statement, stopping at the first error,
with writes confirmed as usual. **DB → Run Buffer in Workbench** does the
same with the file you are editing. A script with psql meta-commands
(`\copy`, `\i`, …) is refused before anything runs, and the message line
lists their line numbers.

## Query history and saved queries

- **Ctrl+R** (**DB → History…**) lists every statement you've executed,
  newest first, deduplicated, capped at 200, persisted in `db_history.toml`
  in your config directory. **Enter** re-inserts an entry into the editor as
  a new statement; **d** deletes one.
- **Ctrl+S** (**DB → Save Query…**) saves the statement at the cursor under
  a name; **Ctrl+B** (**DB → Saved Queries…**) browses them with the same
  keys, persisted separately in `db_queries.toml`.

## Named `:param` binds

Replace the join with a parameterized query:

```sql
SELECT name, role, score FROM people WHERE role = :role ORDER BY score DESC;
```

Run it (**F5**) — before executing, the workbench scans for `:name`
placeholders (skipping string literals, comments, and `::type` casts, so
this doesn't misfire on PostgreSQL casts) and prompts for each one. Type
`engineer` and Enter; the placeholder is substituted as a proper SQL literal
and the query returns Alice (92) then Carol (88).

## The AI SQL assistant

The workbench can turn a question into SQL, using whatever CLI the
`ai_command` setting names (default `claude -p "{prompt}"` — see
[`docs/configuration/index.md`](../../configuration/index.md#settings) and
[`docs/agent-panel/index.md`](../../agent-panel/index.md); any assistant CLI
works), or a provider's HTTP API directly when `ai_provider` is set — same
choice as the AI chat panel and menu. It's schema-only: the model sees
table/column names, types, and
foreign keys, but is **never shown a row of data** — the connection's
read-only default plus the client-side write guard both still apply to
whatever SQL comes back.

In the workbench (not the menu — these are editor-focused keys):

- **Ctrl+A** — **Ask**. Type a question, e.g. `people with more than one
  task`, Enter. The generated SQL lands in the editor with its `EXPLAIN`
  plan shown; **F5** runs it. Prefix with `?` (e.g. `?which tables reference
  people`) to ask a schema question answered in plain English instead.
- **Ctrl+O** — **Optimize**. Sends the statement at the cursor plus its plan,
  asking for a rewrite that avoids full scans (try it on the join above).
- **Ctrl+F** — **Fix error**. After a failed query, sends the error message
  and schema, and drops a corrected query in the editor.
- **Ctrl+K** — **Explain**. Explains the statement at the cursor in plain
  English in the text viewer, without running anything.

Only one request runs at a time; the editor title shows an `AI…` badge while
one is in flight.

## The ASCII chart view

Any two-column `(label, number)` result can be charted. Run:

```sql
SELECT name, score FROM people ORDER BY score DESC;
```

then press **c** in the results pane. It renders a horizontal bar chart in
the text viewer, sized by the last numeric column and labeled by the first —
Alice's bar the longest at 92, Dave's the shortest at 75.

## The ER diagram

Press **Ctrl+E** (from anywhere in the workbench) to build a Mermaid
`erDiagram` from the live schema — typed entity blocks for `people` and
`tasks`, with a crow's-foot edge for the `tasks.person_id → people.id`
foreign key — in a scrollable viewer. Press **y** to yank the Mermaid text
to the clipboard; paste it anywhere Mermaid renders, including a Vix Org
buffer's own Mermaid support.

## CSV/TSV import

`data/sample.csv` in this workspace has the same shape as `people` (minus
`id`):

```csv
name,role,score
Alice,engineer,92
Bob,designer,81
Carol,engineer,88
Dave,manager,75
```

Import creates a table, so it needs write mode: press **F8** first (the
badge switches to `read-write`). Then **Ctrl+U** opens the import prompt;
type `data/sample.csv` (relative to the workspace root you launched `vix .`
from, same as `db/demo.sqlite` was) and press Enter. The header becomes a
`CREATE TABLE sample (name TEXT, role TEXT, score TEXT)` (the table name
comes from the file's stem, columns sanitized to identifiers), and every
remaining row becomes one multi-row `INSERT`, both run for you. Refresh the
tree (**r**) to see the new `sample` table alongside `people` and `tasks`.

## Where to go next

- [`docs/db/index.md`](../../db/index.md) — the complete key-by-key
  reference this tutorial draws from.
- [`crates/vix-db/spec/index.md`](../../../crates/vix-db/spec/index.md) and
  [`crates/vix-db/spec/session.md`](../../../crates/vix-db/spec/session.md)
  — the full specification, including the session/transaction model and the
  SSH tunnel and credential-waterfall design.
- [`docs/configuration/index.md`](../../configuration/index.md) — the
  `ai_command` setting the AI assistant uses.

---

**Previous:** [06 — Org Mode and Roam](../06-org-mode-and-roam/index.md)
**Next:** [08 — HTTP Client and Tools Suite](../08-http-client-and-tools-suite/index.md)

---

Vix™ and Vix IDE™ are trademarks.
