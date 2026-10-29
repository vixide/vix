# IDE Parity Features

Eight features people miss when they move from Visual Studio or a JetBrains
IDE, built on parts Vix already had. The assessment behind them is
[`spec/ide-parity`](../../spec/ide-parity/index.md).

## Local History

Every time you save, Vix™ keeps a timestamped copy of the file, independent of
git and of the undo tree. **File → Local History…** lists them, newest first,
with size and label.

| Key | What it does |
| --- | ------------ |
| Enter | Open a read-only diff from that snapshot to the file as it is now |
| `o` | Open the snapshot's text read-only |
| `r` | Restore the whole file from the snapshot (left unsaved, so you can undo) |
| `s` | Restore only the selected lines (or the cursor line) from the snapshot |
| Esc | Close |

**File → Label Local History Snapshot…** saves a snapshot of the buffer right
now under a name; a labelled snapshot is the last to be pruned. The
`local_history_keep` setting (default 50) is how many snapshots each file
keeps; `0` turns Local History off. Snapshots live under `<config>/history/`.

## Search Everywhere

**Go → Search Everywhere** is one box over files, commands, the open file's
symbols and workspace symbols. Type and the best of each kind appear, files
first. The usual prefixes still pick one kind: `>` commands, `@` symbols,
`@@` workspace symbols, `#` buffers, `:` a line number. The ordinary
**Ctrl+P** palette is unchanged.

## Refactor, Organize Imports, Code Cleanup

These need a language server for the file.

- **Tools → LSP → Refactor…** lists only the server's refactorings (extract,
  inline, move and so on). In the list, Enter applies one and `p` previews it
  as a diff first.
- **Tools → LSP → Organize Imports** asks the server to sort and prune
  imports and applies the result.
- **Edit → Code Cleanup** trims trailing whitespace, drops blank lines at the
  end of the file and ensures a final newline, then (with a server) organizes
  imports. It does not run the formatter; use **Format** for that.

## Run configurations

**Project → Run Configuration…** runs a named configuration from
`.vix/run.toml`, which you commit so a team shares it. **Project → Edit Run
Configurations** opens the file, creating a starter if it is missing.

```toml
[[config]]
name = "build"
command = "cargo build"

[[config]]
name = "api"
command = "cargo run -p api"
cwd = "services/api"        # relative to the project root
env = { PORT = "8080" }
before = ["build"]          # runs first and must succeed; a config or a task

[[config]]
name = "stack"
compound = ["api", "web"]   # started together
```

A configuration has either `command` or `compound`, never both. `before`
entries may also name a task from `tasks.toml`. Everything runs as one shell
line through the same pipeline as **Run Command**, so output streams to the
bottom dock and only one runs at a time. On Windows the members of a compound
run one after another.

## Peek Definition

**Tools → LSP → Peek Definition** shows the definition of the symbol under the
cursor in a popup of surrounding lines without leaving the file. Enter opens
the definition; Esc closes. With no language server it uses the same text
search as Go to Definition and peeks the first match.

## CodeLens

Code lenses appear as dimmed rows **above** the lines they belong to, for
example `Run | Debug` over a Rust test or `3 references` over a function.
They are not part of the file: the cursor, selections, line numbers and
saving all ignore them. They work with soft wrap and with folds (a folded-away
line shows no lens).

- **Where they come from.** The language server's lenses (requested when a
  file opens, again half a second after you stop typing, on save, and
  whenever the server asks for a refresh; lenses the server sends without a
  title are filled in only once they scroll into view). Without a server, or
  where the server put no lens on a test, Vix adds its own **Run test** lens
  above each test it recognizes (Rust, Python, Go, JavaScript/TypeScript,
  Ruby, Elixir, the same rules as **Project → Test at Point**).
- **Running one.** **Tools → LSP → Run Code Lens on Line**
  (`lsp.code_lens_run`) runs the lens above the cursor's line; when the line
  has several it opens a list to pick from. **Tools → LSP → Code Lens…**
  (`lsp.code_lens`) lists every lens in the file, starting at the cursor.
  A command the server advertises is sent back to it; rust-analyzer's
  **Run** runs its `cargo` command in the bottom dock, and **N references**
  opens the references list. Any other client-side command (for example
  rust-analyzer's **Debug**) reports "Unsupported code lens command".
- **Turning them off.** **View → Editor → Code Lens** (`view.code_lens`), or
  `code_lens = false` in the config file.

## Postfix completion

Type an expression, a dot and a template key, then press Tab: `ready.if`
becomes `if ready { … }` and `items.for` becomes a `for item in items` loop.
Templates exist for Rust, JavaScript and TypeScript, Python, Go and the
C family (`if`, `else`, `not`, `for`, `let`/`const`/`var`, `return`, and a few
per language such as `match`, `some`, `log`, `print`). The expression may be a
path, a call or an index (`self.items`, `foo(a, b)`, `xs[0]`).
