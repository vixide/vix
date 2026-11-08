# MCP server

`vix --mcp` turns Vix into a [Model Context Protocol](https://modelcontextprotocol.io)
server, so an AI agent (Claude Code, Zed, or any MCP client) can read a
workspace the way Vix sees it. It speaks MCP over standard input and output
and does not open the editor.

It is **read-only**: no tool writes a file, a setting, a session or a
database. The one file it writes is a log of database tool calls (see
[Shared database connections](#shared-database-connections)).

## Set it up

Point your MCP client at the `vix` command with the `--mcp` flag. The
workspace is the directory you pass, or the client's current directory.

Claude Code:

```sh
claude mcp add vix -- vix --mcp /path/to/project
```

Any client that takes a JSON server list:

```json
{
  "mcpServers": {
    "vix": { "command": "vix", "args": ["--mcp", "/path/to/project"] }
  }
}
```

## What an agent can read

| Tool | What it returns |
| --- | --- |
| `list_open_buffers` | The files Vix last had open in this workspace (from its saved session). |
| `read_buffer` | One file's saved text. |
| `document_symbols` | Functions, types, modules and other declarations, with line numbers — the same offline scan as Vix's Outline panel. |
| `outline` | Markdown headings, Org headlines, or a code file's symbols, each with its level and line. |
| `diagnostics` | Unresolved merge-conflict blocks in one file or every open buffer. |
| `db_schema` | Tables, views and columns of the SQLite databases in the workspace. |
| `db_connections` | The saved database connections you shared: name, kind, environment and database. |
| `db_tables` | Tables, views and routines in one schema of a shared connection. |
| `db_describe` | One table's columns, primary key, indexes, foreign keys and constraints. |
| `db_query` | One read-only statement on a shared connection, at most 100 rows. |

The open buffers are also listed as `file://` resources an agent can read.

## Shared database connections

In the DB workbench (**DB → Connections…**), edit a saved connection and
turn on **Share with MCP**. Only connections you share exist for an agent;
the rest are invisible. An agent sees a shared connection's name, engine,
environment and database name, never its host, user or password.

- **Read-only.** `db_query` runs one `SELECT`, `WITH`, `EXPLAIN` (not
  `EXPLAIN ANALYZE`), `SHOW` or `VALUES` statement. Anything that could
  write is refused before a connection is opened. The statement runs inside
  a read-only transaction that is rolled back, with a 30-second timeout.
- **No prompts.** The password comes from the connection's password command
  or the OS keyring (connect once in the workbench with **Save to keyring**
  on). A connection that needs a typed password fails with an error. An SSH
  tunnel must work without a passphrase or a host-key question (an agent
  key, and a host already in `known_hosts`).
- **Logged.** Every call is a JSON line in `mcp-db.log`, next to Vix's
  `session.toml` and `config.toml` (`~/.config/vix/mcp-db.log` on Linux,
  `~/Library/Application Support/rs.vix/mcp-db.log` on macOS): the time,
  the tool, the connection and the SQL. Only you can read it. At 1 MiB it is renamed `mcp-db.log.1` and a new log starts.
- Only your own settings file is read; a project's `.vix/settings.toml`
  cannot share a connection.

## Limits in this version

- **No live editor.** `vix --mcp` is a separate process. It sees the files
  saved on disk and the list of files your last Vix session had open, not
  unsaved edits in a running Vix.
- **No language server.** Symbols and outlines come from Vix's own offline
  scan, and diagnostics are limited to merge conflicts. Language-server
  diagnostics would need a running server; every diagnostics result says
  `"language_server": "not_running"`.
- **Databases.** SQLite files inside the workspace are opened read-only.
  PostgreSQL and MySQL are reached only through saved connections you
  share. Results are text (a `NULL` shows as empty), and an empty result has
  no column names.
- **SQLite timeout.** SQLite has no server-side timeout, so a 30-second
  read on a shared SQLite connection fails the call, but SQLite may finish
  the abandoned read in the background.

## Safety

- Every path must stay inside the workspace root, after following symbolic
  links.
- Files that may hold secrets are refused: `.env` files, private keys and
  certificates, `.netrc`, `.pgpass`, `credentials*`, and anything in
  `.git/` or `.ssh/`. API keys are never read; the keyring and password
  commands are used only for a connection you shared, when an agent uses it.
- Files over 2 MiB and binary files are refused.
- Nothing is written to disk except `mcp-db.log`, and nothing uses the
  network except a shared connection's database server (and its SSH tunnel,
  if it has one) during a call on it.

The full protocol, including error codes, is in
[`spec/mcp-server`](../../spec/mcp-server/index.md).
