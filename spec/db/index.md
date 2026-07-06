# DB

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

Pure logic (statement splitting, highlighting, completion, formatting, the
tree, the grid) is unit-tested in the submodules; `db::session` is tested
against in-memory `SQLite` (including transactions spanning statements), and
`tests/db_smoke.rs` drives the whole workbench end-to-end on `SQLite` files —
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

Not yet ported: SSH tunnels, keyring credential waterfalls, streaming
pagination, query cancellation, and staged cell edits. The persistent-session
prerequisite they share is now in place (`db::session`, sqlx) — so
transactions and savepoints already work by simply executing `BEGIN` /
`SAVEPOINT` / `COMMIT` / `ROLLBACK` from the query editor. The remaining
milestones are sketched in [session.md](session.md).

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

SQL replies (Ask / Optimize / Fix) route to the editor and are validated with
`EXPLAIN`; prose replies (schema question / Explain) open in the scrollable text
viewer. The reply destination is tracked as an `AiReply` on the request so one
`AiDest::Db` path serves both.

The assistant itself is whatever CLI the [`ai_command`](../ai/index.md) setting
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
`apply_ai_sql`. `tests/db_smoke.rs` drives the whole Ask → brief → reply →
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
  (`user` vs `app` for background catalog queries), colour-coded by latency;
  `Enter` reloads a statement into the editor.
- **ER diagram** (`Ctrl+E`, `db::erd`) — builds a Mermaid `erDiagram` from the
  live schema (typed entity blocks plus crow's-foot foreign-key edges) in a
  scrollable viewer; `y` yanks the Mermaid text to paste anywhere Mermaid
  renders (including Vix's own Org/Mermaid support).
