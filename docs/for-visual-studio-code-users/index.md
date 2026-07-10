# Vix™ for VS Code users

VS Code got a lot right: the command palette, quick open, sane defaults, an
integrated terminal, and extensions for everything. Its one structural cost is
that it is a browser wearing a trench coat — hundreds of megabytes of Electron
between you and your text. Vix asks a simple question: what if the VS Code
*workflow* ran as a 10-millisecond native binary, entirely in your terminal?

## The one-minute pitch

Vix is a terminal IDE written in Rust. It keeps the ideas you like — command
palette, quick open, file explorer sidebar, bottom panel with a terminal,
Problems view, git integration, LSP everything — and drops the browser. It
ships a **VS Code keymap** (macOS and Windows flavors), so your fingers mostly
already know it. Works over ssh, in tmux, on a Raspberry Pi, in the recovery
console — anywhere a terminal goes.

```sh
cargo run                    # open Vix in the current directory
# then: View -> Keymap -> VSCode macOS (or VSCode Windows)
```

## Your muscle memory, mapped

With the VS Code keymap active (`Ctrl` stands in for `Cmd` on macOS):

| VS Code habit | In Vix |
| ------------- | ------ |
| `Ctrl+P` | Quick Open (fuzzy file finder) |
| `Ctrl+Shift+P` | Command Palette |
| `Ctrl+G` / `Ctrl+Shift+O` / `Ctrl+T` | Go to line / symbol / workspace symbol |
| `Ctrl+B` / `Ctrl+Shift+E` | Toggle sidebar / focus explorer |
| `Ctrl+J` / `` Ctrl+` `` | Toggle bottom panel / terminal |
| `Ctrl+S` / `Ctrl+W` / `Ctrl+Shift+T` | Save / close / reopen closed |
| `Ctrl+F` / `Ctrl+R` / `Ctrl+Shift+F` | Find / replace / find in files |
| `Ctrl+/` | Toggle comment |
| `Ctrl+Shift+K` | Delete line |
| `Ctrl+Shift+L` | Select all occurrences (multi-cursor) |
| `Alt+Up` / `Alt+Down` | Move line up / down |
| `Ctrl+\` | Split editor |
| `Ctrl+]` / `Ctrl+Shift+\` | Match bracket |
| `Ctrl+Shift+M` | Problems (diagnostics) |
| `F12` | Go to definition |
| `F1` | Keyboard help |

The features behind them are the ones you expect: LSP diagnostics, hover,
completion, rename, references, code actions, inlay hints, call hierarchy; a
DAP debugger with breakpoints, stepping, watches, and a REPL; a test runner
with jump-to-failure; tasks; snippets with Tab-stops; multi-cursor and column
selection; split panes; minimap; sticky scroll; indent guides; bracket
colorization; word-occurrence highlighting; format-on-save and auto-save.

## Where Vix beats VS Code

- **Weight.** A single native binary vs. an Electron app. Instant startup, tiny
  memory footprint, no background update service, no telemetry.
- **The terminal is the native habitat.** ssh into a server and run the *same
  full IDE* there — no Remote-SSH extension, no server-side installer, no
  version skew. tmux, mosh, containers: it just runs.
- **No extension roulette.** LSP, debugger, git, themes, and keymaps are core
  features, versioned with the editor and tested together. Nothing to install,
  nothing to break on update, no supply-chain surface of a thousand `node_modules`.
- **Privacy by architecture.** No accounts, no marketplace, no phone-home.
- **A few things VS Code does not have built in**: an Org-mode suite with
  roam-style notes and backlinks, a branching **persistent undo tree**,
  clipboard history with a picker, a `.http` REST client, leap-style jump
  labels, and a spreadsheet-style CSV editor.

## Where VS Code still wins

Credit where due:

- **The extension marketplace.** Fifty thousand extensions cover long-tail
  needs Vix never will. If your workflow depends on a specific extension
  (Jupyter, remote containers, a proprietary language pack), VS Code keeps it.
- **Rich GUI surfaces.** Image diffing, embedded webviews, notebook UIs,
  drag-and-drop — a terminal grid cannot render those. (Vix does view images
  and render Markdown, within terminal limits.)
- **Polish under a mouse-first workflow.** Vix supports the mouse well
  (click, drag, wheel, menus), but its center of gravity is the keyboard.
- **Pair tooling.** Live Share has no terminal equivalent here (tmux sharing
  is a different beast).

## An honest experiment

1. `cargo build --release`; put `vix` on your `PATH`.
2. In a real project: `vix .` — switch the keymap: `Ctrl+Shift+P`, type
   `keymap`, choose **VSCode macOS** or **VSCode Windows**.
3. Spend a day inside it. `Ctrl+P` to move around, `` Ctrl+` `` for the
   terminal, `Ctrl+Shift+M` when the compiler complains, `F12` to chase
   definitions.
4. Then try it where VS Code cannot follow: ssh to a server and open the same
   project remotely with nothing but this one binary.

Worst case, you go back and VS Code feels roomy. Best case, you notice the fan
stopped spinning.

## See also

- [`docs/keymaps/index.md`](../keymaps/index.md) — every keymap, every binding.
- [`docs/language-server-protocol/index.md`](../language-server-protocol/index.md)
  — configuring language servers.
- [`index.md`](../../index.md) — the full feature tour.

---

Vix™ and Vix IDE™ are trademarks.
