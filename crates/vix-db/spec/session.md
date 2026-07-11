# DB sessions — design notes and remaining milestones

Status: **core implemented.** The workbench uses **embedded [sqlx] drivers**
(`db::session`) for a persistent per-connection session: typed result sets, real
driver errors, and no dependency on client tools being installed.

[sqlx]: https://crates.io/crates/sqlx

## What is implemented

- `db::session::Session` — one sqlx `Any` connection per workbench
  connection, owned by a dedicated worker thread running a current-thread
  tokio runtime. `Session::run(sql)` exchanges one request for one reply over
  channels, so the TUI stays synchronous. Dropping the `Session` closes the
  channel and the worker
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
- **Transaction UI (M2)** — a client-side `TxState { None, Open, Aborted }` is
  tracked from executed keywords and errors (`note_tx`), badged in the editor
  pane title (`TX` / `TX!`), and surfaced as **DB → Begin / Commit / Rollback**
  menu items. Inside an open transaction the write-confirmation gate is relaxed
  (the change is provisional and `ROLLBACK` can undo it). See
  `tests/db_smoke.rs::transaction_state_badge_and_relaxed_confirm`.

## Milestones

The M2–M7 roadmap is complete; each milestone is summarized below.

**Async queries + cancellation (M3) — done.** `Session` gained non-blocking
`send`/`poll` (alongside the still-synchronous `run` used for fast internal
catalog/commit queries) and a `restart` that reconnects on a fresh connection.
User statements (`F5`) and `EXPLAIN` (`F6`/`F7`) now run asynchronously: the
Browser holds a `pending_query`, the host drains it each tick via
`poll_db_query` (like `poll_http`), and the editor title shows an elapsed
indicator while it runs. The workbench is busy until the reply lands (only
`Ctrl+C` responds). `Ctrl+C` cancels by abandoning the worker — its result is
discarded when it finally arrives — and reconnecting so the UI is usable
again; this is universal across engines (no `pg_backend_pid` juggling) at the
cost of losing transaction state and not truly interrupting a server-side
query. See `tests/db_smoke.rs::async_query_runs_off_the_event_loop_and_cancels`.

**Streaming results (M4) — done.** The worker fetches with `fetch` (a row
stream) instead of `fetch_all` and forwards the result as `Chunk`s over the
reply channel: a `Head` (column names), then `Rows` batches of up to `BATCH`
(512), then `Done(truncated)`. `Session::run` drains chunks into a full `Table`
for internal callers; the async path ([`poll_query`]) appends each batch to the
grid (`Grid::append_rows`) so rows show up progressively, capped at `MAX_ROWS`
(20,000) with a "truncated" note (`msg.db_rows_truncated`). Cancellation
(abandon + reconnect) discards any un-drained chunks. See
`tests/db_smoke.rs::large_result_streams_in_batches`. A future refinement is
true server-side pagination (Postgres `DECLARE … CURSOR`; `FETCH n` on
scroll-to-bottom) rather than a fixed cap.

**Staged cell edits (M5) — done.** On an editable grid (a table preview whose
primary key `primary_key_sql` resolves; no PK ⇒ read-only), `i` edits the
selected cell into a staged map `edits: (row, col) → (original, new)`, shown
bold-yellow in the grid. `W` commits: each edit becomes an `UPDATE … WHERE pk =
…`, re-checked against the loaded value for optimistic conflict detection
(`msg.db_edit_conflict`), all wrapped in one `BEGIN`/`COMMIT` (rollback on any
error), then the preview refreshes. See
`tests/db_smoke.rs::staged_cell_edits_commit_in_a_transaction`.

**SSH tunnels (M6) — done.** `Connection` gained
`ssh_host/ssh_user/ssh_port/ssh_identity`. When `ssh_host` is set (server
engines only), `finish_connect` calls `db::tunnel::open`: it reserves a free
local port (bind `127.0.0.1:0`, drop the listener), spawns
`ssh -N -o ExitOnForwardFailure=yes -L <local>:<db_host>:<db_port> [user@]host`
(adding `-p`/`-i` when set), waits for the local port to accept a connection,
and the URL is built with `connect::url_via_local` against `127.0.0.1:<local>`.
The `Tunnel` owns the `ssh` child and kills it on drop, so its lifetime is tied
to the connection (disconnect tears it down). The `ssh` argument construction
is pure and unit-tested; only `open` spawns a process. See
`tests/db_smoke.rs`/`db::tunnel` tests.

**Credential waterfall (M7) — done.** A server connection resolves its
password before prompting (`db::secret::resolve`, called from `start_connect`):
first the per-connection `password_command` (any command that prints the
secret — `pass show db/prod`, `op read …`), then the OS keyring via its CLI
(`security find-generic-password` on macOS, `secret-tool lookup` on Linux);
only on a miss does it fall back to the prompt. The connection form adds a
**Password cmd** field and a **Save to keyring** toggle — when set, a prompted
password is stored back (`security add-generic-password` / `secret-tool store`)
on a successful connect, so later connects skip the prompt. The command
*construction* (`keyring_lookup` / `keyring_store`) is pure and unit-tested;
only `run` touches the process table. sqlx does not read `~/.pgpass` /
`~/.my.cnf`, so those are deliberately not part of the waterfall.
