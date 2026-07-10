# Vix™ for Sublime Text users

Sublime Text made a promise the rest of the industry spent a decade catching up
to: an editor can be *fast*, beautiful, and powerful at the same time. Goto
Anything changed how everyone navigates. If Sublime is still your daily driver,
you value speed and minimalism over kitchen sinks — which is exactly the crowd
Vix was built for. The pitch: Sublime's velocity, in a terminal, with the IDE
layer (LSP, debugger, git) built in instead of bolted on.

## The one-minute pitch

Vix is a terminal IDE written in Rust: one native binary, instant startup, no
Package Control, no license nag. It ships a **Sublime Text keymap**, and its
command palette speaks Sublime's dialect — `Ctrl+P` opens Goto Anything-style
fuzzy finding, and the same palette does `@` symbols, `:` line numbers, and `>`
commands, just like the editor you know.

```sh
cargo run                    # open Vix in the current directory
# then: View -> Keymap -> Sublime Text
```

In the terminal, `Ctrl` stands in for `Cmd` on macOS.

## Your muscle memory, mapped

| Sublime habit | In Vix (Sublime keymap) |
| ------------- | ----------------------- |
| Goto Anything (`Cmd+P`) | `Ctrl+P` — fuzzy file open (`@`, `:`, `>` modes in the palette) |
| Command Palette (`Cmd+Shift+P`) | `Ctrl+Shift+P` |
| Goto Symbol (`Cmd+R`) | `Ctrl+R` |
| Goto Line (`Ctrl+G`) | `Ctrl+G` |
| Select next occurrence (`Cmd+D`) | `Ctrl+D` — selects **all** occurrences (like `Alt+F3`), a caret on each |
| Expand selection to line (`Cmd+L`) | `Ctrl+L` |
| Join lines (`Cmd+J`) | `Ctrl+J` |
| Jump to matching bracket (`Ctrl+M`) | `Ctrl+M` |
| Duplicate line (`Cmd+Shift+D`) | `Ctrl+Shift+D` |
| Delete line (`Ctrl+Shift+K`) | `Ctrl+Shift+K` |
| Comment (`Cmd+/`) | `Ctrl+/` |
| Find / Replace (`Cmd+F` / `Cmd+H`) | `Ctrl+F` / `Ctrl+H` |
| Find in Files (`Cmd+Shift+F`) | `Ctrl+Shift+F` |
| Build (`Cmd+B`) | `Ctrl+B` — runs the test suite |
| Console (`` Ctrl+` ``) | `` Ctrl+` `` — a real shell in a panel |
| Save / Save As / Close / Reopen | `Ctrl+S` / `Ctrl+Shift+S` / `Ctrl+W` / `Ctrl+Shift+T` |

Multiple selections — Sublime's crown jewel — are first-class: `Ctrl+D` puts a
caret on every occurrence, and `Alt+Shift+Up/Down` grows a rectangular
(column) selection; every caret types, deletes, and pastes together.

## Where Vix beats Sublime

- **The IDE layer is built in.** LSP (diagnostics, completion, go-to, rename,
  references, code actions), a DAP debugger, a test runner, and full git
  tooling with per-hunk staging — no Package Control safari, no LSP plugin to
  configure per machine.
- **Terminal-native.** ssh to a server and run the *same* editor there. tmux,
  containers, low-power boxes: one static binary travels anywhere.
- **Free and open source.** MIT/Apache-2.0. No license popup, ever.
- **Modern niceties out of the box**: minimap (yes, in a terminal), sticky
  scroll, indent guides, bracket colorization, jump labels, clipboard history,
  a persistent branching undo tree, Org-mode notes with backlinks, an HTTP
  client, and a CSV table editor.
- **Actively developed** with a spec-driven, fully-tested codebase in Rust
  (`unsafe` forbidden).

## Where Sublime still wins

- **Raw rendering polish.** Sublime's GPU-accelerated, pixel-perfect canvas —
  smooth scrolling, font ligatures, the pretty minimap — outclasses any
  character-grid terminal. Vix's minimap is charming; Sublime's is gorgeous.
- **Package ecosystem.** Thousands of packages and color schemes, plus a
  Python API for writing your own. Vix has no plugin system.
- **Sublime-grade micro-optimizations.** Sublime opens multi-gigabyte files
  with famous grace. Vix handles large files (async parsing over 50 KB), but
  Sublime remains the benchmark.
- **Add-next-occurrence granularity.** Vix's `Ctrl+D` selects *all* matches at
  once rather than one at a time — same destination, less ceremony, but less
  control mid-flight.

## An honest experiment

1. `cargo build --release`; put `vix` on your `PATH`.
2. In a project: `vix .` — press `Ctrl+P`, type `keymap`, pick **Sublime Text**.
3. Navigate like home: `Ctrl+P` to a file, `Ctrl+R` to a symbol, `Ctrl+D` to
   multi-edit a word, `Ctrl+Shift+D`/`Ctrl+Shift+K` to shuffle lines.
4. Then try what Sublime needs plugins for: hover an LSP diagnostic, rename a
   symbol project-wide, stage one git hunk from the Git menu, set a breakpoint
   from the Run menu — and do it all again over ssh.

Sublime taught the world that editors should be fast. Vix agrees — and adds
that they should also carry their own batteries.

## See also

- [`docs/keymaps/index.md`](../keymaps/index.md) — every keymap, every binding.
- [`docs/language-server-protocol/index.md`](../language-server-protocol/index.md)
  — configuring language servers.
- [`index.md`](../../index.md) — the full feature tour.

---

Vix™ and Vix IDE™ are trademarks.
