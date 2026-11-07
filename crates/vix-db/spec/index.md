# DB

DB workbench: connection management, schema browsing, and SQL queries.

Menu "DB" after menu "AI".

Database management, 

Connection Management
- Saved Connections - Save and manage multiple database connections
- Quick Connect - Select from saved connections, only enter password
- Secure - Passwords never saved to disk

Database Browser
- Interactive Tree View - Navigate schemas, tables, views, and functions
- Table Details - View columns, constraints, indexes, foreign keys, and triggers
- Relations - Outgoing and incoming foreign keys and dependent views per table
- Expandable - Collapse/expand schemas for easy navigation

SQL Query Editor
- Smart Query Execution
- Multi-Query Support - Write multiple queries separated by ;
- Execute at Cursor - Only executes the query where your cursor is
- Ctrl+Enter or F5 - Quick execution

Syntax Highlighting
- Color-Coded - Keywords (cyan), strings (green), numbers (yellow)
- Comments - SQL comments in gray
- Real-Time - Highlights as you type
- 🔍 Intelligent Autocomplete
- SQL Keywords - 70+ SQL keywords with prefix matching
- Table Names - Autocomplete table names from your database
- Column Names - Context-aware column suggestions
- Table.Column - Type users. to see columns from users table
- Aliases - After FROM orders o, type o. to see columns from orders
- Keyboard Navigation - Arrow keys to navigate, Tab to accept

Query Formatting
- Auto-Beautify - Press Alt+Shift+F to format query
- Proper Indentation - 4-space indentation
- Keywords Uppercase - SQL keywords in UPPERCASE
- Line Breaks - Major clauses on new lines
- Respects Semicolons - Formats only the query at cursor

Results Display
- Table View - Clean, scrollable results table
- Horizontal Scroll - Handle wide result sets
- Row Count - Shows number of rows returned
- Filter Results 
## Implementation notes

Implemented in the `crate::db` module tree; the **DB** menu sits after **AI**
(mnemonic `Alt+D`). Engines are reached through embedded [sqlx] drivers via
its runtime `Any` driver — `SQLite` (bundled), and pure-Rust
`PostgreSQL`/`MySQL` over rustls — so no external client tools are needed.
Each workbench connection holds **one persistent connection** on a dedicated
worker thread (`db::session`, a current-thread tokio runtime behind blocking
channels), which means session state — temporary tables, `search_path`, and
`BEGIN`/`COMMIT`/`ROLLBACK` transactions — survives from statement to
statement. Passwords appear only in the in-memory connection URL; saved
connections persist in the settings file without any password field.

[sqlx]: https://crates.io/crates/sqlx

The overlay's screens and keys:

- **Connections** — `Enter` connect · `a` add · `e` edit · `d` delete ·
  `Esc`/`q` close. The add/edit form cycles the engine kind with `Space` and
  toggles the access (read-only default) and keyring rows the same way; it also
  takes a **Password cmd** (a command that prints the password).
- **Password** — masked entry, held in memory for the session only. Server
  connections first try the credential waterfall — the `password_command`, then
  the OS keyring — and only prompt on a miss; a prompted password is saved to
  the keyring when **Save to keyring** is on.
- **Workbench** — `Tab`/`Shift+Tab` cycle the tree / editor / results panes;
  `Ctrl+Enter` or `F5` executes the statement at the cursor; `Alt+Shift+F`
  formats it; `Esc` returns to the connections list. `F8` toggles write mode
  (connections open read-only — see below), `Ctrl+L` opens the query log,
  `Ctrl+E` builds an ER diagram, and `Ctrl+A` / `Ctrl+O` drive the AI
  assistant. `F5` / `F6` / `F7` run asynchronously — the editor title shows the
  elapsed time and `Ctrl+C` cancels (on PostgreSQL and MySQL by asking the
  server to stop the statement, keeping the session; see "Cancel and error
  positions" below).
  - *Schema tree*: `←`/`→`/`Space` collapse/expand; on a table `Enter` shows
    columns and `i`/`f`/`g`/`x` show indexes / foreign keys / triggers /
    constraints; `s` shows row-count/size statistics, `D` the `CREATE`
    statement, `R` the relations view (below); `r` refreshes the catalog;
    `m` marks a table and `e` exports the marked tables (or the one under the
    cursor) whole — see *Table export* below.
  - *Relations* (`R`, module `db::relations`, T779): on a table or view, the
    grid lists one row per relation with four columns — relation (outgoing,
    incoming or view), constraint name, from and to, each side written
    `table (col, …)` with multi-column keys in key order and a table in
    another schema written `schema.table`. Outgoing rows are the table's own
    foreign keys, incoming rows the keys on other tables that reference it,
    and view rows the views reading from it. Catalog queries per engine:
    PostgreSQL reads `pg_constraint` (pairing `conkey`/`confkey` with
    `unnest … WITH ORDINALITY`) and finds dependent views and materialized
    views through `pg_depend` and each view's `pg_rewrite` rule; MySQL reads
    `information_schema.KEY_COLUMN_USAGE` rows with `REFERENCED_TABLE_NAME`
    set, and `VIEW_TABLE_USAGE` for views (8.0.13+; when that query fails it
    falls back to searching `VIEWS.VIEW_DEFINITION` for the backtick-quoted
    table name); SQLite runs `pragma_foreign_key_list` on the table and over
    every table for incoming keys (a `REFERENCES t` without columns resolves
    to `t`'s primary key). SQLite records no view dependencies, so its view
    rows are **approximate**: every view's SQL is tokenized and kept when it
    names the table outside strings and comments, so a column or alias
    spelled like the table also counts. Names enter the queries only as
    escaped string literals. The builders and row grouping are pure and
    unit-tested per engine; `tui/tests/db_smoke.rs` covers SQLite end to end.
  - *Query editor*: syntax highlighting colored by the active Vix theme's
    `syntax` block (`keyword` / `string` / `number` / `comment`, the same
    colors the code editor uses; the spec's cyan/green/yellow/gray only as
    fallbacks); autocomplete over 80+ keywords, table names, `table.column`,
    and — right after `JOIN` — `table ON …` clauses from the foreign-key graph
    (arrows navigate, `Tab` accepts). Qualifiers resolve table aliases bound
    anywhere in the statement under the cursor, even on another line (T780,
    module `db::alias`): after `FROM orders o`, `FROM orders AS o`,
    `JOIN customers c`, `JOIN customers AS c`, `UPDATE orders o`, or a comma
    join (`FROM a x, b y`), typing `o.` offers the columns of `orders`.
    Schema-qualified (`public.orders o`, and `public.orders.` without an
    alias) and quoted names (`"Order Items" oi`, `` `orders` o ``) resolve
    too. Clause keywords (`WHERE`, `ON`, `USING`, `LEFT`, `JOIN`, `GROUP`,
    `NATURAL`, …) are never taken for an alias unless written after `AS` or
    quoted; strings and comments are skipped; derived tables and table
    functions (`FROM (SELECT …) s`) bind nothing, though aliases inside the
    subquery still resolve. When two bindings share an alias, the last one
    written wins. A statement with `:name` placeholders
    prompts for each value before running.
  - *Results*: `←`/`→` scroll wide sets by column, `/` filters live; `x`
    expands the selected row vertically, `f` follows a foreign-key cell to its
    parent row, `c` charts a `(label, number)` result; the pane title shows the
    row count. On an editable table preview (one with a primary key), `i` stages
    an edit to the selected cell, `a` stages a new row, `d` stages the selected
    row's deletion, `u` discards the row's staged changes, and `W` commits them
    all in one transaction; `F` searches the table's rows on the server (see
    *dbjoy-inspired* below).
  - *AI & data* (see the sections below): `Ctrl+A` ask (prefix `?` for a
    schema question), `Ctrl+O` optimize, `Ctrl+F` fix the last error, `Ctrl+K`
    explain the query; `Ctrl+U` imports a CSV/TSV file into a new table;
    `Ctrl+P` loads a `.sql` file into the editor and runs it (see *Running a
    `.sql` file* below).

Pure logic (statement splitting, highlighting, completion, formatting, the
tree, the grid) is unit-tested in the submodules; `db::session` is tested
against in-memory `SQLite` (including transactions spanning statements), and
`tui/tests/db_smoke.rs` drives the whole workbench end-to-end on `SQLite` files —
fully self-contained, no external database or CLI required.

## pgsavvy-inspired additions

A second round of features adapted from
[pgsavvy](https://github.com/davesavic/pgsavvy) (a lazygit-style PostgreSQL
TUI), fitted to Vix's persistent sqlx session (`db::session`):

- **Query history** (`Ctrl+R`, module `db::store`) — every executed statement
  lands in `db_history.toml` (deduplicated, newest first, capped at 200);
  `Enter` re-inserts an entry into the editor, `d` deletes it.
- **Saved queries** (`Ctrl+B` to browse, `Ctrl+S` to save the statement at the
  cursor under a name) — persisted in `db_queries.toml`.
- **Write/DDL confirmation** — `is_write_statement` scans keywords outside
  strings/comments (so a CTE like `WITH … DELETE` is caught) and gates
  execution behind an `Enter/y` / `Esc/n` confirmation view.
- **Execute All** (`F9`) — runs every statement in order, stopping at the
  first error; the grid shows the last result.
- **EXPLAIN / EXPLAIN ANALYZE** (`F6` / `F7`; `EXPLAIN QUERY PLAN` on SQLite)
  with a plan-doctor insight flagging full table scans per engine dialect.
- **Plan tree** (T776, from [dbjoy](https://github.com/agustind/dbjoy); module
  `plan`) — on PostgreSQL, `F6` / `F7` send `EXPLAIN (FORMAT YAML)` /
  `EXPLAIN (ANALYZE, FORMAT YAML)` and the results pane shows the plan as a
  tree, one row per node in pre-order with its title indented by depth (node
  type, join type, index and relation with alias, `Update on t` for a
  `ModifyTable`, `InitPlan 1: …` for an init plan or sub-plan) and its key details (index /
  recheck / hash / merge conditions, join filter, filter, sort and group
  keys, rows removed). Plain `EXPLAIN` adds estimated rows and cost; with
  `ANALYZE` the columns are time share, self time, estimated rows, actual
  rows, loops and a note. A node's **self time** is its actual total time ×
  loops minus its direct children's (clamped at zero), and its **share** is
  that over the sum of every node's self time. Up to the 3 slowest nodes with
  at least 10% of the time are noted and drawn bold red; a node whose row
  estimate (per loop, counts below one taken as one) is off from the actual
  by 10× or more either way is noted with the factor and drawn yellow; a node
  that never ran is noted. The full-scan hint still applies (the titles say
  `Seq Scan`); otherwise the status line gives the node count and, with
  `ANALYZE`, the execution time. YAML, not the planned `FORMAT JSON`,
  because sqlx's `Any` driver cannot decode the `json` column `FORMAT JSON`
  returns while YAML comes back as `text`; `plan` reads both formats into the
  same model (a small reader for the YAML subset `explain.c` writes) and is
  unit-tested on hand-recorded JSON and YAML plans. A plan that does not parse
  is left as the raw text. MySQL and SQLite keep the text plan unchanged, and
  the AI assistant's validation and optimize paths still use the text
  `EXPLAIN` everywhere. `EXPLAIN ANALYZE` of a write still needs write mode
  and the confirmation, and once confirmed it is shown as a plan too.
- **Table preview** — `p` on a tree table/view runs `SELECT * … LIMIT 200`
  with engine-correct identifier quoting.
- **Tree search** — `/` filters the schema tree live, ignoring collapse state.
- **Results grid** — `←`/`→` select a column, `s` cycles a numeric-aware sort
  (▲/▼ in the header), `y`/`Y` yank the cell/row (TSV) to the clipboard, `v`
  opens a full-content cell viewer with a JSON pretty-print toggle (`p`), and
  `e` exports the filtered+sorted view as CSV, TSV, JSON, NDJSON, Markdown,
  or SQL `INSERT`s to a file or the clipboard (module `db::export`).

SSH tunnels, keyring credential waterfalls, streaming pagination, query
cancellation, and staged cell edits (M2–M7) are all done too — transactions
and savepoints work by simply executing `BEGIN` / `SAVEPOINT` / `COMMIT` /
`ROLLBACK` from the query editor. See [session.md](session.md) for the
milestone-by-milestone detail.

## surus-inspired: the AI SQL assistant

A natural-language → SQL surface adapted from
[surus](https://github.com/Geometrein/surus) (an AI-first PostgreSQL client). It
keeps surus's three safety principles, fitted to Vix's assistant model:

- **Schema-only privacy.** The assistant is shown the *shape* of the database —
  tables, columns and their types, and foreign-key edges — but never a single
  row of data. The schema brief and the user's question travel on the assistant
  CLI's **stdin**; only a fixed instruction goes on its command line, so no
  user-supplied text is ever interpolated into a shell command.
- **Read-only by default.** A connection opens read-only (see *Read-only,
  query log & ERD* below), and the brief tells the model "READ-ONLY: answer with
  a single `SELECT`." Defense in depth: if the model ignores that, the generated
  write is still refused at the read-only gate on `F5`, and the validation step
  uses `EXPLAIN` (never `EXPLAIN ANALYZE`), which cannot execute a write.
- **Draft → EXPLAIN → iterate.** Generated SQL is dropped into the editor and
  immediately validated with `EXPLAIN`; the plan-doctor insight flags full
  scans, and `Ctrl+O` feeds the query *plus its own plan* back to the assistant
  for a faster rewrite — surus's tight text-to-SQL loop.

Keys, in the Workbench:

- **`Ctrl+A` — Ask AI.** Opens a prompt; type a question ("orders per user this
  month"), `Enter` sends it. The recovered SQL lands in the editor with its plan
  shown; `F5` runs it (subject to the read-only gate). `Esc` cancels. Prefixing
  the input with `?` ("`?which tables reference users`") switches to a
  **schema question** answered in plain English instead of SQL.
- **`Ctrl+O` — Optimize.** Sends the statement at the cursor together with its
  `EXPLAIN` plan, asking for an equivalent query that avoids full scans.
- **`Ctrl+F` — Fix error.** After a query fails, sends the failed statement and
  the database's error message (with the schema) and drops the corrected query
  in the editor.
- **`Ctrl+K` — Explain.** Explains the statement at the cursor in plain English
  in the text viewer (no SQL run).

With `ai = "off"` (T730) every one of these requests fails in the workbench
unsent, and Vix warns that AI is off (`crates/vix-ai-panel/spec/ai/index.md`).

SQL replies (Ask / Optimize / Fix) route to the editor and are validated with
`EXPLAIN`; prose replies (schema question / Explain) open in the scrollable text
viewer. The reply destination is tracked as an `AiReply` on the request so one
`AiDest::Db` path serves both.

The assistant itself is whatever CLI the [`ai_command`](../../vix-ai-panel/spec/index.md) setting
names (default `claude -p "{prompt}"`) — the same command the **AI** menu and
chat panel use, so the surface is provider-agnostic and needs no API-key
handling of its own. Requests run in the background on the shared
`spawn_ai` machinery: the reply is polled each event-loop tick, the SQL is
recovered from it (a fenced ```` ```sql ```` block, else leading prose is
stripped to the first SQL keyword), and the editor title shows an `AI…` badge
while a request is in flight. Only one AI request runs at a time.

The prompt/brief builders and SQL recovery are the pure, unit-tested `db::ai`
module (`instruction` / `context` / `optimize_context` / `extract_sql`). The
`Browser` tracks the request lifecycle as an `AiState` (`Idle` → `Pending` →
`Running`) and hands the host an `AiRequest` (command-line prompt plus stdin
context) through `take_ai_request`, mirroring the `take_dirty_*` stores; the
host spawns it as `AiDest::Db` and routes the reply back through
`Browser::apply_ai_reply`. `tui/tests/db_smoke.rs` drives the whole Ask → brief → reply →
editor flow on a `SQLite` file and asserts that seeded row data never reaches
the brief.

## surus-inspired: read-only, query log & ERD

The same surus round added three companion features the AI surface builds on:

- **Read-only by default.** A saved connection carries a `writable` flag that
  defaults off, so a fresh (or legacy) connection opens read-only. Enforcement
  is two-layered: at the database level a session pragma
  (`PRAGMA query_only` on SQLite, `SET default_transaction_read_only` on
  PostgreSQL; MySQL relies on the client guard), and at the client level the
  `is_write_statement` guard refuses writes until write mode is on. `F8` toggles
  write mode for the session (re-issuing the pragma), and the connection form's
  **Access** row sets the saved default. The editor pane title shows a
  `read-only` / `read-write` badge.
- **Query log** (`Ctrl+L`, `db::store::Log`) — an in-memory, session-scoped log
  of every execution with its duration, row count, outcome, and origin
  (`user` vs `app` for background catalog queries), color-coded by latency;
  `Enter` reloads a statement into the editor.
- **ER diagram** (`Ctrl+E`, `db::erd`) — builds a Mermaid `erDiagram` from the
  live schema (typed entity blocks plus crow's-foot foreign-key edges) in a
  scrollable viewer; `y` yanks the Mermaid text to paste anywhere Mermaid
  renders (including Vix's own Org/Mermaid support).

## Cancel and error positions (from dbjoy)

Built from the dbjoy assessment
([`spec/comparisons/dbjoy.md`](../../../spec/comparisons/dbjoy.md)), rows 1
and 2.

- **Cancel keeps the session (T774).** When a session connects, the worker
  reads the server's id for the connection (`SELECT pg_backend_pid()` on
  PostgreSQL, `SELECT CAST(CONNECTION_ID() AS SIGNED)` on MySQL; the cast is
  needed because sqlx's `Any` row mapping rejects `BIGINT UNSIGNED`) and the
  session keeps a `CancelHandle`: its connection URL plus the cancel
  statement, `SELECT pg_cancel_backend(<pid>)` or `KILL QUERY <id>`
  (`session::backend_id_sql`, `session::cancel_sql`, pure and unit-tested).
  `Ctrl+C` on a running statement sends that statement from a background
  thread over a short-lived second connection with the same URL (through the
  SSH tunnel when there is one), bounded by a 10-second timeout, and the
  status line says the server was asked (`msg.db_cancelling_server`). The
  session is not touched: the statement comes back as an error with SQL state
  `57014` (PostgreSQL) or `70100` (MySQL), which the workbench reports as
  cancelled rather than as an error to fix. Temporary tables, session
  settings and an open transaction survive; a cancelled statement inside a
  transaction marks it aborted (`TX!`), and the status line asks for a
  rollback (`msg.db_cancelled_tx_aborted`). PostgreSQL really requires the
  rollback; MySQL keeps the transaction usable after `KILL QUERY`, but the
  client mirror marks any failure inside a transaction aborted, as it always
  has.
- **Fallback: reconnect.** There is no server cancel on SQLite (in-process,
  no backend id), nor when the id could not be read at connect. A server
  cancel that fails (the second connection cannot be opened, the statement
  is rejected, `pg_cancel_backend` answers `false`, or the timeout passes)
  falls back too, and so does a second `Ctrl+C` while a server cancel is
  already under way (a hung network read the server cancel cannot reach).
  The fallback is the earlier behavior: abandon the worker and reconnect in
  the background. The transaction and session state are lost, and when a
  transaction was open the status line says so
  (`msg.db_cancelled_tx_lost`); otherwise it reads `msg.db_cancelled`.
- **Error positions (T775).** A PostgreSQL error that carries a position
  (the `P` field of the error response) moves the editor cursor to that
  character of the statement and focuses the editor. The position is
  1-based in characters of the SQL that was sent; the workbench maps it back
  through the statement's offset in the buffer and, for `F6`/`F7`, past the
  `EXPLAIN …` prefix it added. A position inside that prefix, a position into
  an internally generated query (a PL/pgSQL body), and a statement whose bind
  parameters were substituted (the sent text no longer matches the buffer)
  leave the cursor where it is. `F9` (run all) does not move the cursor.
  MySQL and SQLite errors carry no position.
- **Server notices: not built (T775 spike).** Notices (`RAISE NOTICE`,
  warnings) cannot be reached through sqlx 0.9.0, so they are still dropped.
  The spike found:
  - *Positions survive the `Any` driver.* `sqlx-postgres`'s
    `AnyConnectionBackend::fetch_many` passes errors through unchanged, so a
    `sqlx::Error::Database` still boxes the engine's `PgDatabaseError`, and
    `try_downcast_ref::<sqlx::postgres::PgDatabaseError>()` followed by
    `position()` gives `PgErrorPosition::Original(n)`. The worker stores
    it before it sends the error, and `Session::error_position` reads it; it
    is not a `QueryError` field because adding one to that public struct
    would break semver after 2.0.
  - *Notices never leave the connection.* `PgStream::recv` (the connection
    stream module of `sqlx-postgres` 0.9.0) consumes every
    `NoticeResponse` itself: it emits a `log`/`tracing` event with target
    `sqlx::postgres::notice` and reads the next message. `PgConnection` has
    no notice handler or channel, and `AnyConnection` would not expose one
    anyway (its backend is a private `Box<dyn AnyConnectionBackend>`).
  - *Why not capture the log event.* A process-global `log` or `tracing`
    subscriber filtering that target would see every session's notices
    with nothing to tell which session or statement raised them, only at
    whatever level the subscriber enables, and Vix installs no subscriber.
    That is a hack, not a feature, so it was not done.
  - *What it would take.* A notice hook upstream in sqlx (a callback or
    channel on `PgConnection`), and a PostgreSQL session that holds a typed
    `sqlx::PgConnection` instead of `AnyConnection` so the hook is
    reachable; the worker would then forward notices as a new `Chunk` and
    the results pane would list them under the grid. MySQL warnings are a
    separate path: the warning count in the OK packet is not exposed through
    `Any` either, but a `SHOW WARNINGS` after each MySQL statement would work
    today, at one extra round trip per statement; it is left for a later
    task.

## dbjoy-inspired: staged row changes and row search

From the reading of [dbjoy](https://github.com/agustind/dbjoy) recorded in
[`spec/comparisons/dbjoy.md`](../../../spec/comparisons/dbjoy.md).

### Staged inserts, deletes and smarter cells (T777)

A table preview whose table has a primary key is editable; one without a
primary key, and any other result, stays read only. Changes are staged in
`db::staging::Staged`, keyed by the grid's underlying row index (stable under
`/` filtering and sorting), and nothing reaches the database until `W`:

| Key (results pane) | What it does |
| --- | --- |
| `i` | edit the selected cell; `Enter` stages the value, `Esc` cancels |
| `a` | stage a new, blank row at the end of the grid and select it |
| `d` | stage the selected row's deletion, or restore it; on a new row, discard it |
| `u` | discard every staged change to the selected row |
| `W` | commit everything staged, in one transaction |

The grid colors a row by what is staged for it: the changed cells of a
modified row bold yellow, a new row bold green, a row staged for deletion red
and struck through. A deleted row cannot be edited until it is restored, and
its cell edits come back with it.

**Commit.** `db::staging::statements` builds one statement per changed row, in
the order `DELETE`s, `UPDATE`s (all of a row's edited cells in one `SET`),
`INSERT`s, so a deleted row's key is free before a new row reuses it. Rows are
found by their full primary key; a row whose key cannot be resolved is skipped
rather than risk a broader `WHERE`. Before the transaction, every row an
`UPDATE` will touch is re-read and compared with the values loaded into the
grid (the optimistic conflict check, `msg.db_edit_conflict`). Then `BEGIN`,
each statement through `Session::execute`, and `COMMIT`. If any statement
fails, or affects other than exactly one row (`msg.db_edit_not_one_row`,
naming the statement), the whole batch is rolled back and stays staged. A new
row's blank cells are left out of its `INSERT`, so they take the column
default; a new row with no values is `INSERT … DEFAULT VALUES` (`() VALUES
()` on MySQL). Committing needs write mode, then re-runs the preview (with its
row search, if any).

**Values.** `db::staging::value_sql` turns a typed value into SQL:

- a value starting with `=` is an SQL expression the server computes on
  commit, inserted unquoted: `=now()`, `=upper('x')`, `=NULL`;
- `\=` stores literal text that starts with `=` (`\=x` stores `=x`); a lone
  `=` is the text `=`;
- anything else is a literal from `catalog::quote_value`: always quoted on
  PostgreSQL (an untyped literal takes the column's type) and MySQL (where
  backslashes are doubled too, since they escape in the default `sql_mode`),
  and a bare number on SQLite, as before.

**Column facts.** When a preview becomes editable, `db::columns` reads each
column's type and whether the server generates it, with best-effort catalog
queries (a failing one just means free-text cells):

- *Generated columns* (SQLite `pragma_table_xinfo` hidden 2/3, PostgreSQL
  `attgenerated` and `GENERATED ALWAYS` identities, MySQL `VIRTUAL GENERATED` /
  `STORED GENERATED`) are read only: `i` on one says so, and an `INSERT` leaves
  them out.
- *Pick lists.* In the cell editor, `↑`/`↓` cycle through a column's allowed
  values, listed under the input with the current one highlighted; typing
  still works. The values come from a PostgreSQL enum (`pg_enum`, in sort
  order), a MySQL `ENUM(…)` column type, a single-column `CHECK (col IN (…))`
  (parsed by `columns::parse_in_check`, including PostgreSQL's `col = ANY
  (ARRAY[…])` rewrite and MySQL's `_utf8mb4'…'` introducers; read from
  `pg_get_constraintdef`, from `information_schema.check_constraints` on MySQL
  8.0.16 and later, and from the table's `CREATE` statement on SQLite), or a
  boolean column (`true`/`false` on PostgreSQL, `1`/`0` for MySQL
  `tinyint(1)` and SQLite `BOOLEAN`).

sqlx's `Any` driver cannot decode a SQLite column declared `BOOLEAN` (or
`DATE`/`DATETIME`), so previewing such a SQLite table fails before any of this
applies; that predates T777 and is a driver limitation.

### Server-side row search (T778)

On a table preview, `F` opens a filter row: a **column** (starting on the
selected one), an **operator** and a **value**. `↑`/`↓` move between the
three, `←`/`→` choose the column or operator, typing edits the value, `Enter`
runs the search, `Delete` clears it (back to the plain preview) and `Esc`
closes. Under the form, the `WHERE` condition it builds is shown live, or why
it is not complete yet.

`db::row_filter::where_clause` is the pure builder. The column is quoted with
`catalog::quote_ident` and values with `catalog::quote_value`, so a value can
never leave its literal:

| Operator | Condition |
| --- | --- |
| contains | `CAST(col AS TEXT) ILIKE '%v%'` on PostgreSQL; `LOWER(CAST(col AS CHAR/TEXT)) LIKE LOWER('%v%')` on MySQL and SQLite; `%`, `_` and `!` escaped with `ESCAPE '!'` |
| `=`, `<>`, `<`, `<=`, `>`, `>=` | `col op v` |
| between | `col BETWEEN low AND high`, typed `low..high` |
| `IN` | `col IN (a, b, …)` from a comma-separated list; an item may be single- or double-quoted to hold a comma |
| `IS NULL`, `IS NOT NULL` | no value |
| raw SQL | the value is the whole condition, as written |

The preview is re-run as `SELECT * … WHERE <condition> LIMIT 200`, so it finds
rows the capped preview never loaded, and the result stays editable (a commit
re-runs the same search). The results title shows the active condition. A
search is refused while changes are staged (the re-run would drop them), and
any search whose SQL writes (`editor::is_write_statement`, which only a raw
condition can trigger) is refused whatever the write mode. `/` still filters
the loaded rows client-side.

Tests: the builders are unit-tested per engine in `staging`, `columns` and
`row_filter`, and `tui/tests/db_smoke.rs` runs stage → commit, the
exactly-one-row rollback (a `DELETE` whose row someone else removed), and the
row search end to end on SQLite.

## dbjoy-inspired: table export and running files

From the same reading of [dbjoy](https://github.com/agustind/dbjoy) as the
planned items below ([`spec/comparisons/dbjoy.md`](../../../spec/comparisons/dbjoy.md)).

### Table export (T784)

The results-grid export (`e` in results) writes the rows already loaded.
`e` in the **schema tree** exports *whole tables*: the tables marked with `m`
(a toggle; marks are kept until disconnect), or the table under the cursor
when none is marked. Views and functions are not exported. The dialog
(`View::Export` with `Browser::table_export` set) cycles the format with
`←`/`→` and takes a destination path; `Enter` starts, `Esc` closes.

| Format | Output |
| ------ | ------ |
| CSV | One `table.csv` per table in the chosen directory (created if missing), header row, `NULL` as an empty field |
| JSON | One `table.json` per table: an array of objects; `NULL`, numbers and booleans stay typed |
| NDJSON | One `table.ndjson` per table: one object per line, typed like JSON |
| SQL | One file: `BEGIN;`, one `INSERT` per row for every table, the sequence resets, `COMMIT;` |

When the chosen tables span schemas, per-table files are named
`schema.table.ext`; characters other than letters, digits, `-`, `_` and `.`
become `_`, so a name never leaves the directory. The default destination is
`vix-export` (a directory) or `vix-export.sql`, and follows the format while
it is unedited.

**SQL ordering.** Tables are ordered by the foreign keys among them (from
`catalog::relationships_sql`): a table comes after every table it
references, ties in the chosen order. Self-references and keys to tables
outside the export do not constrain the order (rows *within* a
self-referencing table stay in table order). If the keys form a cycle there
is no valid order: the tables keep the chosen order and the file's header
comment says so and asks to load with foreign-key checks off; the status
line says so too. Generated columns are left out of `INSERT`s. Values keep
their decoded type: `NULL`, bare numbers, `TRUE`/`FALSE`, quoted text (with
`\` doubled on MySQL), binary as `X'…'` (`'\x…'::bytea` on PostgreSQL).
`INSERT`s name the table schema-qualified on PostgreSQL and unqualified on
SQLite and MySQL (a MySQL schema is the database, so the dump loads into a
database of another name). The comments in the file are fixed English, like
the SQL, so a dump does not depend on the UI language.

**Sequence reset**, at the end of the SQL file, read inside the snapshot:

| Engine | Read | Written |
| ------ | ---- | ------- |
| PostgreSQL | each column with an owned sequence (`pg_get_serial_sequence`: serial and identity) | `SELECT setval('seq', COALESCE(MAX(col), 1), MAX(col) IS NOT NULL) FROM t;` |
| MySQL | `information_schema.tables.AUTO_INCREMENT` | `ALTER TABLE t AUTO_INCREMENT = n;` |
| SQLite | `sqlite_sequence.seq` (none without an `AUTOINCREMENT` table) | `DELETE FROM sqlite_sequence WHERE name = 't';` + `INSERT INTO sqlite_sequence …` |

**One snapshot, streamed, on a second connection.** The export opens its own
read-only connection from the session's URL (same credentials, same SSH
tunnel) on a background thread, so **an open user transaction is never
touched** — and its uncommitted changes are not exported — and the workbench
stays usable while it runs. It reads every table inside one read-only
transaction: `BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY` on
PostgreSQL; `SET TRANSACTION ISOLATION LEVEL REPEATABLE READ` then `START
TRANSACTION WITH CONSISTENT SNAPSHOT, READ ONLY` on MySQL; `BEGIN` on SQLite.
It ends with `COMMIT` (`ROLLBACK` after a failure). Each table is one
`SELECT` whose rows are written to the file as the driver yields them — no
`MAX_ROWS` cap, nothing accumulated — and progress reaches the status line
every 512 rows. Each file is written as `name.part` and renamed when
complete; a failure removes the partial file and reports the error (passwords
redacted). An in-memory SQLite database is refused, since a second
connection would see a different, empty database. Disconnecting, or closing
the DB overlay, drops the export: it stops at its next progress report,
rolls back and removes its partial file.

Code: `db::table_export` — the pure, unit-tested pieces (`fk_order`,
`snapshot_begin_sql`, `column_names_sql`, `select_sql`, `sequences_sql`,
`sequence_reset_sql`, `sql_literal`, `insert_sql`, `csv_line`,
`json_object`, `file_name`), the worker, and the `Browser` methods
(`toggle_export_mark`, `open_table_export`, `poll_table_export`,
`table_export_running`); the host polls it with the query in
`App::poll_db_query`.

### Running a `.sql` file (T786)

`Ctrl+P` in the workbench opens a path prompt (`View::Import` with
`Browser::import_is_sql_file`); `Enter` reads the file, loads it into the
query editor (replacing its text) and runs it with **Execute All** — every
statement in order, stopping at the first error, through the usual
read-only refusal and write confirmation. **DB → Run Buffer in Workbench**
(`db.run_buffer`, also in the palette) does the same with the active editor
buffer, unsaved changes included; with no connection it opens the
connections list. Both end in `Browser::run_script`.

Execute All first scans the editor for **psql meta-commands**
(`editor::meta_commands`): lines whose first non-blank character is a
backslash outside strings, quoted identifiers, `--` and (nested) `/* */`
comments and dollar-quoted bodies — `\copy`, `\connect`, `\c`, `\i`,
`\set`, and the rest. They are psql client commands, not SQL, so if any is
found nothing runs and the status line lists their line numbers; the
script stays in the editor to be fixed. The check applies to `F9` on any
buffer, not only to loaded files.

## dbjoy-inspired (planned, Run W)

**Planned, not built.** These come from a reading of
[dbjoy](https://github.com/agustind/dbjoy), a macOS PostgreSQL client; the
assessment is [`spec/comparisons/dbjoy.md`](../../../spec/comparisons/dbjoy.md).
When a task lands, its text moves out of this section into the sections
above, and its box is checked in `tasks.md`.

- **Environments and connection strings (T781).** A saved connection carries
  an `environment`: `local`, `development`, `testing`, `staging` or
  `production` (default `local`), shown as a colored badge in the list and
  the editor title. On `production`, every write shows its SQL and asks,
  even inside an explicit transaction; `DROP` and `TRUNCATE` always ask on
  any connection. Pasting a connection string into the add form fills it: a
  `postgres://`, `postgresql://`, `mysql://` or `sqlite:` URL, or a libpq
  `key=value` string. A password in the string is never saved; it goes to
  the keyring prompt like a typed one.
- **Copy for AI (T782).** In results, a key copies the statement, its column
  names and types, and the selected rows (or the visible page, capped) as
  Markdown. On a table it copies the columns, keys, constraints and indexes
  with the rows. It is an explicit, user-initiated copy, so it does not
  change the assistant's schema-only rule: the assistant itself still never
  sees a row.
- **Schema compare (T785).** Compare one schema across two saved
  connections, PostgreSQL first: tables, columns, keys, indexes, checks,
  views, functions, triggers, enums and sequences. Each difference shows
  both definitions and its migration SQL. The whole migration is ordered so
  it runs (types before the tables that use them, drops before creates,
  foreign keys last). Changes that delete data start unchecked; changes that
  can fail on existing rows (`NOT NULL`, a unique index, a type change) are
  flagged. The script can be copied, saved, opened in the editor or run on
  the target in one transaction, after a confirmation that names the
  target's environment.

## Headless schema read (T731)

`catalog::read_sqlite_schema(path)` reads one `SQLite` file's tables, views
and columns without the workbench: it opens the file through
`catalog::sqlite_read_only_url` (`mode=ro`, so the file is never created or
written), applies `PRAGMA query_only = ON`, and runs
`catalog::SQLITE_SCHEMA_SQL`, returning `(object, kind, column, type)` rows.
No credentials are involved. The `vix --mcp` server's `db_schema` tool uses
it ([`spec/mcp-server`](../../../spec/mcp-server/index.md)); server engines
are deliberately not offered there, since they need a password.

## Errors (T610)

`session::SessionError`; `Session::connect`, `send`, `restart` and `run`, and
`catalog::read_sqlite_schema`, return it instead of `String`:

| Variant | Meaning |
| --- | --- |
| `Spawn(io::Error)` | the worker thread could not be started |
| `Connect { message }` | the connection could not be opened (credentials redacted), or the worker exited before reporting ready |
| `Setup { message }` | a setup statement (such as the read-only pragma) failed |
| `NotConnected` | the worker is gone, or no session is open; `Display` is the translated `msg.db_not_connected` |
| `Query(QueryError)` | the statement failed; keeps the classified `QueryError` |

The type is `#[non_exhaustive]` with a hand-written `Display`/`Error` (no
`thiserror`). `Display` is the same text the earlier `String` errors carried,
so the workbench status lines are unchanged. `Browser::run_sql` and
`run_catalog` return it too; the workbench turns it into text only at the
point it shows a message.
