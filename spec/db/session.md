# DB sessions — design notes and remaining milestones

Status: **core implemented** — but not as originally sketched. The first
version of this document proposed keeping the CLI bridge (`sqlite3` / `psql`
/ `mysql` child processes) and holding one client open per connection with
sentinel-framed stdout. That approach was superseded: the workbench now uses
**embedded [sqlx] drivers** (`db::session`), which give the same persistent
session with none of the framing fragility — typed result sets instead of
TSV parsing, real driver errors, and no dependency on client tools being
installed.

[sqlx]: https://crates.io/crates/sqlx

## What is implemented

- `db::session::Session` — one sqlx `Any` connection per workbench
  connection, owned by a dedicated worker thread running a current-thread
  tokio runtime. `Session::run(sql)` exchanges one request for one reply over
  channels (same blocking semantics the CLI bridge had; the TUI stays
  synchronous). Dropping the `Session` closes the channel and the worker
  closes the connection cleanly.
- Statements execute via `raw_sql` + `AssertSqlSafe` (the workbench runs
  exactly what the user typed — sqlx's dynamic-SQL opt-in, not an injection
  hazard). Cells decode through a widest-first cascade
  (i64 → f64 → String → bool → bytes); `NULL` renders empty.
- URLs come from `db::connect::url` (`sqlite:…?mode=rwc`,
  `postgres://user:pass@host:port/db`, `mysql://…`), with the password
  percent-encoded and living only in that in-memory string.
- **Transactions and savepoints work today** by executing `BEGIN` /
  `SAVEPOINT` / `COMMIT` / `ROLLBACK` from the query editor — state persists
  across statements (`tests/db_smoke.rs::transactions_span_statements_in_the_workbench`).

## Remaining milestones

**M2 — transaction UI.** The capability exists; the UI does not surface it.
Track a client-side `TxState { None, Open, Aborted }` from executed keywords
and error events; show a `TX` badge in the workbench title; add
DB → Transaction menu items; relax the write-confirmation gate inside an
explicit transaction (a rollback path exists).

**M3 — async queries + cancellation.** `Session::run` blocks the UI for the
statement's duration. Split it into `send`/`poll` (the app already pumps
`poll_http`/`poll_dap`-style receivers every tick), show a spinner + elapsed
time, and cancel with `Ctrl+C`: capture `pg_backend_pid()` /
`CONNECTION_ID()` at connect and cancel out-of-band on a second short-lived
connection; for SQLite, drop and respawn the session.

**M4 — streaming results.** Swap `fetch_all` for `fetch` and forward row
batches as they arrive; the grid appends per batch (`1,204 rows… loading`).
Postgres can add true pagination with native cursors
(`DECLARE vix_cur CURSOR FOR …; FETCH 500` on scroll-to-bottom).

**M5 — staged cell edits.** Needs M2. `i` edits a grid cell in place; edits
accumulate in `staged: Map<(pk, col), new>` (primary keys from the existing
`Detail::Columns` queries; rowid on SQLite). The commit dialog lists the
generated `UPDATE`s, runs them in one transaction, and re-`SELECT`s each row
first for conflict detection. No PK ⇒ read-only.

**M6 — SSH tunnels.** `Connection` gains optional
`ssh_host/ssh_user/ssh_port/ssh_identity`. Connect spawns
`ssh -N -L <local>:<host>:<port>` (agent/identity auth comes free), picks
`<local>` by binding port 0 and dropping the listener, waits for the tunnel,
then points the sqlx URL at `127.0.0.1:<local>`. The tunnel child's lifetime
is tied to the session's.

**M7 — credential waterfall.** Before prompting: a per-connection
`password_command` (e.g. `pass show db/prod`); the OS keyring via CLI
(`security find-generic-password -w`, `secret-tool lookup`); then today's
prompt, with an offer to store into the keyring on success. (The CLI-era
`~/.pgpass`/`~/.my.cnf` step no longer applies — sqlx does not read client
config files.)
