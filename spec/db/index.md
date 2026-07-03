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
  `Esc`/`q` close. The add/edit form cycles the engine kind with `Space`.
- **Password** — masked entry, held in memory for the session only.
- **Workbench** — `Tab`/`Shift+Tab` cycle the tree / editor / results panes;
  `Ctrl+Enter` or `F5` executes the statement at the cursor; `Alt+Shift+F`
  formats it; `Esc` returns to the connections list.
  - *Schema tree*: `←`/`→`/`Space` collapse/expand; on a table `Enter` shows
    columns and `i`/`f`/`g`/`x` show indexes / foreign keys / triggers /
    constraints; `r` refreshes the catalog.
  - *Query editor*: syntax highlighting colored by the active Vix theme's
    `syntax` block (`keyword` / `string` / `number` / `comment`, the same
    colors the code editor uses; the spec's cyan/green/yellow/gray only as
    fallbacks); autocomplete over 80+ keywords, table names, and
    `table.column` (arrows navigate, `Tab` accepts).
  - *Results*: `←`/`→` scroll wide sets by column, `/` filters live, and the
    pane title shows the row count.

Pure logic (statement splitting, highlighting, completion, formatting, the
tree, the grid) is unit-tested in the submodules; `db::session` is tested
against in-memory `SQLite` (including transactions spanning statements), and
`tests/db_smoke.rs` drives the whole workbench end-to-end on `SQLite` files —
fully self-contained, no external database or CLI required.

## pgsavvy-inspired additions

A second round of features adapted from
[pgsavvy](https://github.com/davesavic/pgsavvy) (a lazygit-style PostgreSQL
TUI), fitted to Vix's process-per-query CLI bridge:

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
