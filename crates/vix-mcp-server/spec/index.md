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
  `document_symbols` (`vix_textops::symbols`), `outline` (Markdown via the
  private `markdown::headings`, Org via `vix_org::headlines`, otherwise symbols),
  `diagnostics` (`vix_conflict_tool::find_all`; no language server),
  `db_schema`, and the DB connection tools `db_connections`, `db_tables`,
  `db_describe` and `db_query`, which only check their arguments and shape
  the host's answer as JSON (`connections_json`, `tables_json`,
  `describe_json`, `query_json`, `rows_json`, pure and unit-tested).
- `workspace` is the boundary: `resolve` (lexical normalization, then
  canonicalization, then a prefix check against the canonical root, so
  neither `..` nor a symbolic link escapes), `is_secret` (names compared in
  ASCII lower case, `.git` and `.ssh` directory names too, since a
  case-insensitive file system opens `.GIT/config` as `.git/config`; T832), `read_text`
  (2 MiB cap, no binaries, UTF-8 only) and `find_sqlite` (bounded walk,
  extension plus SQLite header, no symbolic links followed).
- `file_uri` / `uri_path` convert between absolute paths and `file://` URIs
  for `resources/list` and `resources/read`.

## Markdown headings and the crate graph (T860)

The `outline` tool's Markdown headings come from `markdown::headings`, one
pass over `pulldown-cmark`'s events: every ATX and setext heading (not a
`#` line inside a code block, nor `#tag` with no space), its level, its
title and the 1-based source line it starts on. The title is the heading's
text, code spans and link and image text, a line break inside it becoming one
space, trimmed; markup and link URLs are dropped.

Until T860 the tool borrowed `vix_markdown_preview::render_full`'s table of
contents and mapped each preview row back to a source line. That made the
headless server depend on a preview crate and, through it, on
`vix-outline-panel`, for one heading list; it also gave titles the preview's
display text (a link as `text (url)`, a quoted heading with its `│ `
rail, a two-line setext heading cut at the break). The server now depends on
`pulldown-cmark` directly, which the preview already pulled in, and
`tui/tests/crate_layering.rs` (rule 5) keeps `vix-mcp-server` from reaching
any `*-panel`, `*-picker`, `*-preview` or `*-view` crate again. Nothing
moved out of `vix-markdown-preview`: its public API is unchanged, and its
table of contents still holds preview rows for the TOC overlay.

The symbol scan behind `document_symbols` and the code `outline` lived in
`vix-palette` until T861, the server's last edge to a UI crate. It moved to
`vix-textops` (`vix_textops::symbol`), which only needs `regex`;
`vix-palette` keeps its published `Symbol` and `symbols` as thin wrappers. Rule 5 also lists `vix-palette`
among the crates the server must not reach (`HEADLESS_FORBIDDEN`).

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

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). Line numbers and the walk depth are plain additions under `RCA-EXC-2` expectations that state their proofs; the tool schema and the tool-name lookup use `get` and a proven object key. The Markdown heading scan counts lines with `checked_add`; **Declared (MCP-F1):** a line count that does not fit (impossible for text in memory) keeps the line it had. A file with no extension has the empty extension (`RCA-IN-3`: absent only).
