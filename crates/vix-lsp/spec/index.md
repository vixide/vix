# Language Server Protocol (LSP)

LSP client: process management and document sync over `vix_lsp_core`. Vix
speaks the Language Server Protocol to bring semantic features — **diagnostics**,
**hover**, **go-to-definition**, and **completion** — to any language you have a
server for.

## As implemented in Vix

**Status:** Shipped. The protocol core lives in the internal `lsp_core` crate
(JSON-RPC framing, message builders, response/diagnostic parsers, and char↔UTF-16
position maths — all pure and unit-tested). The host (`crates/vix-lsp/src/lib.rs`) owns the IO:
it launches a server per language, reads its framed stdout on a background thread
(an `mpsc` channel, like the run-command feature), writes requests to stdin, runs
the `initialize`/`initialized` handshake, and keeps each open document in sync with
full-text `didChange`. `App::poll_lsp` drains server messages once per event-loop
iteration.

There is **no built-in server** — Vix launches only what you configure.

## Configuration

LSP is controlled by two settings (see `docs/configuration/index.md`):

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

This table was, until the T123 audit (below), badly stale — it listed 4
features when ~28 request/notification methods were already wired
end-to-end. Every method listed here is reachable from the UI (an action,
a keybinding, or an automatic trigger), not just present unused in
`vix-lsp-core`.

| Feature | How to use | Notes |
| ------- | ---------- | ----- |
| Diagnostics | automatic | Colored underlines (red error, yellow warning, cyan info, blue hint), separate channel from spellcheck. Push-based (`textDocument/publishDiagnostics`) only — see § Known gaps. Aggregated across every opened file into a workspace Problems panel (`lsp.diagnostics`); a diagnostic's `relatedInformation` (T134) — secondary locations like "previous definition here" — appears as indented, separately navigable rows right after it. |
| Go to Definition / Declaration / Type Definition / Implementation | Tools → Language Server | Falls back to the heuristic cross-workspace search when no server handles the file. |
| Hover | Tools → Language Server → Hover | Tooltip with type/doc text for the symbol under the cursor; dismissed by the next keypress. |
| Completion | `Ctrl+Space` | A list anchored at the cursor; `↑`/`↓` move, `Enter`/`Tab` accept, `Esc` cancels. `completionItem/resolve` fills in fuller detail/documentation lazily, once an item is selected. |
| Signature Help | `lsp.signature_help`, or automatically right after typing `(`/`,` inside a call | Popup with the active parameter highlighted. |
| Find References | `lsp.references` (Tools → Language Server) | Every reference across the workspace, not just the current file. |
| Rename | `lsp.rename` | Sends `textDocument/prepareRename` first (T134): a server-validated "not renameable here" shows a status message instead of opening a prompt that would only fail, and a server-supplied placeholder seeds the prompt when one is given. A server with no `prepareRename` support falls back to the pre-T134 behavior (host's own word-under-cursor guess) rather than erroring. Applies the resulting `workspace/applyEdit` across every affected file. |
| Code Actions | `lsp.code_action` | Quick-fixes and refactors offered at the cursor/selection; a command-only action executes via `workspace/executeCommand`. |
| Code Lens | automatic, per visible line | Inline invokable annotations (e.g. "▶ Run test") a server attaches to a line. |
| Formatting / Range Formatting | `lsp.format`, and format-on-save | Whole-document when there's no selection, `textDocument/rangeFormatting` when there is. Unrelated to `vix-format-tool` (that's data-format normalization — JSON/YAML/TOML — not source-code style). |
| Document Symbols / Workspace Symbols | `lsp.document_symbols` / `lsp.workspace_symbols` | Hierarchical (nested `children`) document symbols. Distinct from `vix-outline-panel`'s own Tree-sitter-based outline — the two are independent sources, not merged. |
| Document Highlight | automatic, cursor-follow | Highlights every occurrence of the symbol under the cursor in the active file. |
| Inlay Hints | automatic, toggled by `show_inlay_hints` | Inline type/parameter-name annotations. |
| Folding Ranges | automatic, on open | Feeds the editor's own code-folding. |
| Selection Range | expand/shrink selection | Walks the server's `parent` chain of enclosing ranges around the cursor. |
| Linked Editing Range | automatic | Ranges (e.g. an open/close tag pair) that should be edited together. |
| Call Hierarchy | `lsp.call_hierarchy` | `prepareCallHierarchy` + `callHierarchy/incomingCalls` (outgoing calls not wired). |

## Known gaps against LSP 3.17 (T123 audit, 2026-09-16)

A full method-by-method diff against LSP 3.17 (not just the features this
task's own suspect list named) found the real feature set is much larger
than the stale table above previously showed, and turned up gaps in three
different shapes — some already fixed as part of this audit, some real and
still open, and some the task suspected that turned out not to be gaps at
all:

**Not a gap** (task suspected, audit found already implemented):
document formatting / range formatting (above) — fully wired, unrelated to
`vix-format-tool`.

**Fixed as part of this audit** (small, safe, no new protocol surface):
- The `initialize` request's advertised `capabilities` didn't match reality
  — it claimed `"didSave": false` while `textDocument/didSave` is actually
  sent, and declared no support at all for most already-implemented
  features (rename, code actions, document/workspace symbols, signature
  help, references, code lens, inlay hints, folding/selection ranges,
  document highlight, linked editing, call hierarchy, `workspace/applyEdit`,
  `workspace/executeCommand`) — a spec-correct server could reasonably
  withhold behavior for capabilities a client never declared. Every one of
  those is now declared, matching what's actually requested/handled.
- A JSON-RPC `error` response (as opposed to a `result`) was silently
  dropped — a failed rename/code-action/format/… just appeared to do
  nothing, with no feedback. Now surfaced to the status line
  (`LspEvent::RequestFailed`, `status.lsp_request_failed`).
- Signature help existed but was manual-invoke-only; it now also
  auto-triggers right after typing `(`/`,` inside a call, matching every
  other editor's convention for the feature.

**Still open** (real gaps, each its own follow-up task below, none in any
prior cut list):
- **Semantic tokens** (`textDocument/semanticTokens/*`): zero
  implementation. Genuinely additive to Tree-sitter's purely syntactic
  highlighting — things requiring type/binding resolution (mutable vs.
  immutable binding, trait-default vs. inherent method, unused
  variable/parameter) that Tree-sitter structurally cannot know.
- **One server per `language_id`, never per-buffer**: `Lsp`'s server
  registry is `HashMap<String, Server>` keyed by `language_id`, and
  `config_for` takes the *first* matching config by extension — there is no
  path for two servers to both run against the same file (a common
  real-world setup: a type-checker LSP + a separate linter LSP on the same
  buffer).
- **Pull-based `workspace/diagnostic`**: not used — Vix relies entirely on
  push (`publishDiagnostics`), so the Problems panel only ever shows
  diagnostics for files that have actually been opened/synced at least
  once, not a server's whole-project analysis.
- **No `$/progress`**: a long-running server operation (e.g. an initial
  index build) gives no percentage/message feedback beyond the busy-poll
  rate speeding up.
- **Single-root only**: `initialize` sends one `rootUri`, no
  `workspaceFolders` array; Vix's own editor-level multi-root concept
  (`App::workspace_folders`) isn't propagated to LSP servers at all.

**Closed since the audit above**: `prepareRename` (§ Features, "Rename")
and `relatedInformation` (§ Features, "Diagnostics") — both T123e; and
**server crash recovery** — T123c. A crashed server (the reader thread
detects the dead process via EOF) is now respawned automatically, up to 3
consecutive attempts since it last stayed up for 30 seconds (a genuine
crash loop — a bad command, a real bug — gets 3 tries then a
`msg.lsp_server_crashed` message instead of respawning forever; an
isolated crash after a long healthy run gets its own fresh budget). Every
file that was open on the crashed server gets a `didOpen` replayed with
its *current* buffer content once the new process is ready
(`LspEvent::ServerRestarted`) — `Lsp` never holds buffer content itself,
only the host does, so this is a `Lsp`→host→`Lsp` round trip, not
something `Lsp` can do alone. All three were done as part of the T134
security/depth re-audit rather than deferred.

See `tasks.md`'s T123a–T123f for the implementation status of each open
item above.

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
