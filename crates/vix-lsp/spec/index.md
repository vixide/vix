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
`didChange` — one ranged change when the server negotiated incremental sync,
the whole text otherwise (T615, § Document sync). `App::poll_lsp` drains server messages once per event-loop
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
handles, and shut down (`shutdown` + `exit`) when Vix exits. More than one
entry can list the same extension (T123b) — e.g. a type-checker plus a
separate linter both configured for `.rs` — and both run against every
matching file at once; see "Closed since the audit" below for exactly
which features fan out across them and which don't.

## Features

This table was, until the T123 audit (below), badly stale — it listed 4
features when ~28 request/notification methods were already wired
end-to-end. Every method listed here is reachable from the UI (an action,
a keybinding, or an automatic trigger), not just present unused in
`vix-lsp-core`.

| Feature | How to use | Notes |
| ------- | ---------- | ----- |
| Diagnostics | automatic | Colored underlines (red error, yellow warning, cyan info, blue hint), separate channel from spellcheck. The message is also shown inline at the end of its line and in the status line when the cursor reaches it (T614, § Inline diagnostics). A server that offers `diagnosticProvider` is also asked per document (`textDocument/diagnostic`, T619, § Pull diagnostics), so a pull-only server (such as Roslyn) underlines too. Aggregated across every opened file into a workspace Problems panel (`lsp.diagnostics`), which also pulls a fresh `workspace/diagnostic` report on open (T123d) so it reflects a server's whole-project analysis, not just push (`textDocument/publishDiagnostics`) for files that happen to be open. A diagnostic's `relatedInformation` (T134) — secondary locations like "previous definition here" — appears as indented, separately navigable rows right after it. |
| Progress | automatic | `$/progress` work-done reports (T123d, T732) are tracked per server and token: `begin` adds a task, `report` updates its message/percentage, `end` removes it. While any task is in flight the status bar shows the newest one (`Lsp::progress_line`, e.g. `"rust: Indexing: 3/10 crates (30%) +1"`) and clears it on `end`; `LspEvent::Progress` is still emitted for `begin`/`report`. |
| LSP Status | View → LSP Status… (`lsp.status`) | T732 — see "Server status (T732)" below. |
| Go to Definition / Declaration / Type Definition / Implementation | Tools → Language Server | Falls back to the heuristic cross-workspace search when no server handles the file. |
| Hover | Tools → Language Server → Hover | Tooltip with type/doc text for the symbol under the cursor; dismissed by the next keypress. |
| Completion | `Ctrl+Space`, or automatically while typing (T612) | A list anchored at the cursor; `↑`/`↓` move, `Enter`/`Tab` accept, `Esc` cancels. `completionItem/resolve` fills in fuller detail/documentation lazily, once an item is selected. See § Completion below for filtering, auto-trigger, and how an accepted item is applied. |
| Signature Help | `lsp.signature_help`, or automatically right after typing `(`/`,` inside a call | Popup with the active parameter highlighted. |
| Find References | `lsp.references` (Tools → Language Server) | Every reference across the workspace, not just the current file. |
| Rename | `lsp.rename` | Sends `textDocument/prepareRename` first (T134): a server-validated "not renameable here" shows a status message instead of opening a prompt that would only fail, and a server-supplied placeholder seeds the prompt when one is given. A server with no `prepareRename` support falls back to the pre-T134 behavior (host's own word-under-cursor guess) rather than erroring. Applies the resulting `workspace/applyEdit` across every affected file. |
| Code Actions | `lsp.code_action` | Quick-fixes and refactors offered at the cursor/selection; a command-only action executes via `workspace/executeCommand`. |
| Code Lens | automatic; `lsp.code_lens_run` runs the cursor line's lens, `lsp.code_lens` lists them all | T726. Drawn by the host as a dimmed virtual row above the line. `Lsp::refresh_code_lens` re-requests a file's lenses in the background (only from servers that advertise `codeLensProvider`; an error is silent) and the reply comes back as `LspEvent::CodeLensUpdate` naming the file, dropped if the document changed meanwhile. A lens sent without a command is resolved on demand with `Lsp::resolve_code_lens` (`codeLens/resolve`, when `resolveProvider` is set) as `LspEvent::CodeLensResolved`. The client declares `workspace.codeLens.refreshSupport`; a server's `workspace/codeLens/refresh` request is acknowledged and surfaced as `LspEvent::CodeLensRefresh`. `Lsp::executes_command` says whether a server advertised a command in `executeCommandProvider.commands`, so the host sends only those back as `workspace/executeCommand` and maps or refuses client-side ones (e.g. rust-analyzer's `runSingle`). |
| Formatting / Range Formatting | `lsp.format`, and format-on-save | Whole-document when there's no selection, `textDocument/rangeFormatting` when there is. Unrelated to `vix-format-tool` (that's data-format normalization — JSON/YAML/TOML — not source-code style). |
| Document Symbols / Workspace Symbols | `lsp.document_symbols` / `lsp.workspace_symbols` | Hierarchical (nested `children`) document symbols. Distinct from `vix-outline-panel`'s own Tree-sitter-based outline — the two are independent sources, not merged. |
| Document Highlight | automatic, cursor-follow | Highlights every occurrence of the symbol under the cursor in the active file. |
| Inlay Hints | automatic, toggled by `show_inlay_hints` | Inline type/parameter-name annotations. |
| Folding Ranges | automatic, on open | Feeds the editor's own code-folding. |
| Selection Range | expand/shrink selection | Walks the server's `parent` chain of enclosing ranges around the cursor. |
| Linked Editing Range | automatic | Ranges (e.g. an open/close tag pair) that should be edited together. |
| Call Hierarchy | `lsp.call_hierarchy` (callers), `lsp.call_hierarchy_outgoing` (callees) | `prepareCallHierarchy` + `callHierarchy/incomingCalls` / `outgoingCalls` (T620, § Call and type hierarchy). `Alt+D` in the result list flips the direction. |
| Type Hierarchy | `lsp.type_hierarchy_supertypes`, `lsp.type_hierarchy_subtypes` | `prepareTypeHierarchy` + `typeHierarchy/supertypes` / `subtypes` (T620). `Alt+D` in the result list flips the direction. |
| Semantic Tokens | automatic, re-requested on content change and on scrolling (T123a, T615) | `textDocument/semanticTokens/range` for the lines around the view when the server offers it, else `full/delta` or `full` (§ Semantic token requests), decoded against the `initialize`-time legend and merged into the syntax highlighting layer alongside (winning ties over) Tree-sitter highlighting — including for grammars with no Tree-sitter query at all, where this becomes the only highlighting source. Mapped onto the 4 syntax color slots every theme defines (`comment`/`keyword`/`number`/`string`); an unmapped token type is silently skipped rather than left unstyled-but-logged or erroring. Modifiers and additional theme slots are a possible follow-up, not done here. |

## Completion (T612)

**Parsing.** `parse_completion` returns a `CompletionList`: the items plus
`is_incomplete` (`CompletionList.isIncomplete`; a bare `CompletionItem[]`
is complete). Each `CompletionItem` keeps, besides `label`/`insert_text`/
`detail`/`data`: the `textEdit` range (`range`, or an `InsertReplaceEdit`'s
`insert` range), `additionalTextEdits` (`additional_edits` — e.g. the
auto-import line), `filterText`, `sortText`, `kind`, and
`commitCharacters`. The client advertises `insertReplaceSupport` and
`commitCharactersSupport` to match. The server's
`completionProvider.triggerCharacters` are read at `initialize` and kept
per server (`Lsp::is_completion_trigger`).

**Filtering as you type.** The popup keeps the server's full list and a
fuzzy-filtered, ranked view of it (`vix-palette`'s `fuzzy_score`), matched
against each item's `filterText` (else its label) using the text typed
between the popup's anchor (the start of the word at the cursor when the
list arrived) and the cursor. Ties and the empty query fall back to
`sortText` (else label) order. Typing an identifier character (letter,
digit, `_`) or Backspace keeps the popup open and re-filters it; when the
list was `isIncomplete`, each such edit re-requests completion
(`triggerKind` 3) and the reply replaces the full list. The popup closes
when the cursor leaves the word (moves before the anchor or off the line),
when nothing matches a complete list, or on any other key. Typing one of
the selected item's commit characters accepts it first, then inserts the
character.

**Accepting.** An item with a `textEdit` range replaces that range —
its end extended to the cursor, since the user may have typed past it while
the reply was in flight — with the item's text; one without replaces the
typed word prefix (the pre-T612 behavior, which the non-LSP Org-roam and
Org-contacts completers still use). The item's `additionalTextEdits` are
applied in the same step through the same `apply_edits_to_text` as rename
and formatting, as one undo step, and the cursor lands after the inserted
text (shifted by any additional edit before it).

**Auto-trigger.** With `lsp_auto_complete` on (the default), typing an
identifier character or one of the server's trigger characters while no
popup is open requests completion after a short debounce (100 ms), so a
burst of keystrokes sends one request; a trigger character is reported to
the server as `triggerKind` 2 with its `triggerCharacter`, an identifier
character as `triggerKind` 1. Silent when no server handles the file.
`Ctrl+Space` (`lsp.complete`) still requests immediately.

**Snippet completions (T613).** The client advertises
`completionItem.snippetSupport: true`, and each `CompletionItem` keeps its
`insertTextFormat` (`insert_text_format`; `is_snippet` for format 2). An
accepted snippet item goes through the same range replacement and
`additionalTextEdits` as above, but inserts its body as parsed by the
shared `vix-snippet-tool` tabstop engine in its LSP dialect
(`parse_lsp`): the tabstop markers are removed, choices insert their first
option, and variables resolve to their default text or to nothing. A
tabstop session then starts over the inserted text — the same session the
snippet picker uses — so `$1`'s placeholder is selected, Tab moves to `$2`,
…, then `$0`, a further Tab (or Esc) ends it, and a nested placeholder
typed over is skipped. Accepting a snippet item while a session is
already running replaces that session. A plain-text item (format 1, or
none) is inserted verbatim, `$` and all.

**Multiple servers.** Each handling server's reply arrives as its own
`LspEvent::Completion` and replaces the popup's list (unchanged from
T123b); trigger characters are the union across every server handling the
file.


## Pull diagnostics (T619)

Push (`textDocument/publishDiagnostics`) is not the only way a server reports
problems: LSP 3.17 servers that offer `capabilities.diagnosticProvider` can
wait to be asked, and some (Roslyn, for one) *only* answer when asked. Vix
asks each such server per document with `textDocument/diagnostic`:

| When | What is sent |
| ---- | ------------ |
| The server's `initialize` reply offers `diagnosticProvider` | a pull for every document already open on it (the `didOpen`s queued during startup) |
| `didOpen` on a ready server that offers it | a pull for that document |
| `didChange` | a pull 250 ms after the last change to that document (`PULL_DEBOUNCE`), so a burst of keystrokes sends one request; `Lsp::poll` sends it once due, and `Lsp::busy` stays true meanwhile so the event loop ticks fast enough |
| The server sends `workspace/diagnostic/refresh` | an acknowledgement (`null`), then a pull for every document open on that server |

Each pull carries the `resultId` of that server's last report for that
document as `previousResultId` (none the first time; forgotten on
`didClose` and on a restart). The reply is a
`DocumentDiagnosticReport` (`message::parse_document_diagnostic_report`):

- `kind: "full"` — its `items` replace that server's diagnostics for the file
  in the same per-(file, server) map push fills (T123b), and its `resultId`
  is kept for the next pull. An empty `items` clears them.
- `kind: "unchanged"` — the server's previous report still stands: the
  diagnostics are left as they are and only the `resultId` is updated.
- `relatedDocuments` — the same two kinds for other files (a header whose
  diagnostics changed because this file did), merged the same way.

A reply is dropped when the document changed or closed while the request was
out (its positions would describe other text); the pull that change scheduled
brings the current report. An error reply (`ServerCancelled`, an unsupported
method) is silent, like the other ambient requests. Push and pull can both
come from one server; whichever arrives last for a file wins, as with two
pushes. The client declares `workspace.diagnostics.refreshSupport: true`.
Nothing in the host changed: underlines, inline text and the Problems panel
already read the merged map every tick.

Tested by `message` unit tests (full, unchanged and related reports) and in
`tui/tests/lsp_smoke.rs` against a pull-only Python mock (no
`publishDiagnostics` at all): its report produces underlines in a live
`App`, a second pull carries the first `resultId`, an `unchanged` reply
keeps the diagnostics, and a refresh request re-pulls.

## Call and type hierarchy (T620)

| Action | Menu (Tools → Language Server) | Requests |
| ------ | ------------------------------ | -------- |
| `lsp.call_hierarchy` | Call Hierarchy (Callers) | `textDocument/prepareCallHierarchy`, then `callHierarchy/incomingCalls` |
| `lsp.call_hierarchy_outgoing` | Call Hierarchy (Callees) | `textDocument/prepareCallHierarchy`, then `callHierarchy/outgoingCalls` |
| `lsp.type_hierarchy_supertypes` | Type Hierarchy (Supertypes) | `textDocument/prepareTypeHierarchy`, then `typeHierarchy/supertypes` |
| `lsp.type_hierarchy_subtypes` | Type Hierarchy (Subtypes) | `textDocument/prepareTypeHierarchy`, then `typeHierarchy/subtypes` |

The prepare step fans out to every server handling the file (T123b). Each
server's first prepared item comes back as `LspEvent::CallHierarchyPrepared`
or `LspEvent::TypeHierarchyPrepared` carrying a `HierarchyItem`: the item's
raw JSON plus `server`, the language id of the server that produced it.
`Lsp::request_incoming_calls`, `request_outgoing_calls`, `request_supertypes`
and `request_subtypes` take that `HierarchyItem` and send the follow-up to
that server only, since the item is opaque to every other server (this closes
T123b's call-hierarchy limit). The reply is one `LspEvent::Hierarchy` with its
`HierarchyDirection` (`Incoming`, `Outgoing`, `Supertypes`, `Subtypes`) and
the locations, sent even when empty so the host can say there are none:

| Direction | Location of each result |
| --------- | ----------------------- |
| Incoming | the call site in the caller: its first `fromRanges` entry (else the caller's `selectionRange`, else `range`) |
| Outgoing | the callee itself: `to.uri` at `to.selectionRange` (else `range`); the `fromRanges` are call sites in the current function, which the user is already looking at |
| Supertypes / Subtypes | each `TypeHierarchyItem`'s `uri` at its `selectionRange` (else `range`) |

The host shows the results in the static result list the references use,
titled by direction (`ui.hierarchy_incoming`, `…_outgoing`,
`…_supertypes`, `…_subtypes`). It keeps the prepared item while that list is
open, so **`Alt+D`** flips the direction (callers ⇄ callees, supertypes ⇄
subtypes) by asking the same server again, without preparing anew. An empty
result shows `status.hierarchy_none` instead of an empty list.

The client declares `textDocument.callHierarchy` and
`textDocument.typeHierarchy`. Parsers: `message::first_hierarchy_item`,
`parse_incoming_calls`, `parse_outgoing_calls`, `parse_type_hierarchy_items`.
Host: `tui/src/app/hierarchy.rs`. Tested against a Python mock in
`tui/tests/lsp_smoke.rs` (all four directions, the toggle, and two servers
answering prepare with each follow-up reaching only its own server).

## Inline diagnostics (T614)

Diagnostics are readable where they are, not only in the Problems panel:

- **Inline.** The `inline_diagnostics` setting picks which lines show their
  diagnostic as dimmed end-of-line virtual text (the editor widget's
  `VirtualTextLayer::Diagnostics` layer), colored by severity like the underline:

  | Value | Lines that show their diagnostic |
  | ----- | -------------------------------- |
  | `"off"` | none |
  | `"cursor-line"` (default) | the cursor's line only |
  | `"all"` | every line that has one |

  Any other value behaves as `"cursor-line"`. A line shows one message: its most
  severe diagnostic (the first published, on a tie), first line only, truncated
  at the view's edge. A diagnostic belongs to the line its range starts on, the
  same line `goto` jumps to.
- **Status line.** When the cursor arrives on a line that has a diagnostic, the
  status line shows `Error: message` (or Warning / Info / Hint), whatever the
  setting. **Go → Next/Previous Issue** always shows the message of the
  diagnostic it lands on, even when the cursor was already on that line. When
  the cursor leaves the line, the message is cleared, unless something else has
  replaced it.

The selection rules are pure and unit-tested in `vix_lsp::inline`
(`InlineMode`, `inline_entries`, `diagnostic_on_line`); the host maps them to
the editor in `App::refresh_inline_diagnostics` and `App::show_cursor_diagnostic`
(`tui/src/app/inline_diagnostics.rs`).

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

**Still open**: none — the T123 audit's whole list is closed (see below).

**Closed since the audit above**: `prepareRename` (§ Features, "Rename")
and `relatedInformation` (§ Features, "Diagnostics") — both T123e; **server
crash recovery** — T123c; **pull diagnostics / `$/progress`** — T123d;
**semantic tokens** (§ Features, "Semantic Tokens") — T123a;
**multiple servers per buffer** — T123b; and **multi-root workspace
propagation** — T123f. A file can now be handled by more
than one configured server at once (a common real-world setup: a
type-checker LSP + a separate linter LSP both watching the same
extension) — `configs_for` returns every matching config, not just the
first, and document sync (`didOpen`/`didChange`/`didClose`/`didSave`) and
every per-document read request (hover, definition family, references,
completion, document/workspace symbols, code actions, formatting, rename,
signature help, code lens, inlay hints, folding/selection ranges, document
highlight, linked editing, semantic tokens, call-hierarchy preparation)
fan out to all of them — each server's response arrives as its own event,
same as always, so two servers answering the same hover request simply
produce two `LspEvent::Hover`s in sequence rather than one merged one.
Diagnostics are the one case that needed real internal restructuring
rather than just fan-out: they're now tracked per (file, server) pair
internally so a second server publishing for a file no longer clobbers
the first's report — `diagnostics_for`/`all_diagnostics` merge across
servers at read time, same external shape as before (just `Diagnostic`
instead of `&Diagnostic`, since a flattened merge can't be borrowed).
**Deliberately not fanned out**, documented in each method's own doc
comment as a real, narrow limitation rather than silently applied:
`request_completion_resolve` and `execute_command` — each continues a
response one *specific* earlier-answering server gave (a completion
item's opaque resolve payload, a code action/lens's own command), and
nothing tracks *which* server that was once more than one is active for
the same file, so these two still target only the first matching
config. Correct whenever only one server handles a file (still the
common case); when more than one does *and* the feature in question is
one of these two, no worse than before T123b. Properly fixing this
would mean tagging completion items and code lenses with their
originating server through `LspEvent`, `CompletionItem`, and `CodeLens`
— a real follow-up, not attempted here. Call- and type-hierarchy items
used to share this limit; since T620 each one is a `HierarchyItem`
tagged with the server that prepared it, and the follow-up request goes
to that server only (§ Call and type hierarchy).
A crashed server (the reader thread detects the dead process via EOF) is
now respawned automatically, up to 3 consecutive attempts since it last
stayed up for 30 seconds (a genuine crash loop — a bad command, a real bug
— gets 3 tries then a `msg.lsp_server_crashed` message instead of
respawning forever; an isolated crash after a long healthy run gets its
own fresh budget). Every file that was open on the crashed server gets a
`didOpen` replayed with its *current* buffer content once the new process
is ready (`LspEvent::ServerRestarted`) — `Lsp` never holds buffer content
itself, only the host does, so this is a `Lsp`→host→`Lsp` round trip, not
something `Lsp` can do alone. Opening the Problems panel now also pulls a
fresh `workspace/diagnostic` report from every running server (§ Features,
"Diagnostics"), merged into the same map push already fills; and a
`$/progress` update (e.g. an initial index build) now shows as a one-line
status message (`"Indexing: 3/10 crates (30%)"`) instead of nothing. All
four were done as part of the T134 security/depth re-audit rather than
deferred.
`initialize` now sends every open workspace folder, not just one
`rootUri` — `workspaceFolders: [{uri, name}, ...]` (`rootUri` stays the
first folder, kept for servers predating LSP 3.6), and a server that
asks for it (`capabilities.workspace.workspaceFolders.
changeNotifications`) is sent `workspace/didChangeWorkspaceFolders`
whenever a folder is added after startup — see "Add folder to workspace".
Each running `Server` (one per `language_id`, unchanged by T123b) simply
receives the whole folder set rather than one root; there is deliberately
no "spawn a server per root" architecture — T123b's fan-out mechanism
already lets one server answer for every folder it's told about. There
is also no "remove folder from workspace" action in the app yet, so only
the `added` half of `didChangeWorkspaceFolders` has a caller; `Lsp`'s
notification builder supports `removed` too, for whenever that action
exists.

Every item T123's own audit found is now closed.

## Position encoding

LSP columns are code-unit offsets in the server's negotiated encoding (UTF-16 by
default; UTF-8/UTF-32 honored if the server reports `positionEncoding`). The host
converts between those and the editor's char offsets per line with
`vix_lsp::position`, so multi-byte and astral characters map correctly.

## Document sync

Edits are detected with the editor's monotonic content revision: once per
event-loop tick the active document is compared to the last value synced, and a
`didChange` is sent only when it actually changed. The first sync of a file sends
`didOpen`; closing its tab sends `didClose`.

**Incremental sync (T615).** The server's `capabilities.textDocumentSync` (a
number, or `TextDocumentSyncOptions.change`) is read at `initialize`
(`vix_lsp_core::sync::SyncKind`). For a server that asked for `2`
(incremental), `Lsp` keeps the text it last sent that server for each open
document and sends one ranged change: the common prefix and suffix of the old
and new text are kept and the middle replaced (`vix_lsp_core::sync::diff`).
Every other server gets the full text (`1`; also `0` and an absent capability,
for which the full text is always safe), and so does a server whose
`initialize` has not been answered yet, since its encoding and sync kind are
still unknown. A text equal to the last one sent sends nothing and does not
bump the version.

The change is computed from two whole texts, not replayed from the editor's
edit history, so every way a buffer changes gives a correct change with no
extra hook: typing, several carets editing at once (one change spanning the
first to the last edit), undo and redo, formatting and workspace edits, a
reload from disk, and a Cowork peer's edits (`apply_remote_edits`, which bump
the revision but are deliberately absent from the editor's edit log). The edit
log (`start_edit_log`/`take_edit_log`) was not reused for this: it records only
local edits, it has a single consumer (the Cowork session drains it), and a tab
is synced only while active, so the log would have to buffer every edit made
to a background tab. Remote-authority workspaces need nothing special either:
the diff works on the buffer text, whatever machine the server runs on.

Positions in the change use the server's negotiated encoding (UTF-8, UTF-16 or
UTF-32), count every LSP line break (`\n`, `\r\n`, `\r`), and never fall
between the `\r` and `\n` of a CRLF pair; a diff that would start or end
inside a multi-byte character backs off to its boundary. A property test
(`vix-lsp-core`'s `sync` tests) applies generated edit sequences, including
an undo back to the start, and checks the server-side result equals the
buffer after every step in all three encodings; `tui/tests/lsp_smoke.rs`
checks the same end to end against a Python mock server that applies each
change itself, for typing, several carets, undo, redo and a remote edit, and
that a full-sync server still gets the whole text.

## Semantic token requests (T615)

The client advertises `semanticTokens.requests` `{ "full": { "delta": true },
"range": true }`, and reads the server's offer from
`semanticTokensProvider` at `initialize` (`message::parse_semantic_tokens_support`).
`Lsp::request_semantic_tokens(path, visible)` sends each server:

| Server offers | Request |
| ------------- | ------- |
| `range`, and the host passed the visible range | `semanticTokens/range` for that range |
| `full.delta`, and a full result with a `resultId` is stored for the document | `semanticTokens/full/delta` with `previousResultId` |
| otherwise | `semanticTokens/full` |

A full or delta reply with a `resultId` is kept as the base for the next delta
(only for a server that offers deltas). A delta reply's `edits` are applied to
that base (`message::apply_semantic_tokens_edits`, highest `start` first); one
that cannot be applied drops the base, so the next request is a full one. A
range reply is never kept as a base. Each request records the document version
it was made against, like the T652 edit-returning requests, and a reply for a
version the server no longer has is not shown (it is still kept as a delta
base, since the server's next delta is computed against it).

The host (`App::poll_semantic_tokens`, `tui/src/app/semantic_tokens.rs`) asks
once a server for the document is ready and has offered tokens
(`Lsp::semantic_tokens_ranged`), after the active document is synced. Switching
to a document, or reopening one after a server restart, asks at once; a change
to the same document waits 150 ms (`SEMANTIC_DEBOUNCE`) so a burst of
keystrokes sends one request. For a ranged server the request covers a window
of lines aligned to the view's height and reaching one view above and below
the visible lines (`token_window`); scrolling within the window sends nothing,
and scrolling out of it asks again after the same debounce. The reply replaces
the whole semantic-token layer, so lines outside the window lose their tokens
until they are scrolled to.

**Edit-returning replies are tied to their document (T652).** Formatting,
range formatting and linked-editing requests record the path, URI and the
document version each server had when the request was sent. Their replies do
not arrive as `LspEvent`s. `Lsp::poll` queues them as `EditReply { path, kind }`
values, and the host drains the queue with `Lsp::take_edit_replies()` right
after `poll`. A reply is delivered as `EditReplyKind::Edits` /
`EditReplyKind::LinkedRanges` only if that server still has the recorded
version. If the document changed or closed in the meantime, it comes back as
`EditReplyKind::Stale` instead. Both types are `#[non_exhaustive]`. (The old
`LspEvent::Edits` / `LspEvent::LinkedRanges` variants were removed in 2.0.)

The host applies `Edits` to the tab open on `path`, not to the active tab. It
also treats the reply as stale if that tab's buffer has changed since its
last sync, which catches a keystroke followed by a tab switch before the next
sync tick. A format-on-save writes the formatted tab back to its own file,
even when another tab is active by then. A stale reply cancels that re-save;
the plain save already wrote the file. Linked ranges are used only while
`path` is still the active tab. The host syncs the active document just
before sending these requests, so the version the reply is checked against
is the text the user sees. Rename, code-action and `workspace/applyEdit`
edits already name their files (`FileEdits`) and are applied by path, so a
tab switch cannot misroute them.

## Remote workspaces (T710, T716)

Servers start through the workspace authority (`vix_authority::current().command`),
so in a remote workspace they run on the remote machine, talking over the `ssh`
channel's stdio. Paths are the authority's own throughout: document, root and
workspace-folder URIs are `file://` URIs of the remote paths, and every path in a
reply (definition, references, call hierarchy, symbols, workspace edits,
diagnostics, code actions) is turned back into an authority path with
`uri_to_path` and opened or written through the authority. Under a non-local
authority (`Authority::is_local` is `false`):

- the server is looked up first with `vix_authority::can_run` (`which` on the
  remote side, or `is_file` for a configured path with a `/`); a missing one
  fails to spawn (`ServerError::SpawnFailed`) as it would locally, instead of
  an `ssh` that starts, reports "not found" and is treated as a crash;
- it starts in the first workspace folder (locally it inherits Vix's own
  working directory, as before);
- `initialize` sends `processId: null`: a server may watch the client's
  process id and exit when it is gone, and this machine's id means nothing on
  the remote one;
- diagnostic paths are keyed as reported, not canonicalized: canonicalizing
  is a round trip per published report and per `diagnostics_for` lookup
  (every frame), and the server reports the URIs it was sent, which come from
  tab paths the authority already canonicalized on open.

The App keeps language servers off in an untrusted remote workspace (T714,
`spec/remote-editing`). Tested in `tui/tests/authority_remote_tools.rs` with a
Python mock server under a `FakeAuthority` (whose paths exist only inside it)
and over a fake `ssh`.

## Restart and server log (T576)

Each server's stderr is captured on a reader thread into a per-language ring buffer of the last 2,000 lines, which survives restarts. `Lsp::server_log(path)` returns it under a `== language (command) ==` heading; **LSP → Show Server Log** opens it in a read-only tab. `Lsp::restart(path)` shuts the server(s) for `path` down, drops the diagnostics they published, resets the crash-respawn budget, spawns fresh ones and returns the files that were open on them; the host forgets their sync state so `didOpen` is replayed with current content on the next tick. **LSP → Restart Server** drives it.

## Server status (T732)

`Lsp::statuses()` returns one `ServerStatus` per configured language id, in
configuration order: the language id, the command line, the extensions, the
root (the first workspace folder), a `ServerState` (`NotStarted` — no
matching file opened yet; `Starting` — spawned, `initialize` pending;
`Running`; `Failed` — the command could not be spawned, or the crash-respawn
budget ran out), the capabilities advertised in the `initialize` result
(`vix_lsp_core::message::parse_capability_names`: every non-`false`, non-null
key of `capabilities`, a trailing `Provider` dropped, sorted), the number of
open documents, the in-flight progress lines, and the last `ServerError`
since the last restart (`SpawnFailed`, `Crashed`, or `Request(message)` for a
JSON-RPC error reply that was surfaced to the user). `Lsp::restart_language`
restarts one server by language id, exactly like `Lsp::restart` does for one
file's servers, and clears its error and failed state.

**View → LSP Status…** (`lsp.status`) is a live overlay over
`statuses()` (re-read every frame): one block per server — header with name
and state, then command, root, extensions, open files, progress,
capabilities (wrapped) and last error. ↑/↓ (or a click) highlight a server,
`r`/Enter restarts it, Esc/`q` closes.

Not done: per-server log viewing from the panel (the existing **Show Server
Log** is per active file), stopping a server without restarting it, and
showing more than the first workspace folder as the root.

## Cargo features

- `process` (default on): starting a server process. Off, `Lsp` can be built but every
  server fails to start. The root `lsp` feature enables it (`spec/cargo-features`, T595).
