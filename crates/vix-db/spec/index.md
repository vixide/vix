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
(mnemonic `Alt+D`). Engines are reached through embedded [sqlx] drivers,
each engine's own (see "Column types" below for why not sqlx's runtime `Any`
driver) — `SQLite` (bundled), and pure-Rust `PostgreSQL`/`MySQL` over
rustls — so no external client tools are needed.
Each workbench connection holds **one persistent connection** on a dedicated
worker thread (`db::session`, a current-thread tokio runtime behind blocking
channels), which means session state — temporary tables, `search_path`, and
`BEGIN`/`COMMIT`/`ROLLBACK` transactions — survives from statement to
statement. Passwords appear only in the in-memory connection URL; saved
connections persist in the settings file without any password field.

[sqlx]: https://crates.io/crates/sqlx

The overlay's screens and keys:

- **Connections** — `Enter` connect · `a` add · `e` edit · `d` delete ·
  `Esc`/`q` close. Each row shows the connection's environment badge. The
  add/edit form cycles the engine kind with `Space` and toggles the access
  (read-only default) and keyring rows the same way, and cycles the
  **Environment** row (`local` → `development` → `testing` → `staging` →
  `production`) and toggles **Share with MCP** (off by default; see
  *Shared with MCP clients* below); it also takes a **Password cmd** (a
  command that prints the password). Pasting a connection string anywhere in the form fills it (see
  *Environments and connection strings* below).
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
    statement, `R` the relations view (below), `A` copies the table for AI (see
    below); `r` refreshes the catalog;
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
    parent row, `c` charts a `(label, number)` result, `A` copies the result
    for AI as Markdown (see below); the pane title shows the
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
  - *Schema compare*: `Ctrl+D` compares the schema with another saved
    connection and writes the migration (see *Schema compare (T785)*).

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
  execution behind an `Enter/y` / `Esc/n` confirmation view, which shows every
  pending statement in full. Inside an explicit transaction a write runs
  without asking, since `ROLLBACK` can undo it — except `DROP`/`TRUNCATE`
  and anything on a `production` connection (`editor::needs_confirmation`,
  T781).
- **Execute All** (`F9`) — runs every statement in order, stopping at the
  first error; the grid shows the last result.
- **EXPLAIN / EXPLAIN ANALYZE** (`F6` / `F7`; `EXPLAIN QUERY PLAN` on SQLite)
  with a plan-doctor insight flagging full table scans per engine dialect.
- **Plan tree** (T776, from [dbjoy](https://github.com/agustind/dbjoy); module
  `plan`) — on PostgreSQL, `F6` / `F7` send `EXPLAIN (FORMAT JSON)` /
  `EXPLAIN (ANALYZE, FORMAT JSON)` and the results pane shows the plan as a
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
  `ANALYZE`, the execution time. T776 shipped with `FORMAT YAML`, because
  sqlx's `Any` driver could not decode the `json` column `FORMAT JSON`
  returns; since T787 reads every column as text, the workbench asks for
  JSON, as first planned, and tries the JSON reader first. `plan` reads both
  formats into the same model (a small reader for the YAML subset `explain.c` writes) and is
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
  *Copy for AI* (`A`, T782, below) does not relax this: it is a clipboard
  copy the user starts, and nothing from it reaches the assistant unless the
  user pastes it somewhere.
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
  was needed because sqlx's `Any` row mapping rejected `BIGINT UNSIGNED`
  before T787, and is harmless now) and the
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
  - *Positions survive the driver.* `sqlx-postgres` (through `Any` before
    T787, directly since) passes errors through unchanged, so a
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
    T787 moved the session to a typed `PgConnection`, which removes the
    second obstacle but not the first: still no public notice hook in sqlx
    0.9.0, so notices stay out of reach.
  - *Why not capture the log event.* A process-global `log` or `tracing`
    subscriber filtering that target would see every session's notices
    with nothing to tell which session or statement raised them, only at
    whatever level the subscriber enables, and Vix installs no subscriber.
    That is a hack, not a feature, so it was not done.
  - *What it would take.* A notice hook upstream in sqlx (a callback or
    channel on `PgConnection`); the session already holds a typed
    `sqlx::PgConnection` (T787), so the hook would be reachable; the worker would then forward notices as a new `Chunk` and
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

Before T787, sqlx's `Any` driver could not decode a SQLite column declared
`BOOLEAN` (or `DATE`/`DATETIME`), so previewing such a table failed before
any of this applied; T787 fixed it (see "Column types").

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

## dbjoy-inspired: environments and connection strings (T781)

From the [dbjoy](https://github.com/agustind/dbjoy) reading
([`spec/comparisons/dbjoy.md`](../../../spec/comparisons/dbjoy.md)):

- **Environments.** A saved connection carries an environment: `local`,
  `development`, `testing`, `staging` or `production` (default `local`).
  Because the published `Connection` struct cannot gain a field without a
  semver break, the environment is stored by connection name in its own
  settings table, `[db_environments]` (`prod = "production"`); a connection
  with no entry — every connection saved before T781 — is `local`. The form's **Environment** row cycles it with
  `Space`/`←`/`→`; renaming a connection carries its environment, deleting
  one drops it. The host loads it with `Browser::set_environments` and
  persists it from `Browser::take_dirty_environments`.
- **Badge.** The environment is shown as a `[label]` badge (translated) in
  the connections list, the workbench editor title and the confirmation
  view (`db::environment`). Under the monochrome built-in themes the badge
  is plain text, dim for `local`; with a custom JSON theme it takes the
  theme's `syntax` colors (`string` development, `keyword` testing, `number`
  staging, `comment` local), and production is always red.
- **Confirmation policy** (`editor::needs_confirmation`, pure and
  unit-tested). A read never asks. `DROP` and `TRUNCATE`
  (`editor::is_destructive_statement`) always ask, on any connection and
  inside a transaction. On `production` every write asks, even inside an
  explicit transaction — including committing staged cell edits (`W`, the
  confirmation lists the `UPDATE`s) and a CSV/TSV import (`Ctrl+U`, the
  confirmation lists its `CREATE TABLE` and `INSERT`s). Elsewhere a write
  asks outside a transaction only. Run All (`F9`) and `EXPLAIN ANALYZE` of a
  write keep asking everywhere. Read-only mode still refuses writes before
  any of this.
- **Connection strings** (`db::conn_string::parse`, pure, unit-tested on
  malformed input too). A bracketed paste into the add/edit form that parses
  as a connection string fills the engine, file, host, port, user, database
  and TLS-mode fields, and the name when it is empty (the database, else the
  file stem, else the host); other text is typed into the field as before.
  Accepted: `postgres://` and `postgresql://`, `mysql://` and `sqlite:` URLs
  (`sqlite:app.db`, `sqlite://app.db`, `sqlite:///abs/app.db`,
  `sqlite::memory:`) with strict percent-decoding, an optional port (1–65535),
  IPv6 hosts in brackets, the database as the path, and the query options the
  form has a row for (`sslmode`, `ssl-mode`, `ssl_mode`, and on server URLs
  `host`, `port`, `user`, `password`, `dbname`/`database`); and libpq
  `key=value` strings (`host`, `hostaddr`, `port`, `dbname`, `user`,
  `password`, `sslmode`), with single-quoted values and backslash escapes.
  Other options are listed as ignored on the status line. Several hosts, a
  bad port, a broken `%` escape or an unclosed quote are reported and fill
  nothing.
- **A pasted password is never saved.** It never reaches a form field or the
  settings file. The form holds it (the form shows a note) until it is saved
  or cancelled; a saved form hands it to an in-memory store keyed by the
  connection name, and the connection's next connect uses it exactly like a
  password typed on the prompt — so it is written to the OS keyring only
  when **Save to keyring** is on, and the credential waterfall applies on
  every later connect.

## dbjoy-inspired: Copy for AI (T782)

`A` copies, as Markdown, something to paste into a chat with an assistant
(`db::copy_ai`, its renderers pure and unit-tested; tables rendered by
`db::export`'s Markdown format, clipboard through the same path as `y`/`Y`):

- **In the results pane:** the statement behind the grid in an `sql` fence;
  a column/type table when the types are known; and the rows, from the top
  of the visible page down, in the grid's filtered and sorted order, at most
  **50** rows, under a "Rows (shown of total)" heading. Types are known for a
  table preview (`p`), from the catalog's column report; for an arbitrary
  statement the copy carries the column names only, because the driver's
  per-column type names would need a new variant on the streamed
  `session::Chunk` enum, a semver break deferred to the next major version.
- **On a schema-tree table or view:** a `## table (engine)` heading, then the
  columns, primary key, foreign keys, constraints and indexes from the
  catalog's own reports, and up to 50 preview rows.

This is a copy the user starts, of rows the user can already see; it puts
nothing anywhere but the clipboard. It does **not** change the assistant's
schema-only rule: the AI assistant (`Ctrl+A`/`O`/`F`/`K`) is still shown the
schema and never a row, because what a user chooses to paste into some chat
is the user's decision, while what Vix sends on its own must stay safe by
default.


## Headless schema read (T731)

`catalog::read_sqlite_schema(path)` reads one `SQLite` file's tables, views
and columns without the workbench: it opens the file through
`catalog::sqlite_read_only_url` (`mode=ro`, so the file is never created or
written), applies `PRAGMA query_only = ON`, and runs
`catalog::SQLITE_SCHEMA_SQL`, returning `(object, kind, column, type)` rows.
No credentials are involved. The `vix --mcp` server's `db_schema` tool uses
it ([`spec/mcp-server`](../../../spec/mcp-server/index.md)). Server engines
reach MCP clients only as saved connections the user shares (next section).

## Shared with MCP clients (T783)

The form's **Share with MCP** row (`FORM_MCP_SHARED`, value
`Browser::form_mcp_sharing`, a `mcp_share::Sharing`, off by default) marks a
saved connection for `vix --mcp`. Like T781's environments it is kept by
connection name in its own settings table, `[db_mcp_shared]`
(`app = true`), because the published `Connection` struct cannot gain a
field; the host loads it with `Browser::set_mcp_shared` and persists it from
`Browser::take_dirty_mcp_shared`. Renaming carries it, deleting drops it.

`mcp_share` is the engine side of the four MCP DB tools; the protocol rules
(log, visibility, JSON) are in
[`spec/mcp-server`](../../../spec/mcp-server/index.md#shared-database-connections-t783).

- `check_query(sql)` (pure): exactly one statement starting with `SELECT`,
  `WITH`, `EXPLAIN`, `SHOW` or `VALUES`; no `ANALYZE`, no write keyword
  (`editor::is_write_statement`'s list, named in the refusal), none of
  `REFUSED_KEYWORDS` (`INTO`, `LOCK`, `SHARE`, `COMMIT`, `ROLLBACK`,
  `SAVEPOINT`). Returns the trimmed statement or a `Refusal`.
- `password(conn)`: no password for `SQLite`, else `secret::resolve` (the
  `password_command`, then the keyring) or `ShareError::NeedsPassword` —
  never a prompt.
- `tunnel::open_batch(conn)`: `tunnel::open` with `-o BatchMode=yes` in
  front, so `ssh` fails instead of asking for a passphrase, a password or a
  host key.
- `opening_sql(kind)` (pure) and `CLOSING_SQL`: the read-only session and
  transaction with the 30-second statement timeout, then `ROLLBACK`
  (PostgreSQL `SET LOCAL statement_timeout = '30s'`, MySQL
  `SET SESSION MAX_EXECUTION_TIME = 30000`; SQLite opened `mode=ro` with
  `query_only` and a client-side deadline, because sqlx cannot interrupt
  it).
- `tables`, `describe` and `query` each open one connection on their own
  current-thread runtime (not a workbench `Session`), run `opening_sql`, the
  catalog queries or the checked statement, `ROLLBACK` and close. Rows are
  read from the stream and it stops after `ROW_LIMIT` (100) rows for a
  query or `CATALOG_ROW_LIMIT` (10,000) for a catalog read, setting
  `Rows::truncated`. A connection error is passed through
  `redact_url_secret`.
- Errors are `ShareError` (`Refused`, `NeedsPassword`, `Tunnel`, `Connect`,
  `Query`, `Timeout`, `NoSuchTable`, `Runtime`), `#[non_exhaustive]`, with
  English `Display` text: it is protocol text for an agent, like every MCP
  message. The tunnel's own errors keep their translated text.

## Schema compare (T785)

Compare one schema across two saved connections of the same engine and
write the migration that makes the **target** look like the **source**. The
source is the workbench's own connection; the target is any other saved
connection of the same kind. **PostgreSQL** (10 or later) and **SQLite** are
supported; **MySQL** is not (the workbench says so). The module tree is
`compare::{snapshot, read, diff, plan, review}`; all but `review` are pure.

**Opening it.** `Ctrl+D` in the workbench, or **DB → Schema Compare…**
(`db.schema_compare`). The first screen lists the candidate targets
(`↑`/`↓`) and the schema name, typed in place: on PostgreSQL it starts as the
schema of the tree selection, else `public`; on SQLite it is `main`. `Enter`
reads the source schema on the workbench session, then connects to the
target on a background thread through the normal credential waterfall
(`password_command`, the OS keyring, then a masked password prompt) and
reads it there. `Esc` stops waiting. A schema missing on the source is
refused; one missing on the target is created (`CREATE SCHEMA IF NOT
EXISTS`).

**Snapshots** (`compare::snapshot`, `compare::read`). A `Snapshot` is plain
data: tables (columns with type, nullability, default, generated flag,
identity and comment; primary key; unique, exclusion and check constraints;
foreign keys; indexes that do not back a constraint; comment), views and
materialized views (definition, comment, and the relations and columns they
read), functions and procedures (identity arguments and full definition),
triggers, enums (values in order) and sequences (type, start, increment,
bounds, cache, cycle). PostgreSQL is read with eleven `pg_catalog` queries
that cast every value to `text` (`format_type(…)`, `pg_get_viewdef(oid)`,
`pg_get_constraintdef`, `pg_get_indexdef`, `pg_get_functiondef`,
`pg_get_triggerdef`), because sqlx's `Any` driver, which the workbench used
until T787, decoded only text, numbers, booleans and bytes (the casts are
harmless now and were kept). Objects owned by an extension, partitions,
internal triggers and identity-owned sequences are left out. SQLite is read
from `sqlite_master` and the `table_xinfo` / `foreign_key_list` pragmas; its
unique and check constraints live inside the stored `CREATE TABLE` text, and
its view dependencies are estimated from the names in the view's SQL.

**Diff** (`compare::diff`). Objects are matched by name (routines by name and
arguments, triggers by table and name; SQLite foreign keys, which have no
names, by their definition), so a rename shows as a removal plus an
addition. Definitions are compared with whitespace collapsed. Each `Change`
is added, removed or changed, carries both definitions and its own
statements, each tagged with the phase it runs in, and two flags:

- **destructive** — drops a table or column, removes or reorders enum values,
  or rebuilds a generated column;
- **risky** — can fail on rows already in the target: `NOT NULL` added (or a
  new `NOT NULL` column without a default), a unique index, any added key or
  constraint (primary, unique, check, foreign), or a column type change
  (`ALTER COLUMN … TYPE … USING col::type`, with the default dropped first and
  restored after).

New enum values become `ALTER TYPE … ADD VALUE … AFTER/BEFORE`; removed or
reordered values become a manual note. A changed function is replaced with
its own `CREATE OR REPLACE` (which fails, and rolls back, if its return type
changed). SQLite can only add and drop columns in place: any other change to
an existing SQLite table (a column's type, nullability or default, a key)
becomes a **manual** change whose only statement is an SQL comment saying
which table to rebuild; it is never run.

**Ordering** (`compare::plan`). The chosen changes' statements run phase by
phase: create schema, create type, alter type, create/alter sequence; drop
trigger, drop view, drop foreign key, drop constraint, drop index, drop
table, drop column, drop routine; create table, add/alter column, add
constraint, create index, add foreign key; create routine, create view,
create trigger; comments; drop sequence, drop type. Inside a phase the diff's
order is kept, except that views follow their dependencies (dependents are
dropped first and created last). A target view that reads a column, table
or view the migration drops or retypes is dropped before the change and
recreated after it, from the source's definition (the target's when the
source has none), even when the view itself did not change. Duplicate
statements are dropped. The script is the header comment, `BEGIN;`, the
statements, and `COMMIT;`, with manual notes kept as comments.

**Review.** A summary line (source → target, schema, chosen/total, the
target's access), the change list (`[x]`/`[ ]`, `+`/`-`/`~`, the object kind,
`table.name`, and the flags *destructive*, *risky*, *manual*), and the
highlighted change's source and target definitions and statements.
Destructive changes start unchecked; everything else starts checked. Keys:

| Key | Action |
| --- | --- |
| `↑`/`↓`, `PgUp`/`PgDn`, `Home`/`End` | move |
| `Space` | choose or drop the change |
| `Enter` / `v` | show the whole script (scrollable; `Esc` back) |
| `y` | copy the script to the clipboard |
| `s` | save it as a `.sql` file (path prompt, prefilled `migration-<target>.sql`) |
| `o` | open it in the workbench editor and close the compare (the editor runs on the source connection; the message says so) |
| `r` | run it on the target, after a confirmation |
| `F8` | toggle write mode on the target session |
| `Esc` / `q` | close the compare and its target session |

**Running.** The target session opens with the saved connection's access, so
a read-only target refuses `r` with a message until `F8` allows writes. The
confirmation names the target, the statement count and the target's
environment badge (T781), recorded when the target is chosen. The run happens on a background thread: `BEGIN`, each
statement, `COMMIT`; on the first failure `ROLLBACK`, and the message names
the failing statement, so nothing changes. After a successful run the target
is read again and the list shows what is left (the unchecked changes).

**Limits.** One schema per compare, same engine on both sides. Not compared:
extensions, grants and owners, partitions, collations, domains and composite
types, column order, statistics and storage options. `ALTER TYPE … ADD
VALUE` runs inside the transaction (PostgreSQL 12 or later), but the new
value cannot be used by a later statement of the same migration. A dropped
serial column's sequence is dropped with `IF EXISTS`.

## Column types (T787)

Every common column type reads as text in the results grid (table previews
and free-form queries), in table exports (T784) and in the MCP `db_query`,
`db_tables` and `db_describe` tools (T783).

**Spike (sqlx 0.9.0).** The workbench opened sqlx's runtime `Any`
connection. Each engine's `Any` backend converts every row with
`AnyRow::map_from` before handing it over, and that conversion first maps
the *column's* type to `AnyTypeInfoKind`: a type outside that list fails the
whole row with `ColumnDecode { … "Any driver does not support …" }`, so it
was neither a per-cell decode error nor a row-description error, and no
fallback in our cell code could catch it. What failed:

- *PostgreSQL:* everything but `bool`, `int2/4/8`, `float4/8`, `bytea`,
  `text`, `varchar` and `citext` — so `timestamp(tz)`, `date`, `time`,
  `interval`, `numeric`, `json(b)`, `uuid`, every array, `inet`/`cidr`,
  `name`, `char(n)`, `oid`, `money`, enums and domains.
- *MySQL:* `TINYINT` (and so `BOOLEAN`), `MEDIUMINT`, `YEAR`, `DECIMAL`,
  `DATE`, `TIME`, `DATETIME`, `TIMESTAMP`, `BIT`, `SET`, `JSON` (binary
  collation) and spatial types; signed `SMALLINT`/`INT`/`BIGINT`, floats,
  text and blobs worked.
- *SQLite:* a column whose *declared* type sqlx reads as `BOOLEAN`, `DATE`,
  `TIME`, `DATETIME` or `NUMERIC` (so `DECIMAL(10,2)` too), even though the
  stored values are plain integers, reals and text.

Options weighed:

- *(a) Cast to `text` in generated SQL.* Cheap for previews, exports and
  the MCP catalog reads, but a free-form `SELECT` would still fail, and the
  export would lose typed numbers.
- *(b) A typed connection per engine* (`PgConnection`, `MySqlConnection`,
  `SqliteConnection`) behind the existing session, decoding each type
  through its own Rust type (`chrono`, `rust_decimal`, `uuid`, `serde_json`
  features). Fixes everything but needs four or five new sqlx features and
  their crates, and a formatter per type that would never quite match the
  server's own output (array literals, intervals, time zones, enums).
- *(c) Typed connections, but read the server's text.* Every workbench
  statement already runs through `raw_sql`, which PostgreSQL executes with
  the simple query protocol and MySQL with `COM_QUERY`: both send *every*
  value in its text form. With the engine's own row, `PgValueRef::as_bytes`
  (PostgreSQL) and `&[u8]` via `try_get_unchecked` (MySQL) give those bytes
  for any type, with no type-specific decoding at all.

**Chosen: (c).** The private `conn` module holds a `Conn` enum (one variant
per engine) and a `TextRow` enum (the engine's row); the session worker, the
cancel connection, the table export and the MCP read-only connection all use
it, and the sqlx `any` feature is gone (no new dependencies or features).

- *Grid text.* PostgreSQL and MySQL cells are the server's own text:
  timestamps as the server formats them (ISO 8601 with PostgreSQL's default
  `DateStyle`), `numeric` / `DECIMAL` exactly, JSON as JSON text, UUIDs
  canonical, arrays in PostgreSQL's literal form (`{1,2,3}`), enums by
  label, `inet` / `cidr` as written. PostgreSQL `bool` still shows `true` /
  `false` (its text form is `t` / `f`). `SQLite` values are read by storage
  class: a `BOOLEAN` column shows its stored `1` / `0`, a `DATE` column its
  text. Binary data (PostgreSQL `bytea` already arrives as `\x…`; MySQL
  binary strings, blobs and `BIT`; `SQLite` blobs) shows as text when it is
  valid UTF-8 and as `\x` hex otherwise, instead of replacement characters.
  `NULL` stays the empty string: the grid has no `NULL` model, and this task
  did not add one.
- *Export values.* `table_export::Value` is chosen per column type:
  integers (PostgreSQL `int2/4/8`, `oid`; MySQL integer types and `YEAR`,
  when they fit an `i64`) and floats stay numbers, PostgreSQL `bool` a
  boolean, `bytea` and MySQL binary columns bytes (written as `X'…'` /
  `'\x…'::bytea` in SQL), and everything else — `numeric` / `DECIMAL`
  included, so it stays exact — text, quoted in SQL and a string in JSON.
- *Plan tree.* With JSON readable, `F6`/`F7` on PostgreSQL ask for
  `FORMAT JSON` again; the YAML reader stays (public, and a fallback).
- *Notices (T775).* Still out of reach: the typed `PgConnection` has no
  notice hook in sqlx 0.9.0 either (see "Server notices" above).
- *Tests.* The decoding and formatting functions are unit-tested with
  hand-built PostgreSQL and MySQL values; `SQLite` is tested end to end with
  `BOOLEAN`, `DATE`, `DATETIME`, `DECIMAL(10,2)`, `JSON` and `BLOB` columns
  through the session, a free-form query and preview
  (`tui/tests/db_smoke.rs`), every export format and `db_query`. No test
  reaches a PostgreSQL or MySQL server.

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
