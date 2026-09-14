# Setting Up LSP

Vix speaks the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
(LSP) — the same protocol VS Code, Neovim, Zed, and every other modern editor
use for compiler-grade help. Vix ships **no built-in server**: you install a
language server yourself, point Vix at it in `config.toml`, and Vix launches
and speaks to it. This tutorial wires up three real servers — `rust-analyzer`,
`pyright`, and `typescript-language-server` — against the repo's demo
workspace, then tours the features you get once a server is running.

For the general shape of the config file (location, format, how to hand-edit
it) see [`docs/configuration/index.md`](../../configuration/index.md). This
page only covers the two LSP-specific keys and the servers themselves; the
protocol overview lives at
[`docs/language-server-protocol/index.md`](../../language-server-protocol/index.md).

Open the demo workspace for this tutorial:

```sh
cd examples/demo-workspace
vix .
```

## How Vix configures a language server

LSP is controlled by two keys in `config.toml`:

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `lsp_enabled` | bool | `true` | Master switch. When `false`, no servers launch at all. |
| `lsp_servers` | list | `[]` | One entry per language server you have installed. |

Each `[[lsp_servers]]` entry has three fields:

| Field | Meaning |
| --- | --- |
| `language_id` | The language id Vix reports to the server (and the key it spawns the process under — one process per `language_id`). |
| `extensions` | File extensions (no leading dot) that route to this server. |
| `command` | The program and arguments Vix spawns, as a list. The server must speak LSP over stdio. |

A server is **not** started when Vix launches — it's spawned lazily the first
time you open a file whose extension matches one of its `extensions`, and shut
down (`shutdown`/`exit`) when Vix exits. There's no separate "workspace root"
setting: Vix sends the folder you opened Vix in (`rootUri`) at `initialize`,
which is why you should open `examples/demo-workspace` itself (or a
subdirectory of it) rather than a single file.

`lsp_servers` is empty by default, so the rest of this tutorial is about
filling it in. Open your config file — `~/.config/vix/config.toml` on Linux,
`~/Library/Application Support/rs.vix/config.toml` on macOS, or
`%APPDATA%\vix\config\config.toml` on Windows — in Vix itself (`Ctrl+O`) or
any editor.

## Rust: `rust-analyzer`

Install it via `rustup` (this is the officially distributed way to get it —
it stays in sync with your active toolchain):

```sh
rustup component add rust-analyzer
```

This adds a `rust-analyzer` shim to `~/.cargo/bin`, which is already on your
`PATH` if you have a working Rust toolchain. Confirm it:

```sh
rust-analyzer --version
```

Add it to `config.toml`:

```toml
[[lsp_servers]]
language_id = "rust"
extensions = ["rs"]
command = ["rust-analyzer"]
```

Save the config file, then open the demo workspace's Rust project:

```sh
vix examples/demo-workspace/rust-app/src/main.rs
```

The first time you touch this file, Vix spawns `rust-analyzer` and sends
`didOpen`; give it a few seconds to index the tiny crate (one file, one
dependency-free binary). Then try it on `word_counts`:

1. Put the cursor on the `word_counts(text)` call inside `main` (around
   line 8) and press **F12** (**Go to Definition**, action id
   `nav.goto_definition`). Vix jumps to the `fn word_counts` definition
   further down the file — a real `textDocument/definition` round trip, not
   the plain-text fallback (that fallback is what runs when no server is
   attached; see "When there's no server" below).
2. With the cursor still on `word_counts`, open **Tools → Language Server →
   Hover** (action id `lsp.hover`). A popup shows the inferred signature —
   `fn word_counts(text: &str) -> HashMap<&str, u32>` — pulled from
   `rust-analyzer`'s type inference, not a doc comment (this function has
   none). The popup dismisses on the next keypress.
3. Open **Tools → Language Server → Find References** (action id
   `lsp.references`, `textDocument/references`). `word_counts` is called
   twice in this file — once in `main`, once in
   `tests::counts_repeated_words` — and both show up in the results panel;
   **Enter** jumps to either.

## Python: `pyright`

Install pyright's language-server binary via npm (the package publishes two
binaries: `pyright`, a CLI type-checker, and `pyright-langserver`, the actual
LSP server — you want the second one in `command`):

```sh
npm install -g pyright
```

(No npm on hand? `pipx install pyright` also works — the `pyright-langserver`
binary name is the same either way.) Confirm it:

```sh
pyright-langserver --version
```

Add it to `config.toml`, alongside the `rust-analyzer` entry:

```toml
[[lsp_servers]]
language_id = "python"
extensions = ["py"]
command = ["pyright-langserver", "--stdio"]
```

`--stdio` matters — unlike `rust-analyzer`, `pyright-langserver` doesn't
default to stdio framing and will hang waiting for a socket without it.

Open the demo workspace's Python script:

```sh
vix examples/demo-workspace/scripts/hello.py
```

Put the cursor on `rows` inside `main()` (the `rows = load_rows(data_path)`
line) and open **Tools → Language Server → Hover**. Pyright reports the
inferred type `list[dict[str, str]]` — exactly what `csv.DictReader` returns,
per `load_rows`'s own `-> list[dict[str, str]]` annotation. Then press **F12**
on `load_rows` to jump to its definition above. `hello.py`'s own FIXME comment
(`this assumes every row has a "score" column that parses as a number`) is a
good next thing to fix with this hover information in hand: everything read
from `row["score"]` is a `str`, so any arithmetic on it needs an explicit
`int()`/`float()` conversion pyright will flag if you get it wrong — try
adding `total = sum(row["score"] for row in rows)` and watch a diagnostic
appear underlining the type mismatch.

## TypeScript / JavaScript: `typescript-language-server`

The demo workspace doesn't ship a TS/JS file, so there's no walkthrough here,
but wiring it up is the same shape. Install it (and the `typescript` package
it wraps) via npm:

```sh
npm install -g typescript-language-server typescript
```

Confirm it:

```sh
typescript-language-server --version
```

Add it to `config.toml`. Like `pyright-langserver`, it needs `--stdio`:

```toml
[[lsp_servers]]
language_id = "typescript"
extensions = ["ts", "tsx", "js", "jsx"]
command = ["typescript-language-server", "--stdio"]
```

One entry covers all four extensions since `typescript-language-server`
itself handles both languages; split it into separate `language_id =
"typescript"` / `"javascript"` entries only if you specifically want two
independent server processes (e.g. different `tsconfig.json` roots).

Your `config.toml` now has three `[[lsp_servers]]` blocks — see
[`docs/configuration/index.md`](../../configuration/index.md#settings) for
the full settings reference this is a slice of.

## The tour: what LSP gives you

Everything below lives under **Tools → Language Server** and, without
exception, the command palette (`Ctrl+P`, then `>` to search commands by
name — see [`docs/command-palette/index.md`](../../command-palette/index.md)).
A few of the most-used actions also have direct keys, shared across every
keymap.

| Feature | Menu item | Key | Action id | LSP method(s) |
| --- | --- | --- | --- | --- |
| Diagnostics | **Tools → Language Server → Diagnostics…** | — | `lsp.diagnostics` | `publishDiagnostics` |
| Hover | **Tools → Language Server → Hover** | — | `lsp.hover` | `hover` |
| Go to Definition | **Tools → Language Server → Go to Definition** | `F12` | `nav.goto_definition` | `definition` |
| Go to Implementation | **Tools → Language Server → Go to Implementation** | — | `nav.goto_implementation` | `implementation` |
| Go to Type Definition | **Tools → Language Server → Go to Type Definition** | — | `nav.goto_type_definition` | `typeDefinition` |
| Find References | **Tools → Language Server → Find References** | — | `lsp.references` | `references` |
| Call Hierarchy | **Tools → Language Server → Call Hierarchy (Callers)** | — | `lsp.call_hierarchy` | — |
| Rename Symbol | **Tools → Language Server → Rename Symbol** | `F2` | `lsp.rename` | `rename` |
| Code Action… | **Tools → Language Server → Code Action…** | — | `lsp.code_action` | `codeAction` |
| Code Lens… | **Tools → Language Server → Code Lens…** | — | `lsp.code_lens` | `codeLens`, `workspace/executeCommand`, `workspace/applyEdit` |
| Highlight Occurrences | **Tools → Language Server → Highlight Occurrences** | — | `lsp.highlight` | `documentHighlight` |
| Completion | (typing) | `Ctrl+Space` | `lsp.complete` | `completion`, `completionItem/resolve` |
| Signature Help | **Tools → Language Server → Signature Help** | — | `lsp.signature_help` | `signatureHelp` |
| Format Document | **Tools → Language Server → Format Document** | — | `lsp.format` | `formatting`, `rangeFormatting` |
| Document Symbols… | **Tools → Language Server → Document Symbols…** | — | `lsp.document_symbols` | `documentSymbol` |
| Workspace Symbols… | **Tools → Language Server → Workspace Symbols…** | — | `lsp.workspace_symbols` | `workspace/symbol` |

A few worth walking through by hand in
`examples/demo-workspace/rust-app/src/main.rs`:

- **Diagnostics.** Errors and warnings underline as you type — red for
  errors, yellow for warnings, cyan for info, blue for hints — on a channel
  separate from spellcheck. **Diagnostics…** opens a workspace-wide list of
  every current diagnostic across open files; **Enter** jumps to one.
- **Completion.** Delete the `.into_iter()` off the `pairs` line and retype
  `counts.into` — `Ctrl+Space` opens a list anchored at the cursor;
  `↑`/`↓` move, `Enter`/`Tab` accept (the accepted text extends what you've
  already typed), `Esc` cancels.
- **Signature Help.** Inside `counts.entry(word).or_insert(0)`, retype the
  `(` after `or_insert` — a parameter-hint popup shows `or_insert`'s
  signature and highlights the argument you're on.
- **Rename Symbol.** Cursor on `pairs`, press `F2`, type a new name, Enter.
  `rust-analyzer` returns a workspace edit and every use of `pairs` in the
  file updates together — the same mechanism reaches across files for a
  symbol used in more than one.
- **Workspace Symbols…** From anywhere in the project, open it and type
  `word_counts` — it jumps straight there even from a different file, unlike
  Document Symbols (`@` in the command palette), which only searches the
  current file's outline.

## When there's no server

Every LSP action degrades gracefully. With `lsp_enabled = false`, or no
`lsp_servers` entry matching the open file, running any of the actions above
reports "language server inactive" in the status line and does nothing else.
**Go to Definition** is the one exception: it additionally falls back to a
heuristic, language-agnostic cross-workspace search, so `F12` still does
*something* useful with no server configured — just without the precision a
real server gives you (it's `rust-analyzer`'s and `pyright`'s exact answers
that made the walkthroughs above reliable).

## Where to go next

- [`docs/language-server-protocol/index.md`](../../language-server-protocol/index.md)
  — the full protocol-conformance reference (every method Vix implements).
- [`docs/reference/actions.md`](../../reference/actions.md) and
  [`docs/reference/keybindings-shared.md`](../../reference/keybindings-shared.md)
  — every action id and key, generated from the same data as the table above.
- [`docs/configuration/index.md`](../../configuration/index.md) — the
  complete settings reference, including `format_on_save`, which runs
  whichever LSP formatter is attached every time you save.

---

**Previous:** [04 — The Git Workflow](../04-the-git-workflow/index.md)
**Next:** [06 — Org Mode and Roam](../06-org-mode-and-roam/index.md)

---

Vix™ and Vix IDE™ are trademarks.
