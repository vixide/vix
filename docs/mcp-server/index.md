# MCP server

`vix --mcp` turns Vix into a [Model Context Protocol](https://modelcontextprotocol.io)
server, so an AI agent (Claude Code, Zed, or any MCP client) can read a
workspace the way Vix sees it. It speaks MCP over standard input and output
and does not open the editor.

It is **read-only**: no tool writes a file, a setting, a session or a
database.

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

The open buffers are also listed as `file://` resources an agent can read.

## Limits in this version

- **No live editor.** `vix --mcp` is a separate process. It sees the files
  saved on disk and the list of files your last Vix session had open, not
  unsaved edits in a running Vix.
- **No language server.** Symbols and outlines come from Vix's own offline
  scan, and diagnostics are limited to merge conflicts. Language-server
  diagnostics would need a running server; every diagnostics result says
  `"language_server": "not_running"`.
- **SQLite only.** Database files inside the workspace are opened
  read-only. Saved PostgreSQL and MySQL connections are never opened,
  because they need passwords.

## Safety

- Every path must stay inside the workspace root, after following symbolic
  links.
- Files that may hold secrets are refused: `.env` files, private keys and
  certificates, `.netrc`, `.pgpass`, `credentials*`, and anything in
  `.git/` or `.ssh/`. Vix's keyring, settings and API keys are never read.
- Files over 2 MiB and binary files are refused.
- Nothing is written to disk and nothing uses the network.

The full protocol, including error codes, is in
[`spec/mcp-server`](../../spec/mcp-server/index.md).
