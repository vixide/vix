# dbjoy compared with the Vix DB workbench

[dbjoy](https://github.com/agustind/dbjoy) is a native macOS PostgreSQL
client in Swift (SwiftUI, with AppKit for the grid and editor), MIT
licensed, created 2026-10-02 and read here at its 2026-11-04 state. It calls
itself "the AI-first PostgreSQL client for the Mac". It talks to the server
through libpq and bundles `pg_dump` and `psql` for backups and database
copies.

This page records what was read, what Vix already has, what Vix should take,
and what it should not. The resulting work is Run W in
[`tasks.md`](../../tasks.md) (T774–T786) and [`plan.md`](../../plan.md). The
owning specs are [`crates/vix-db/spec`](../../crates/vix-db/spec/index.md)
and [`spec/mcp-server`](../mcp-server/index.md).

Method: the README and the engine-neutral `DBCore` sources (editing, plan,
schema diff, export, completion, connection strings) were read, plus the MCP
server and the database copier. Each candidate was then checked against
Vix's code in `crates/vix-db/src`, not against its spec alone.

## What Vix already has

These were dropped from the list: saved connections without stored
passwords, an OS-keyring and password-command waterfall, SSH tunnels,
read-only connections enforced by the server, query history and saved
queries, run at cursor and run all, a SQL formatter, `EXPLAIN` and
`EXPLAIN ANALYZE` (a write needs write mode and a confirmation), a table
preview, live tree search, an ER diagram (Mermaid), following a foreign-key
cell to its row, a full-cell viewer with JSON pretty-printing, sorting,
result export (CSV, TSV, JSON, NDJSON, Markdown, SQL `INSERT`), CSV import,
`:name` parameters, a transaction badge, and AI fix, explain and optimize.

Vix also does things dbjoy does not: SQLite and MySQL as well as
PostgreSQL, with no external client tools, and an AI assistant that is shown
the schema and never a row of data.

## Gaps worth closing

| # | dbjoy has | Vix today | Task |
| --- | --- | --- | --- |
| 1 | Cancel through libpq: the session and its transaction survive | `Ctrl+C` reconnects, and "transaction state is lost" (`execute.rs`) | T774 |
| 2 | Server error positions move the cursor; server notices are shown | Neither | T775 |
| 3 | Plan as a tree: share of time per step, slowest steps highlighted, estimates off by 10× or more flagged | Plan text plus a full-scan hint | T776 |
| 4 | Staged inserts and deletes as well as updates; each statement must affect exactly one row or the batch rolls back | Staged `UPDATE`s with an optimistic conflict check | T777 |
| 5 | Pickers for booleans, enums and `CHECK (col IN (…))`; generated columns read-only; `=expr` cells the server computes, `\=` for literal text | Free text only | T777 |
| 6 | Search rows by field and operator (contains, equals, range, `IN`, `IS NULL`, raw SQL), run on the server, editable results | `/` filters the loaded rows only | T778 |
| 7 | Relations per table: incoming foreign keys and dependent views | Outgoing foreign keys only | T779 |
| 8 | Completion that resolves table aliases | `table.column` and `JOIN … ON` only | T780 |
| 9 | Environments (local, development, testing, staging, production) with a color; on production every write shows its SQL and asks; `DROP` and `TRUNCATE` always ask | Read-only by default; one confirmation for all writes | T781 |
| 10 | Paste a connection string (URL or libpq `key=value`) to fill the form | Vix builds URLs but cannot parse one | T781 |
| 11 | Copy for AI: the table's structure, or the query and its column types, with the selected rows, as Markdown | Nothing | T782 |
| 12 | Export whole tables from one consistent snapshot, streamed; SQL `INSERT`s in foreign-key order with sequences reset | Exports the rows already loaded | T784 |
| 13 | Schema compare between two connections, with migration SQL in an order that runs; destructive changes start unchecked, risky ones are flagged; run in one transaction | Nothing | T785 |
| 14 | Open or drag a `.sql` file into a query tab; psql-only commands (`\copy`, `\connect`) flagged before anything runs | `vix-edit-sql` edits `.sql` files but does not run them | T786 |

## Needs a decision: databases over MCP

dbjoy lets Claude Code and other MCP clients use the connections the user
chooses, with the saved password and SSH tunnel, through four read-only
tools: `list_connections`, `list_tables`, `describe_table` and `run_query`.
Its safeguards: every connection starts unshared; only `SELECT`, `WITH`,
`EXPLAIN`, `SHOW` and `VALUES` are accepted; each query runs in a read-only
transaction with a 30-second timeout and a row cap; and every request is
logged where the user can see it.

`vix --mcp` today offers only SQLite files inside the workspace, and its
spec says why: the server "never reads the OS keyring" and "nothing in this
mode opens a socket". Sharing a PostgreSQL or MySQL connection needs both.
It would be a useful feature, but it relaxes two written safety rules, so it
is proposed as T783 for the maintainer to accept or reject, with the design
in [`spec/mcp-server`](../mcp-server/index.md#proposed-shared-database-connections-t783).

## Not taken

- **Copy a database.** Drops every schema in the target and replays
  `pg_dump` into `psql`. It needs bundled PostgreSQL binaries, which Vix
  deliberately avoids, and it has the largest blast radius of anything in
  dbjoy. Schema compare (T785) covers the safer half.
- **An assistant that runs queries and reads the results.** dbjoy's
  assistant sees rows. Vix's assistant is shown the schema only, and that is
  a stated principle in `crates/vix-db/spec`. Copy for AI (T782) lets a user
  share rows on purpose, one copy at a time. An opt-in, per-connection mode
  that sends results goes to the Ideas backlog, not to tasks.
- **Per-window connections, a starred rail and connection icons.** These
  belong to a multi-window GUI. Vix's workbench is one overlay.
- **Its own provider keys.** Vix's assistant is whatever `ai_command` names;
  it needs no API-key handling.
