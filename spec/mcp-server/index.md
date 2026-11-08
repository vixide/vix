# MCP server mode

**Status:** specified and implemented 2026-10-29 (T731 in
[`tasks.md`](../../tasks.md)); shared database connections added
2026-11-07 (T783). Protocol core:
[`crates/vix-mcp-server`](../../crates/vix-mcp-server/spec/index.md). User
guide: [`docs/mcp-server`](../../docs/mcp-server/index.md). Related: the
Agent Client Protocol client (T635), whose preview-and-accept flow any future
write tool must reuse.

## Goal

`vix --mcp` serves the [Model Context Protocol](https://modelcontextprotocol.io)
(MCP) over standard input and output, so an AI agent (Claude Code, Zed, any
MCP client) can read *Vix's view* of a workspace: the files Vix has open
there, their document symbols and outline, offline diagnostics, the
schema of SQLite databases in the workspace, and — read-only — the saved
DB workbench connections the user chose to share
([below](#shared-database-connections-t783)).

**Read-only in v1.** No tool writes a file, a setting, a session, or a
database; the one file the server writes is its own log of DB tool calls
(see [Safety](#safety)). A later write tool must go through the same
preview and accept flow as T635; it is out of scope here.

## Headless, not attached

There is no running TUI to attach to. `vix --mcp` is its own process: it
does not start the editor, open a terminal, or talk to another Vix. It loads
what it needs itself, from disk, on each request:

| Data | Source in v1 |
| --- | --- |
| Workspace root | The first positional argument (`vix --mcp DIR`; it must be a directory, or the server exits with an error on stderr), else the current directory, canonicalized once at startup. |
| Open buffers | The files Vix last had open in this workspace: the persisted [session](../../crates/vix-session/spec/index.md) entry whose key is the canonical root, filtered to files that still exist inside the root. Unsaved edits in a running Vix are **not** visible (they live only in that process). |
| Document symbols | `vix_palette::symbols`, the same offline declaration scan the Outline panel and Go to Symbol use. No language server. |
| Outline | Markdown headings (`vix-markdown-preview`'s table of contents, mapped back to source lines), Org headlines (`vix_org::headlines`), and for any other file the document symbols as a flat, level-1 outline. |
| Diagnostics | **Offline checks only:** unresolved merge-conflict blocks (`vix_conflict_tool::find_all`). Language-server diagnostics need a live server; v1 does not spawn one, and every diagnostics result says so (`"language_server": "not_running"`). |
| DB schema | SQLite database files found inside the workspace (named `*.db`, `*.db3`, `*.sqlite` or `*.sqlite3` *and* starting with the 16-byte `SQLite format 3` header, so a stray `.db` that is not SQLite is skipped), opened **read-only** (`mode=ro` plus `PRAGMA query_only = ON`) through `vix-db`'s own session and catalog queries. No credentials are involved. Needs the `sql` Cargo capability; without it the tool reports the capability as missing. |
| Shared DB connections | The user's own settings file (`config.toml` in the config directory), read on each request and **never created or written**: `[[db_connections]]`, `[db_environments]` and `[db_mcp_shared]`. Only connections whose `[db_mcp_shared]` entry is `true` are used. A project's `.vix/settings.toml` layer is not read, so a repository cannot share a connection or supply a `password_command`. Needs the `sql` capability. |

## Transport and framing

- JSON-RPC 2.0, one message per line on stdin and stdout, UTF-8, `\n`
  terminated. Messages never contain an embedded newline (they are
  serialized compactly).
- stdout carries protocol messages only. Nothing else is printed there;
  a fatal startup error goes to stderr and the process exits non-zero.
- The server stops when stdin closes (end of file).
- Blank lines are ignored. A JSON-RPC batch (a JSON array) is rejected with
  `-32600`; MCP 2025-06-18 removed batching.

## Lifecycle

- **Protocol version:** `2025-06-18`. The server also accepts `2025-03-26`
  and `2024-11-05`; when a client asks for one of those it is echoed back,
  otherwise the server answers with `2025-06-18` and the client decides.
- `initialize` → `{protocolVersion, capabilities: {tools: {listChanged:
  false}, resources: {listChanged: false, subscribe: false}}, serverInfo:
  {name: "vix", version}, instructions}`.
- `notifications/initialized` and every other notification get no reply.
- `ping` → `{}` at any time.
- Any other request before `initialize` → error `-32002` (server not
  initialized).

## Methods

| Method | Result |
| --- | --- |
| `tools/list` | The ten tools below, each with an `inputSchema` and `annotations: {readOnlyHint: true, openWorldHint: false}`. |
| `tools/call` | `{content: [{type: "text", text: <the JSON result>}], structuredContent: <the same JSON>, isError: false}`. A tool-level failure (unknown path, path outside the root, file too large) is `isError: true` with a text explanation, per MCP, not a JSON-RPC error. |
| `resources/list` | One `file://` resource per open buffer (`uri`, `name` = workspace-relative path, `mimeType: "text/plain"`). |
| `resources/read` | `{contents: [{uri, mimeType, text}]}` for a `file://` URI that resolves inside the root and passes the secret filter; otherwise error `-32602`. |

### Tools

| Tool | Arguments | Returns |
| --- | --- | --- |
| `list_open_buffers` | — | `{root, buffers: [path]}` (workspace-relative). |
| `read_buffer` | `path` | `{path, text}`. |
| `document_symbols` | `path` | `{path, symbols: [{kind, name, line}]}` (1-based lines). |
| `outline` | `path` | `{path, format: "markdown" \| "org" \| "symbols", entries: [{level, title, line}]}`. |
| `diagnostics` | `path` (optional; all open buffers when omitted) | `{language_server: "not_running", diagnostics: [{path, line, end_line, severity: "error", source: "vix-conflict", message}]}`. |
| `db_schema` | — | `{databases: [{path, tables: [{name, kind, columns: [{name, type}]}]} \| {path, error}]}`. |
| `db_connections` | — | `{connections: [{name, kind, environment, database}]}`: the shared connections only (see below). |
| `db_tables` | `connection`, `schema` (optional) | `{connection, schema, schemas: [name], objects: [{name, kind}], truncated}`. |
| `db_describe` | `connection`, `table`, `schema` (optional) | `{connection, schema, table, columns, primary_key, indexes, foreign_keys, constraints}`, each a result set. |
| `db_query` | `connection`, `sql` | `{connection, columns: [name], rows: [[text]], row_count, row_limit: 100, truncated}`. |

A *result set* is `{columns: [name], rows: [[text]], truncated}`: every cell
is text and SQL `NULL` is the empty string, as in the workbench grid; the
column names come from the first row, so an empty result has none.

## Errors

| Code | Meaning |
| --- | --- |
| `-32700` | Parse error (the line is not JSON); `id` is `null`. |
| `-32600` | Invalid request (not a JSON-RPC 2.0 object, or a batch). |
| `-32601` | Method not found. |
| `-32602` | Invalid params (missing tool name, unknown tool, bad resource URI). |
| `-32002` | Server not initialized. |

Error and tool-failure messages are protocol text for an agent, not UI, and
stay English like the tool descriptions (the same reason JSON keys are not
translated). The one human-facing string, the stderr startup error, goes
through `t!`.

## Safety

- **Inside the root only.** Every path argument and `file://` URI is
  lexically normalized against the root (no `..` above it), then
  canonicalized, and the canonical path must still start with the canonical
  root, so a symbolic link cannot lead outside. Anything else is refused.
- **Secrets only for shared connections.** The server reads a database
  password — by running the connection's `password_command`, then from the
  OS keyring (service `vix-db`, account = the connection name) — **only**
  when an MCP client calls `db_tables`, `db_describe` or `db_query` on a
  connection the user marked shared, and only that connection's password.
  The password stays in memory for that one call and is never returned,
  logged or shown in an error. Nothing else is read from the keyring, and
  the server never reads API keys or AI provider configuration. It refuses
  to read files whose names mark them as secrets: `.env` and `.env.*`, `*.pem`, `*.key`, `*.p12`,
  `*.pfx`, `id_rsa*`, `id_ecdsa*`, `id_ed25519*`, `.netrc`, `.pgpass`,
  `credentials*`, and anything under `.git/` or `.ssh/`. The same filter
  applies to `resources/read`, `read_buffer`, symbols, outline and
  diagnostics.
- **Bounded.** Files over 2 MiB are refused; the SQLite search walks at most
  8 directory levels, skips hidden directories, `target` and `node_modules`,
  and stops after 16 databases.
- **Read-only.** The server writes one thing: the DB tool log,
  `mcp-db.log` next to the session file (the config directory), one JSON
  line per DB tool call. It is never written for any other tool, the config
  directory is never created for it, and it is owner-only on Unix. It
  reads the session file and `config.toml` only if they already exist
  (it never creates or writes either), and opens every database read-only.
- **Network only for shared connections.** The server opens a socket only
  to the database server of a shared `PostgreSQL` or `MySQL` connection,
  during a `db_tables`, `db_describe` or `db_query` call on it, and — when
  that connection has an SSH tunnel — starts `ssh` for the tunnel, which
  connects to the SSH host. The tunnel runs with `BatchMode=yes`, so `ssh`
  never prompts; it ends with the call. Nothing else opens a socket, and
  there is no listener: stdio stays the only transport. (A
  `password_command` is the user's own command and does whatever it does,
  such as `op read` reaching a vault.)
- **No prompts.** Vix asks nobody anything in this mode. A shared
  connection that would need a typed password, or an SSH key passphrase,
  fails with a tool error instead. A `password_command` runs with no
  standard input (and its standard error discarded); a command that opens
  a dialog of its own, such as a GPG pinentry, is the user's choice.

## Shared database connections (T783)

A saved DB workbench connection can be shared with MCP clients, read-only.
The design follows [dbjoy](https://github.com/agustind/dbjoy)'s MCP server
([`spec/comparisons/dbjoy.md`](../comparisons/dbjoy.md)), kept to stdio.
The engine side is `vix_db::mcp_share`
([`crates/vix-db/spec`](../../crates/vix-db/spec/index.md)); the protocol
side is the four `db_*` connection tools above.

- **Opt-in per connection.** The workbench's connection form has a
  **Share with MCP** row, off by default. It is saved by connection name in
  a `[db_mcp_shared]` settings table (`app = true`), not on the published
  `Connection` struct, which cannot gain a field without a semver break —
  the same reason as T781's `[db_environments]`. Renaming a connection
  carries the flag; deleting one drops it. A connection that is not shared
  does not exist for MCP clients: `db_connections` leaves it out, and the
  other tools answer it exactly like an unknown name.
- **Names, not addresses.** `db_connections` gives `name`, `kind`
  (`sqlite`, `postgres`, `mysql`), `environment` (T781) and `database` (for
  SQLite, the file's name without its folder) — never the host, port, user,
  password, password command or SSH settings.
- **No prompts.** Credentials come from the connection's `password_command`,
  then the OS keyring (the workbench's waterfall, `vix_db::secret`); a
  server connection with neither fails with "connection NAME needs a
  password, and MCP clients cannot be asked for one …". An SSH tunnel is
  opened with `BatchMode=yes` (`vix_db::tunnel::open_batch`), so it needs an
  agent key or an identity file without a passphrase and a known host key.
- **Reads only.** `db_query` accepts exactly one statement (a piece that is
  only a comment does not count) whose first word, after comments and
  opening parentheses, is `SELECT`, `WITH`, `EXPLAIN`, `SHOW` or `VALUES`.
  It refuses `ANALYZE`/`ANALYSE` anywhere (so `EXPLAIN ANALYZE`), any word
  `vix_db::editor::is_write_statement` flags (which catches
  `WITH … DELETE` and `FOR UPDATE`), and `INTO` (`SELECT … INTO`,
  `INTO OUTFILE`), `LOCK`, `SHARE`, `COMMIT`, `ROLLBACK` and `SAVEPOINT`. The
  scans are the workbench's whole-word scans outside strings and comments,
  so they are conservative (`replace(…)` is refused too). A refused
  statement opens nothing. The check is `vix_db::mcp_share::check_query`,
  pure and unit-tested.
- **A read-only transaction with a timeout, rolled back.** Every call opens
  its own connection, runs these first, then its statements, then
  `ROLLBACK`, and closes:
  - PostgreSQL: `SET default_transaction_read_only = on`,
    `START TRANSACTION READ ONLY`, `SET LOCAL statement_timeout = '30s'`;
  - MySQL: `SET SESSION TRANSACTION READ ONLY`,
    `START TRANSACTION READ ONLY`, `SET SESSION MAX_EXECUTION_TIME = 30000`
    (MySQL applies it to `SELECT` statements);
  - SQLite: the file opened with `mode=ro` (never created), then
    `PRAGMA query_only = ON` and `BEGIN`. SQLite has no statement timeout,
    and sqlx's `Any` driver cannot interrupt a running statement, so the
    30 seconds are a client-side deadline: the call fails with a timeout
    error and the connection is dropped, while SQLite may finish the
    abandoned read in a background thread until it completes.

  The client also gives up on a server engine 5 seconds after the server's
  own timeout, and on opening a connection after 20 seconds. The read-only
  transaction makes the engine refuse a write hidden in a function, and
  the rollback means nothing persists either way. `db_tables` and
  `db_describe` run the workbench's own catalog queries
  (`catalog::objects_sql`, `detail_sql`, `primary_key_sql`) the same way.
- **Capped.** `db_query` returns at most **100** rows; `truncated: true`
  says rows were cut off. Catalog reads stop at 10,000 rows, flagged the
  same way.
- **Seen.** Every call to one of the four tools is appended to the DB tool
  log before anything is opened: one JSON line,
  `{"time":"2026-11-07T12:00:00Z","tool":"db_query","connection":"app","sql":"…"}`,
  with `schema` and `table` when given. SQL longer than 8,192 characters is
  cut and ends with `…`. A call on a shared connection is refused if its
  line cannot be written ("cannot write the DB tool log, so the call was
  not run"), so no call goes unrecorded; a call on an unknown or unshared
  name, and `db_connections`, are logged when the log can be written. When
  the log reaches 1 MiB it is renamed `mcp-db.log.1` (replacing an older
  one) and a new one starts, so the two stay near 2 MiB together. The
  workbench's query log does not show these calls in this version; the
  file is the record.
- **stdio stays the only transport.** Unlike dbjoy, there is no local HTTP
  listener or token, because `vix --mcp` is started by the client.

## Not in v1

- Live language-server diagnostics and symbols (would need to spawn and
  manage servers without the editor running).
- Seeing unsaved buffers of a running Vix (would need an attach channel).
- `PostgreSQL`/`MySQL` databases that are not saved workbench connections
  marked shared: the server never guesses credentials, reads `.pgpass` or
  environment variables, or takes a connection string from the client
  ([shared connections](#shared-database-connections-t783) are the only
  way in).
- Showing MCP DB tool calls in the workbench's query log (the log file is
  the record).
- Any write tool, prompts, subscriptions, or `listChanged` notifications.
- The HTTP transport (stdio only).
