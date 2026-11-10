# Vix™ for IntelliJ / JetBrains users

You picked IntelliJ because it *understands* code: rename actually renames,
Find Action finds everything, and the debugger just works. The trade you
accepted is weight — gigabytes of IDE, index rebuilds, a splash screen with a
progress bar. Vix is an experiment in keeping the parts of that deal you like:
a real IDE — LSP intelligence, a debugger, refactoring, test running — as one
small native binary in a terminal.

## The one-minute pitch

Vix is a terminal IDE written in Rust. Language smarts come from LSP (the same
protocol behind most modern editors): diagnostics, completion, rename,
references, code actions, call hierarchy, formatting. A DAP debugger gives you
breakpoints, stepping, watches, and an evaluate REPL. And it ships an
**IntelliJ keymap** in both macOS and Windows flavors, so Find Action, Go to
Class, and Duplicate Line are where your fingers expect.

```sh
cargo run                    # open Vix in the current directory
# then: View -> Keymap -> IntelliJ macOS (or IntelliJ Windows)
```

In the terminal, `Ctrl` stands in for `Cmd` on macOS.

## Your muscle memory, mapped

| IntelliJ habit | In Vix (IntelliJ keymap) |
| -------------- | ------------------------ |
| Find Action (`Cmd+Shift+A`) | `Ctrl+Shift+A` — the command palette |
| Go to Class (`Cmd+O` / `Ctrl+N`) | `Ctrl+O` (macOS flavor) / `Ctrl+N` (Windows flavor) |
| Go to File (`Cmd+Shift+O` / `Ctrl+Shift+N`) | `Ctrl+Shift+O` / `Ctrl+Shift+N` |
| Go to Symbol (`Cmd+Alt+O`) | `Ctrl+Alt+O` |
| Go to Line (`Cmd+L` / `Ctrl+G`) | `Ctrl+L` / `Ctrl+G` |
| Go to Declaration (`Cmd+B`) | `Ctrl+B` (also `F12`) |
| Find / Replace | `Ctrl+F` / `Ctrl+R` |
| Find / Replace in Path | `Ctrl+Shift+F` / `Ctrl+Shift+R` |
| Find Next / Previous (`Cmd+G`) | `Ctrl+G` / `Ctrl+Shift+G` (macOS flavor) |
| Duplicate Line (`Cmd+D`) | `Ctrl+D` |
| Delete Line (`Ctrl+Y`, Windows) | `Ctrl+Y` (Windows flavor) |
| Comment (`Cmd+/`) | `Ctrl+/` |
| Reformat Code (`Cmd+Alt+L`) | `Ctrl+Alt+L` (LSP format) |
| Settings (`Cmd+,`) | `Ctrl+,` (macOS flavor) |
| Save / Save As / Close | `Ctrl+S` / `Ctrl+Shift+S` / `Ctrl+W` |

Around them: a file-explorer sidebar, split panes, an integrated terminal, a
test runner with jump-to-failure, git status / diff / blame / per-hunk staging
/ branches / stash / merge-conflict resolution, and local-history-like safety
from a **persistent branching undo tree**.

## Where Vix beats IntelliJ

- **Startup and footprint.** Milliseconds and megabytes, not minutes and
  gigabytes. No indexing phase — Tree-sitter parses your file instantly and
  LSP indexes in the background, per server.
- **Runs where IntelliJ cannot.** ssh, tmux, containers, low-power machines.
  Your whole IDE is one static binary you can `scp` to a server.
- **No license, no accounts, no telemetry.** MIT/Apache-2.0, offline-first.
- **A calmer surface.** Menus + a fuzzy palette + `F1` shortcut help instead of
  nested settings dialogs. Configuration is one readable TOML file.
- **Extras IntelliJ charges plugins or Ultimate for**: an HTTP client for
  `.http` buffers, Org-mode notes with backlinks, a CSV table editor, a hex
  editor, QR/UUID/checksum utilities — built in.

## Where IntelliJ still wins

- **Deep semantic refactoring.** LSP rename/code-actions are good; IntelliJ's
  extract-method / change-signature / move-class family is deeper, especially
  for Java/Kotlin. If you refactor large JVM codebases all day, keep IntelliJ.
- **Framework intelligence.** Spring, Android, database tooling, profilers —
  ecosystem depth a terminal IDE does not attempt.
- **GUI affordances.** Diff-merge side-by-side with drag, visual debuggers for
  collections, embedded browsers.
- **Everything works without configuring a language server.** With Vix you
  point it at `rust-analyzer` / `gopls` / `pylsp` once per language.

## An honest experiment

1. `cargo build --release`; put `vix` on your `PATH`.
2. In a project: `vix .` — open the palette (`Ctrl+P`, then `>`), type
   `keymap`, and pick **IntelliJ macOS** or **IntelliJ Windows**. From then on
   `Ctrl+Shift+A` *is* your Find Action.
3. Work normally: `Ctrl+O`/`Ctrl+N` to classes, `Ctrl+B` to declarations,
   `Ctrl+D` to duplicate, `Ctrl+Alt+L` to reformat.
4. Try the terminal-native wins: open the same project over ssh; stage a
   single hunk from the Git menu; run the debugger from the Run menu.

Keep IntelliJ for the heavy refactoring days. For everything else — quick
edits, servers, code review, notes — you may find the splash screen was the
part you needed least.

## See also

- [`docs/keymaps/index.md`](../keymaps/index.md) — every keymap, every binding.
- [`docs/language-server-protocol/index.md`](../language-server-protocol/index.md)
  — configuring language servers.
- [`index.md`](../../index.md) — the full feature tour.

---

Vix™ and Vix IDE™ are trademarks.
