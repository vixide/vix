# MCP server core

Read-only Model Context Protocol server core: JSON-RPC over stdio exposing a
workspace's open buffers, symbols, outline, offline diagnostics and SQLite
schema, and the saved DB connections the user shared (T783). It is the
protocol core of `vix --mcp` (T731): newline-delimited JSON-RPC 2.0 on
stdio. The behavior — methods, tools, error codes, safety
rules and what v1 leaves out — is specified in the cross-cutting
[`spec/mcp-server`](../../../spec/mcp-server/index.md) (see also the
[Model Context Protocol](https://modelcontextprotocol.io)); this page covers
the crate's shape.

## Why a crate

It has real logic worth testing in isolation (message validation, version
negotiation, tool dispatch, the workspace boundary and the secret filter),
and it is host-agnostic: no `App`, no terminal, no `confy`, no `sqlx`. The
App shell (`tui/src/mcp.rs`) is its one consumer today; any other host (a
GUI, a test harness) can serve the same protocol by implementing `Host`.

## API

- `Server::new(root, host, version)` canonicalizes `root` (which must be a
  directory). `Server::handle_line` takes one input line and returns the
  reply line, or `None` for a notification or a blank line;
  `Server::handle` does the same on decoded JSON.
- `serve(server, input, output)` runs the line loop until `input` ends,
  flushing after every reply.
- `Host` is what only the App shell can supply:
  - `open_buffers(root)` — the files Vix last had open in this workspace
    (the server canonicalizes them and drops anything outside the root,
    missing, or secret);
  - `sqlite_schema(path)` — one SQLite file's tables and columns, read-only;
    `None` when the build has no database support;
  - `shared_connections()`, `db_tables(connection, schema)`,
    `db_describe(connection, schema, table)` and `db_query(connection, sql)`
    (T783) — the shared DB connections (`SharedConnection`) and their
    catalog and query results (`DbTables`, `DbDescription`, `Rows`). Each
    has a default that returns `None` (no database support), so a host that
    shares nothing implements only the first two. The host owns the safety
    rules: which connections are shared, the statement check, the log, and
    opening read-only without prompting (the App shell does it through
    `vix_db::mcp_share`).
- `tools` holds the ten tool definitions (`tools::list`) and their
  implementations (`tools::call`): `list_open_buffers`, `read_buffer`,
  `document_symbols` (`vix_palette::symbols`), `outline` (Markdown via
  `vix-markdown-preview`, Org via `vix_org::headlines`, otherwise symbols),
  `diagnostics` (`vix_conflict_tool::find_all`; no language server),
  `db_schema`, and the DB connection tools `db_connections`, `db_tables`,
  `db_describe` and `db_query`, which only check their arguments and shape
  the host's answer as JSON (`connections_json`, `tables_json`,
  `describe_json`, `query_json`, `rows_json`, pure and unit-tested).
- `workspace` is the boundary: `resolve` (lexical normalization, then
  canonicalization, then a prefix check against the canonical root, so
  neither `..` nor a symbolic link escapes), `is_secret`, `read_text`
  (2 MiB cap, no binaries, UTF-8 only) and `find_sqlite` (bounded walk,
  extension plus SQLite header, no symbolic links followed).
- `file_uri` / `uri_path` convert between absolute paths and `file://` URIs
  for `resources/list` and `resources/read`.

## Errors (T610)

`workspace::WorkspaceError` (returned by `resolve` and `read_text`) and
`tools::ToolError` (returned by `tools::call`):

| Type | Variant | Meaning |
| --- | --- | --- |
| `WorkspaceError` | `EmptyPath` | the requested path is empty |
| | `OutsideRoot { request }` | the path leaves the workspace root |
| | `NotFound { request }` | the path does not exist |
| | `NotRegular { request }` | the path is not a regular file |
| | `Secret { request }` | the path names a file that may hold secrets |
| | `TooLarge { path, size }` | the file is over `MAX_FILE_BYTES` |
| | `Io { path, source }` | the file could not be read |
| | `Binary { path }` | the file holds a NUL byte |
| | `NotUtf8 { path }` | the file is not valid UTF-8 |
| `ToolError` | `UnknownTool(name)` | no tool has this name |
| | `MissingPath` | a required `path` argument is absent |
| | `PathNotString` | an optional `path` argument is not a string |
| | `Workspace(WorkspaceError)` | the workspace boundary refused the path |
| | `MissingArgument(name)` | a required string argument (`connection`, `table`, `sql`) is absent |
| | `NotAString(name)` | an optional argument (`schema`) is not a string |
| | `NoSqlCapability` | a DB connection tool in a build without database support |
| | `Host(HostError)` | the host refused or failed (an unshared connection, a refused statement, a database error) |

`Host::sqlite_schema` and the DB connection methods report failures as `HostError`
(`Box<dyn Error + Send + Sync>`); only its `Display` text reaches the client.
Every type is `#[non_exhaustive]` with a hand-written `Display`/`Error` (no
`thiserror`). `Display` is the same text the earlier `String` errors carried,
so the `isError` tool results are unchanged.

## Tests

Unit tests run every method against a fake `Host` over a scratch
workspace: version negotiation, the not-initialized guard, notifications,
parse/batch/method/param errors, one-line replies, every tool, secret and
escape refusals, symbolic links pointing outside, resource round trips,
the `serve` loop, and the DB connection tools' JSON and in-band errors.
`tui/tests/integration/mcp.rs` drives the App shell's `McpHost` through
`Server::handle_line` with real `SQLite` files (T783): an unshared
connection is invisible, writes and `WITH … DELETE` are refused, a
`SELECT` is capped at 100 rows, and every call reaches the log, which
rotates. `tui/tests/mcp_stdio.rs` spawns the real binary for an
`initialize` → `tools/list` → `tools/call` round trip and checks it wrote
nothing.
