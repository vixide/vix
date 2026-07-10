# Vix™ for Eclipse users

Eclipse taught a generation of engineers what an IDE workspace is: perspectives,
Open Resource, Quick Outline, incremental builds. It also taught us what a long
startup feels like. Vix is a different bargain: a real IDE — language
intelligence, debugger, git, test runner — as one small native binary that
opens in a terminal before Eclipse finishes drawing its splash screen.

## The one-minute pitch

Vix is a terminal IDE written in Rust. Language features come from LSP
(diagnostics, completion, rename, references, code actions, formatting), the
debugger speaks DAP (the protocol Eclipse's own LSP4E/debug tooling helped
popularize), and it ships an **Eclipse keymap**, so `Ctrl+Shift+R`, `Ctrl+O`,
and `Ctrl+3` behave the way your hands remember.

```sh
cargo run                    # open Vix in the current directory
# then: View -> Keymap -> Eclipse
```

## Your muscle memory, mapped

| Eclipse habit | In Vix (Eclipse keymap) |
| ------------- | ----------------------- |
| Quick Access (`Ctrl+3`) | `Ctrl+3` — the command palette |
| Open Resource (`Ctrl+Shift+R`) | `Ctrl+Shift+R` — fuzzy file finder |
| Open Type (`Ctrl+Shift+T`) | `Ctrl+Shift+T` — workspace symbol |
| Quick Outline (`Ctrl+O`) | `Ctrl+O` — go to symbol |
| Go to Line (`Ctrl+L`) | `Ctrl+L` |
| Find / Replace (`Ctrl+F`) | `Ctrl+F` / `Ctrl+R` |
| Find Next / Previous (`Ctrl+K` / `Ctrl+Shift+K`) | `Ctrl+K` / `Ctrl+Shift+K` |
| Search (`Ctrl+H`) | `Ctrl+H` — find in workspace |
| Word completion (`Alt+/`) | `Alt+/` — buffer-word autocomplete |
| Toggle Comment (`Ctrl+/`) | `Ctrl+/` |
| Delete Line (`Ctrl+D`) | `Ctrl+D` |
| Format (`Ctrl+Shift+F`) | `Ctrl+Shift+F` — LSP format |
| Toggle Breakpoint (`Ctrl+Shift+B`) | `Ctrl+Shift+B` |
| Build / run tests (`Ctrl+B`) | `Ctrl+B` — run the test suite |
| Save / Save As / Close / Redo | `Ctrl+S` / `Ctrl+Shift+S` / `Ctrl+W` / `Ctrl+Y` |

Around them: a file-explorer sidebar (your Package Explorer instinct), a
Problems view (`Ctrl+Shift+M` in the VS Code keymap; Tools → Language Server →
Diagnostics in any keymap), split panes, an integrated terminal, tasks, and
full git tooling with per-hunk staging and a merge-conflict resolver.

## Where Vix beats Eclipse

- **Startup, memory, and no workspace ceremony.** One native binary, instant
  open, no `.metadata` directory, no "Building workspace…" in the corner.
- **Terminal-native.** Works over ssh, in tmux, in containers — the whole IDE
  goes wherever a shell goes. Eclipse RAP/Che need a very different setup for
  that.
- **No plugin-compatibility matrix.** LSP, DAP, git, themes, and keymaps ship
  in the core, versioned and tested together. No update site roulette.
- **Modern editing niceties built in**: multi-cursor, fuzzy palette, snippets,
  sticky scroll, minimap, jump labels, clipboard history, a persistent
  branching undo tree (local history, but structured), Org-mode notes.
- **Memory safety.** Rust with `unsafe` forbidden; a different reliability
  posture than a large JVM plugin surface.

## Where Eclipse still wins

- **Java depth.** JDT's compiler-backed refactoring, quick fixes, and
  incremental builds remain best-in-class for Java. Vix's Java story is
  whatever `jdtls` (the same engine, via LSP) provides — good, but the full
  IDE integration is richer in Eclipse.
- **The plugin universe.** Modeling tools, embedded/RCP development, vendor
  toolchains — decades of ecosystem Vix does not attempt.
- **GUI surfaces.** Visual diff/merge, GUI builders, profilers.
- **Perspectives and multi-project workspaces.** Vix has workspaces with
  multiple folders, but not Eclipse's perspective switching.

## An honest experiment

1. `cargo build --release`; put `vix` on your `PATH`.
2. In a project: `vix .` — press `Ctrl+3` (Quick Access lives!) and type
   `keymap` to choose **Eclipse**.
3. Navigate the way you always have: `Ctrl+Shift+R` to a file, `Ctrl+O` to a
   symbol, `Ctrl+H` to search the workspace, `Alt+/` to complete a word.
4. Then try what Eclipse makes heavy: open the same project on a remote box
   over ssh, stage one hunk from the Git menu, and notice the editor started
   faster than your last workspace switch.

Eclipse earned its place; nothing here asks you to uninstall it. But for the
80% of editing that does not need JDT's full weight, `vix .` may become the
thing you reach for first.

## See also

- [`docs/keymaps/index.md`](../keymaps/index.md) — every keymap, every binding.
- [`docs/language-server-protocol/index.md`](../language-server-protocol/index.md)
  — configuring language servers.
- [`index.md`](../../index.md) — the full feature tour.

---

Vix™ and Vix IDE™ are trademarks.
