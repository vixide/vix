# Language Server Protocol (LSP)

Vix speaks the Language Server Protocol to bring semantic features — **diagnostics**,
**hover**, **go-to-definition**, and **completion** — to any language you have a
server for.

## As implemented in Vix

**Status:** Shipped. The protocol core lives in the internal `vix-lsp` crate
(JSON-RPC framing, message builders, response/diagnostic parsers, and char↔UTF-16
position maths — all pure and unit-tested). The host (`src/lsp.rs`) owns the IO:
it launches a server per language, reads its framed stdout on a background thread
(an `mpsc` channel, like the run-command feature), writes requests to stdin, runs
the `initialize`/`initialized` handshake, and keeps each open document in sync with
full-text `didChange`. `App::poll_lsp` drains server messages once per event-loop
iteration.

There is **no built-in server** — Vix launches only what you configure.

## Configuration

LSP is controlled by two settings (see `docs/configuration.md`):

- `lsp_enabled` (bool, default `true`) — master switch.
- `lsp_servers` (list) — one entry per server, matched to files by extension:

```toml
[[lsp_servers]]
language_id = "rust"
extensions = ["rs"]
command = ["rust-analyzer"]

[[lsp_servers]]
language_id = "python"
extensions = ["py"]
command = ["pylsp"]
```

A server is spawned lazily the first time you open a file whose extension it
handles, and shut down (`shutdown` + `exit`) when Vix exits.

## Features

| Feature         | How to use                          | Notes                                                        |
| --------------- | ----------------------------------- | ------------------------------------------------------------ |
| Diagnostics     | automatic                           | Colored underlines (red error, yellow warning, cyan info, blue hint) on a channel separate from spellcheck. |
| Go to Definition | `F12` / Tools → Language Server     | Uses `textDocument/definition`; falls back to the heuristic cross-workspace search when no server handles the file. |
| Hover           | Tools → Language Server → Hover     | Tooltip with type/doc text for the symbol under the cursor; dismissed by the next keypress. |
| Completion      | `Ctrl+Space`                        | A list anchored at the cursor; `↑`/`↓` move, `Enter`/`Tab` accept, `Esc` cancels. The accepted text extends the already-typed prefix. |

## Position encoding

LSP columns are code-unit offsets in the server's negotiated encoding (UTF-16 by
default; UTF-8/UTF-32 honored if the server reports `positionEncoding`). The host
converts between those and the editor's char offsets per line with
`vix_lsp::position`, so multi-byte and astral characters map correctly.

## Document sync

Edits are detected with the editor's monotonic content revision: once per
event-loop tick the active document is compared to the last value synced, and a
`didChange` (full text) is sent only when it actually changed. The first sync of a
file sends `didOpen`; closing its tab sends `didClose`.
