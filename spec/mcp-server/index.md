# MCP server mode

**Status:** specified and implemented 2026-10-29 (T731 in
[`tasks.md`](../../tasks.md)). Protocol core:
[`crates/vix-mcp-server`](../../crates/vix-mcp-server/spec/index.md). User
guide: [`docs/mcp-server`](../../docs/mcp-server/index.md). Related: the
Agent Client Protocol client (T635), whose preview-and-accept flow any future
write tool must reuse.

## Goal

`vix --mcp` serves the [Model Context Protocol](https://modelcontextprotocol.io)
(MCP) over standard input and output, so an AI agent (Claude Code, Zed, any
MCP client) can read *Vix's view* of a workspace: the files Vix has open
there, their document symbols and outline, offline diagnostics, and the
schema of SQLite databases in the workspace.

**Read-only in v1.** No tool writes a file, a setting, a session, or a
database. A later write tool must go through the same preview and accept
flow as T635; it is out of scope here.

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
| DB schema | SQLite database files found inside the workspace (named `*.db`, `*.db3`, `*.sqlite` or `*.sqlite3` *and* starting with the 16-byte `SQLite format 3` header, so a stray `.db` that is not SQLite is skipped), opened **read-only** (`mode=ro` plus `PRAGMA query_only = ON`) through `vix-db`'s own session and catalog queries. Saved server connections (`PostgreSQL`, `MySQL`) are **not** opened: they need a password from the OS keyring, and the server never reads secrets. Needs the `sql` Cargo capability; without it the tool reports the capability as missing. |

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
| `tools/list` | The six tools below, each with an `inputSchema` and `annotations: {readOnlyHint: true, openWorldHint: false}`. |
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
- **No secrets.** The server never reads the OS keyring, settings passwords,
  API keys, or AI provider configuration. It refuses to read files whose
  names mark them as secrets: `.env` and `.env.*`, `*.pem`, `*.key`, `*.p12`,
  `*.pfx`, `id_rsa*`, `id_ecdsa*`, `id_ed25519*`, `.netrc`, `.pgpass`,
  `credentials*`, and anything under `.git/` or `.ssh/`. The same filter
  applies to `resources/read`, `read_buffer`, symbols, outline and
  diagnostics.
- **Bounded.** Files over 2 MiB are refused; the SQLite search walks at most
  8 directory levels, skips hidden directories, `target` and `node_modules`,
  and stops after 16 databases.
- **Read-only.** The server writes nothing: it reads the session file only
  if it already exists (it never creates one), does not load or create
  `config.toml`, and opens databases read-only.
- **No network.** Nothing in this mode opens a socket.

## Proposed: shared database connections (T783)

**Proposed, not accepted.** This needs the maintainer's decision, because it
relaxes two rules in [Safety](#safety): the server would read credentials
from the OS keyring or a `password_command`, and it would open sockets to
database servers. It comes from
[dbjoy](https://github.com/agustind/dbjoy)'s MCP server; see
[`spec/comparisons/dbjoy.md`](../comparisons/dbjoy.md).

If accepted, a saved DB workbench connection gains a `share_with_mcp` flag,
off by default, and `vix --mcp` adds four tools for the shared connections
only:

| Tool | Arguments | Returns |
| --- | --- | --- |
| `db_connections` | — | `{connections: [{name, kind, environment, database}]}` (no host, user or password). |
| `db_tables` | `connection`, `schema` (optional) | Tables, views and routines in the schema, and the other schema names. |
| `db_describe` | `connection`, `table` | Columns, keys, indexes, constraints and foreign keys. |
| `db_query` | `connection`, `sql` | The first 100 rows as text, with the column names. |

Safeguards:

- **Opt-in per connection.** A connection that is not shared does not exist
  for MCP clients.
- **No prompts.** Credentials come from the keyring or the connection's
  `password_command` only. A connection that would need a typed password
  fails with a message, because nobody is there to type it.
- **Reads only.** `db_query` accepts one statement starting with `SELECT`,
  `WITH`, `EXPLAIN` (never `ANALYZE`), `SHOW` or `VALUES`, and rejects
  anything `is_write_statement` flags. It runs inside a read-only
  transaction with a 30-second statement timeout (PostgreSQL
  `SET LOCAL statement_timeout`, MySQL `MAX_EXECUTION_TIME`), then rolls
  back, which also stops writes hidden in functions.
- **Seen.** Every call is appended to a log file next to the session file:
  time, tool, connection and SQL. The workbench's query log shows it.
- **stdio stays the only transport.** Unlike dbjoy, there is no local HTTP
  listener or token, because `vix --mcp` is started by the client.

## Not in v1

- Live language-server diagnostics and symbols (would need to spawn and
  manage servers without the editor running).
- Seeing unsaved buffers of a running Vix (would need an attach channel).
- `PostgreSQL`/`MySQL` schemas (need secrets; proposed as T783 above).
- Any write tool, prompts, subscriptions, or `listChanged` notifications.
- The HTTP transport (stdio only).
