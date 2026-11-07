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
  elapsed time and `Ctrl+C` cancels (reconnecting to free the UI).
  - *Schema tree*: `←`/`→`/`Space` collapse/expand; on a table `Enter` shows
    columns and `i`/`f`/`g`/`x` show indexes / foreign keys / triggers /
    constraints; `s` shows row-count/size statistics, `D` the `CREATE`
    statement; `r` refreshes the catalog.
  - *Query editor*: syntax highlighting colored by the active Vix theme's
    `syntax` block (`keyword` / `string` / `number` / `comment`, the same
    colors the code editor uses; the spec's cyan/green/yellow/gray only as
    fallbacks); autocomplete over 80+ keywords, table names, `table.column`,
    and — right after `JOIN` — `table ON …` clauses from the foreign-key graph
    (arrows navigate, `Tab` accepts). A statement with `:name` placeholders
    prompts for each value before running.
  - *Results*: `←`/`→` scroll wide sets by column, `/` filters live; `x`
    expands the selected row vertically, `f` follows a foreign-key cell to its
    parent row, `c` charts a `(label, number)` result; the pane title shows the
    row count. On an editable table preview (one with a primary key), `i` stages
    an edit to the selected cell and `W` commits all staged edits as `UPDATE`s
    in one transaction (with conflict detection).
  - *AI & data* (see the sections below): `Ctrl+A` ask (prefix `?` for a
    schema question), `Ctrl+O` optimize, `Ctrl+F` fix the last error, `Ctrl+K`
    explain the query; `Ctrl+U` imports a CSV/TSV file into a new table.
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
  execution behind an `Enter/y` / `Esc/n` confirmation view.
- **Execute All** (`F9`) — runs every statement in order, stopping at the
  first error; the grid shows the last result.
- **EXPLAIN / EXPLAIN ANALYZE** (`F6` / `F7`; `EXPLAIN QUERY PLAN` on SQLite)
  with a plan-doctor insight flagging full table scans per engine dialect.
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

## dbjoy-inspired (planned, Run W)

**Planned, not built.** These come from a reading of
[dbjoy](https://github.com/agustind/dbjoy), a macOS PostgreSQL client; the
assessment is [`spec/comparisons/dbjoy.md`](../../../spec/comparisons/dbjoy.md).
When a task lands, its text moves out of this section into the sections
above, and its box is checked in `tasks.md`.

- **Cancel keeps the session (T774).** `Ctrl+C` asks the server to stop the
  statement instead of reconnecting: `pg_cancel_backend(pid)` on PostgreSQL
  and `KILL QUERY id` on MySQL, sent over a short-lived second connection
  with the same credentials. The session, its temporary tables and an open
  transaction survive; a cancelled statement inside a transaction leaves it
  aborted, and the `TX!` badge says so. SQLite, and any server cancel that
  fails, falls back to today's reconnect, and the status line then says the
  transaction was lost.
- **Error positions and notices (T775).** A PostgreSQL error that carries a
  position moves the editor cursor to that character of the statement.
  Server notices (`RAISE NOTICE`, warnings) are shown under the result
  instead of being dropped. A spike confirms first that sqlx's `Any` driver
  keeps both.
- **Plan tree (T776).** On PostgreSQL, `F6`/`F7` request `FORMAT JSON` and
  show the plan as a tree. Each node shows its share of the total time (with
  `ANALYZE`), the slowest nodes are highlighted, and a node whose row
  estimate is off from the actual count by 10× or more is flagged. The
  full-scan hint stays. MySQL and SQLite keep the text plan.
- **Staged inserts, deletes and expressions (T777).** Besides cell updates, a
  table preview stages new rows and row deletions, color-coded by kind. `W`
  commits them in one transaction, and every statement must affect exactly
  one row or the whole batch rolls back. Boolean, enum and `CHECK (col IN
  (…))` columns offer their values to pick from; generated columns are read
  only. A cell value starting with `=` is an SQL expression the server
  computes on commit (`=now()`); `\=` stores text that starts with `=`.
  Tables without a primary key stay read only.
- **Server-side row search (T778).** On a table preview, a filter row takes a
  column, an operator (contains, equals, not equal, range, `IN`, `IS NULL`,
  `IS NOT NULL`, raw SQL) and a value. Vix builds the `WHERE` clause with
  quoted identifiers and literals and runs it on the server, and the results
  stay editable. `/` keeps filtering the loaded rows.
- **Relations (T779).** On a table, a relations view lists outgoing foreign
  keys, incoming foreign keys (the tables that reference this one) and the
  views that depend on it, per engine.
- **Alias-aware completion (T780).** After `FROM orders o` or `JOIN customers
  AS c`, typing `o.` offers the columns of `orders`.
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
- **Table export (T784).** Export whole tables, not only the loaded rows:
  CSV, JSON or NDJSON per table, or one SQL file of `INSERT`s in
  foreign-key order with sequences reset. Rows stream to the file from one
  consistent snapshot (`REPEATABLE READ READ ONLY` on PostgreSQL, a read
  transaction on SQLite and MySQL), so a large table never sits in memory.
- **Run a `.sql` file (T786).** Open a `.sql` file (or the current buffer)
  in the workbench editor and run it, statement by statement, stopping at
  the first error. psql-only meta-commands (`\copy`, `\connect`, `\i` and
  the rest) are found before anything runs, and the run is refused with
  their line numbers.

## Headless schema read (T731)

`catalog::read_sqlite_schema(path)` reads one `SQLite` file's tables, views
and columns without the workbench: it opens the file through
`catalog::sqlite_read_only_url` (`mode=ro`, so the file is never created or
written), applies `PRAGMA query_only = ON`, and runs
`catalog::SQLITE_SCHEMA_SQL`, returning `(object, kind, column, type)` rows.
No credentials are involved. The `vix --mcp` server's `db_schema` tool uses
it ([`spec/mcp-server`](../../../spec/mcp-server/index.md)); server engines
are deliberately not offered there, since they need a password.

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
`pg_get_triggerdef`), because sqlx's `Any` driver decodes only text,
numbers, booleans and bytes. Objects owned by an extension, partitions,
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
confirmation names the target and the statement count (and its environment,
once connections carry one: `Compare::target_environment` is the marked
hook for T781). The run happens on a background thread: `BEGIN`, each
statement, `COMMIT`; on the first failure `ROLLBACK`, and the message names
the failing statement, so nothing changes. After a successful run the target
is read again and the list shows what is left (the unchecked changes).

**Limits.** One schema per compare, same engine on both sides. Not compared:
extensions, grants and owners, partitions, collations, domains and composite
types, column order, statistics and storage options. `ALTER TYPE … ADD
VALUE` runs inside the transaction (PostgreSQL 12 or later), but the new
value cannot be used by a later statement of the same migration. A dropped
serial column's sequence is dropped with `IF EXISTS`.

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
