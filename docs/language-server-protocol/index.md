# Language Server Protocol

Vix™ speaks the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
(LSP), so you get real, compiler-grade help — diagnostics, completion, hover,
go-to-definition, rename, and more — from the same language servers other editors
use. Vix ships **no built-in server**: you point it at a server you have
installed (e.g. `rust-analyzer`, `pyright`, `gopls`, `typescript-language-server`),
and Vix manages the process and protocol.

The protocol core lives in the `vix-lsp-core` crate (JSON-RPC framing, request
builders, response parsers, and char↔UTF-16/UTF-8 position maths); the process IO
and editor wiring live in the `vix-lsp` crate. The authoritative method list is
[`crates/vix-lsp/spec/language-server-protocol.tsv`](../../crates/vix-lsp/spec/language-server-protocol.tsv).

## Configuration

Two settings (see [configuration](../configuration/index.md)):

- `lsp_enabled` — master on/off switch (on by default).
- `lsp_servers` — a list mapping language ids / file extensions to a server
  command. Each entry names the command to spawn and the extensions it handles.

Servers start lazily when you open a matching file. Vix advertises UTF-16 and
UTF-8 position encodings and adopts whatever the server selects.

## What you get

Most actions live under **Tools → Language Server** and the command palette
(`>LSP …`); a few have direct keys.

| Capability | How to use it | LSP method(s) |
| ---------- | ------------- | ------------- |
| **Diagnostics** | Errors/warnings underline in color; **Diagnostics…** opens a workspace-wide list (Enter jumps). A server that waits to be asked is asked after each open and shortly after you stop typing. | `publishDiagnostics`, `textDocument/diagnostic`, `workspace/diagnostic` |
| **Hover** | Type info / docs popup for the symbol under the cursor. | `hover` |
| **Completion** | Auto-complete popup; detail/docs fill in as you scroll the list. | `completion`, `completionItem/resolve` |
| **Go to definition** | `F12` jumps to where a symbol is defined. | `definition` |
| **Implementation / type definition** | Jump to the implementation or the type's definition. | `implementation`, `typeDefinition` |
| **Find references** | Lists every use in the results panel (Enter jumps). | `references` |
| **Call hierarchy** | Callers or callees of the function under the cursor in the results panel; `Alt+D` there flips between them. | `prepareCallHierarchy`, `callHierarchy/incomingCalls`, `callHierarchy/outgoingCalls` |
| **Type hierarchy** | Supertypes or subtypes of the type under the cursor; `Alt+D` flips between them. | `prepareTypeHierarchy`, `typeHierarchy/supertypes`, `typeHierarchy/subtypes` |
| **Highlight occurrences** | Marks the other occurrences of the symbol under the cursor. | `documentHighlight` |
| **Document symbols** | Outline of the current file; Enter jumps. | `documentSymbol` |
| **Workspace symbols** | Prompt for a query, jump to a symbol anywhere in the project. | `workspace/symbol` |
| **Signature help** | Parameter hints popup. | `signatureHelp` |
| **Format** | Format the whole document, or the selection. | `formatting`, `rangeFormatting` |
| **Rename** | `F2`, then a new name — applied across every file via a workspace edit. | `rename` |
| **Code actions** | Quick-fixes / refactors in a chooser; the chosen action's edit is applied. | `codeAction` |
| **Code lens** | Lists invokable lenses; running one executes its server command. | `codeLens`, `workspace/executeCommand`, `workspace/applyEdit` |
| **Expand / shrink selection** | Grow or shrink the selection to the next syntactic range. | `selectionRange` |
| **Code folding** | Fold/unfold ranges the server reports (▾/▸ in the gutter; **View → Editor**). | `foldingRange` |
| **Inlay hints** | Inline type/parameter annotations (toggle in **View → Editor**). | `inlayHint` |
| **Linked editing** | Edit linked ranges (e.g. an open/close tag pair) together. | `linkedEditingRange` |

Lifecycle (`initialize`/`initialized`/`shutdown`/`exit`) and document sync
(`didOpen`/`didChange`/`didClose`/`didSave`) are handled automatically. A server
that asks for incremental sync is sent only the changed range of each edit;
others get the whole document. Semantic tokens are requested for the lines
around the view when the server supports ranges, and refreshed shortly after
you stop typing or scroll elsewhere.

## Server status and progress

While a server reports work through `$/progress` — indexing a project,
building, loading crates — the status bar shows the newest task beside the
git branch, as `language: title: message (n%)`, with `+N` when more tasks are
in flight. It clears when the server reports the task ended.

**View → LSP Status…** lists every configured server, live:

- its name (language id) and state: *not started* (no matching file opened
  yet), *starting*, *running*, or *failed* (the command would not start, or it
  crashed more often than the automatic respawn allows);
- its command, workspace root, file extensions and number of open files;
- its in-flight progress tasks;
- the capabilities it advertised at `initialize` (`hover`, `definition`,
  `rename`, …);
- its last error since it was last restarted: the command would not start,
  it exited unexpectedly, or the message of a failed request.

Use ↑/↓ to pick a server and `r` (or Enter) to restart it; Esc closes the
panel. **Tools → Language Server → Restart Server** and **Show Server Log**
act on the active file's servers instead.

## When no server is configured

Every LSP action degrades gracefully: with no server attached it reports
"language server inactive" in the status line and does nothing else.
Go-to-definition additionally falls back to a heuristic, language-agnostic search
so the `F12` jump still works without a server.

---

Vix™ and Vix IDE™ are trademarks.
