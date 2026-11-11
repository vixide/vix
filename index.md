<div align="center">

# Vix IDE™

**The terminal IDE that already knows your keys.**

Save is `Ctrl+S`. Find is `Ctrl+F`. Everything else is in a menu bar or the
`Ctrl+P` command palette. Language servers, a debugger, Git, Org mode and a
database workbench come built in, so Vix is useful the moment it opens.

[![CI](https://github.com/vixide/vix/actions/workflows/ci.yml/badge.svg)](https://github.com/vixide/vix/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/vix-tui.svg)](https://crates.io/crates/vix-tui)
[![License: Apache-2.0 / BSD-3-Clause / MIT / GPL](https://img.shields.io/badge/license-Apache--2.0%20%7C%20BSD--3--Clause%20%7C%20MIT%20%7C%20GPL-blue.svg)](LICENSE)
[![GitHub Discussions](https://img.shields.io/github/discussions/vixide/vix?style=social)](https://github.com/vixide/vix/discussions)

[Website](https://vixide.github.io/) ·
[Install](#install) ·
[Documentation](https://vixide.github.io/vix/) ·
[Getting Started](docs/getting-started/index.md) ·
[FAQ](docs/faq.md) ·
[Cookbook](docs/cookbook.md) ·
[Development](docs/development.md) ·
[Discussions](https://github.com/vixide/vix/discussions) ·
[Compare](docs/compare/index.md) ·
[Changelog](CHANGELOG.md)

![Vix overview: open a file, edit, save, find, and browse keyboard shortcuts](docs/demos/overview.gif)

</div>

## Try it in 30 seconds

```sh
brew install vixide/homebrew-tap/vix-tui   # or: npm install -g @vixide/vix
vix                                        # open the current directory
```

Press `Ctrl+P` to find a file, `F1` to see every shortcut, and `Ctrl+Q` to
quit. Other ways to install, including Windows, Linux packages and
`cargo install`, are [below](#install).

## Vix at a glance

|                     |                                                                 |
| ------------------- | --------------------------------------------------------------- |
| **One binary**      | No plugin manager, no runtime, nothing to assemble first        |
| **14 keymaps**      | Apple, VS Code, Emacs, Vi, Spacemacs, Helix, IntelliJ, Eclipse, Sublime, Zed, Fresh, ttt |
| **21 themes**       | Plus a theme editor and your own JSON themes                     |
| **15 languages**    | The whole interface, from Arabic to Welsh                        |
| **0 `unsafe`**      | `unsafe` Rust is forbidden in every crate                        |
| **Your choice of 5 licenses** | Apache-2.0, BSD-3-Clause, MIT, GPL-2.0 or GPL-3.0      |

## Why Vix

- **Familiar keys, no modes by default.** Ctrl/Cmd shortcuts, a real menu bar
  (Vix File Edit View Go Run AI DB JJ Git IM Org Project OC Tools Help), mouse
  support, and `F1` for every shortcut. Fourteen keymaps — Apple (the default),
  VS Code (macOS/Windows), Emacs, Vi, Spacemacs, Helix, IntelliJ (macOS/Windows),
  Eclipse, Sublime, Zed, Fresh and ttt — switchable live from **View → Keymap**.
- **An IDE, not just an editor.** Language Server Protocol, a DAP debugger,
  Tree-sitter highlighting, Git (hunk staging, blame, an interactive commit
  graph, conflict resolver) and Jujutsu, a test runner, project tasks and an
  integrated terminal ship in one binary.
- **Things other editors leave to plugins.** An Org-mode suite with Org-roam,
  a SQLite/PostgreSQL/MySQL workbench, a REST client for `.http` files, a
  container menu for Docker and Podman, table/JSON/YAML/hex edit surfaces,
  and a persistent, branching undo tree.
- **Careful by default.** `unsafe` code is forbidden in every crate. Rhai
  scripts get no file, network or process access, and a project's own scripts
  run only after you trust that workspace. The DB workbench opens connections
  read-only. A [minimal build](#install) leaves out every network, shell and
  external-program feature for locked-down machines.
- **Yours to adjust.** 21 bundled themes plus your own, a UI fully translated
  into 15 languages, snippets, macros, and Rhai scripts that add commands and
  key bindings without a rebuild.
- **Terminal or desktop.** The same editor also runs in a native window via
  experimental [desktop GUIs](#desktop-guis).

## Who it's for

- **You live in the terminal but miss your IDE.** Go to definition, rename,
  a debugger and a Git panel, over SSH or in tmux, without a config project.
- **You tried Vim or Helix and bounced off modes.** Vix starts with the
  shortcuts every other app uses; switch on the Vi keymap when you want it.
- **You came from Emacs for Org mode.** Agenda, tables with formulas, Column
  View, Org-roam and dailies are built in, with menus to find them.
- **You work with data and APIs.** Query a database, send an HTTP request,
  inspect a container and edit the CSV it produced, all in one window.

## See it work

| | |
| --- | --- |
| ![Command palette: fuzzy-find files, then switch to command mode](docs/demos/palette.gif) | ![Multiple cursors: select every occurrence of a word and edit them at once](docs/demos/multi-cursor.gif) |
| **Command palette.** Fuzzy-find files, then type `>` for commands. | **Multiple cursors.** Edit every occurrence of a word at once. |
| ![Git hunk staging: stage two changes in one file, one hunk at a time](docs/demos/git-hunks.gif) | ![Database workbench: connect to SQLite, run a join, read the result grid](docs/demos/db-workbench.gif) |
| **Git hunk staging.** Stage one change at a time. | **Database workbench.** Run a query, read the grid. |

More recordings — Org-roam, edit surfaces and themes — are in
[`docs/demos/`](docs/demos/).

## How Vix compares

| | Vix | Vim | Emacs | Helix | Zed |
| --- | :---: | :---: | :---: | :---: | :---: |
| Runs in a terminal | ✓ | ✓ | ✓ | ✓ | - |
| Familiar keys by default (no modes) | ✓ | - | - | - | ✓ |
| Menu bar | ✓ | - | ✓ | - | ✓ |
| LSP, debugger and Tree-sitter built in | ✓ | - | ~ | ✓ | ✓ |
| Git client built in | ✓ | - | ~ | ~ | ✓ |
| Jujutsu client built in | ✓ | - | - | - | - |
| Integrated terminal | ✓ | ✓ | ✓ | - | ✓ |
| Org mode | ✓ | - | ✓ | - | - |
| Database workbench | ✓ | - | ~ | - | - |

`✓` built in, `-` not built in (a plugin or package may exist), `~` partial.
The full matrix, with footnotes and the places where Vix is behind, is in
[the feature comparison](docs/compare/index.md). Coming from another
editor? See the pages for
[Vim](docs/vix-for-vim-users/index.md),
[Emacs](docs/vix-for-emacs-users/index.md),
[Spacemacs](docs/vix-for-spacemacs-users/index.md),
[Helix](docs/vix-for-helix-users/index.md),
[VS Code](docs/vix-for-visual-studio-code-users/index.md),
[IntelliJ](docs/vix-for-intellij-users/index.md),
[Eclipse](docs/vix-for-eclipse-users/index.md),
[Sublime](docs/vix-for-sublime-users/index.md) and
[Zed](docs/vix-for-zed-users/index.md) users.

## Install

The command is `vix` however you install it.

```sh
# macOS or Linux, Homebrew
brew install vixide/homebrew-tap/vix-tui

# macOS or Linux, installer script
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/vixide/vix/releases/latest/download/vix-tui-installer.sh | sh

# Any platform with npm
npm install -g @vixide/vix

# Any platform with a Rust toolchain (1.97+)
cargo install vix-tui
```

On Linux, every release also has native packages (`.deb`, `.rpm`, Arch
`.pkg.tar.zst`) for x86_64 and aarch64. Download yours from the
[latest release](https://github.com/vixide/vix/releases/latest), then for
example `sudo apt install ./vix-tui_*_amd64.deb` or
`sudo dnf install ./vix-tui-*.x86_64.rpm`.

On Windows, use PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/vixide/vix/releases/latest/download/vix-tui-installer.ps1 | iex"
```

More channels (winget, Scoop, the AUR, a Nix flake and mise) are prepared in
[`packaging/`](packaging/index.md) but **not published yet**; `cargo binstall`
metadata is in the published crate but untested, and the container image is
built on every version tag but its ghcr.io package is not public yet, so
`docker pull` does not work yet. Use one of the commands above.

Prebuilt archives (`.tar.xz`, `.zip`, `.msi`) are attached to every
[GitHub Release](https://github.com/vixide/vix/releases), along with a
**minimal build** (`vix-minimal-<target>`): no DB workbench, scripting,
terminal, HTTP client, AI, Git/Jujutsu, containers, IRC, remote editing,
language servers, debug adapters or syntax highlighting; no TLS stack, no network connections, and Vix starts no
other program. See [Getting Started](docs/getting-started/index.md) for every
option, and use a [Nerd Font](https://www.nerdfonts.com/) for the best-looking
icons (Vix falls back to plain characters without one).

## Your first minute

```sh
vix                     # open the current directory
vix src/main.rs:42:7    # open a file at line 42, column 7
```

| Keys | Does |
| --- | --- |
| `Esc` | Close the welcome screen (reopen it from **Help → Welcome…**) |
| `Ctrl+P` | Command palette: fuzzy-find files; type `>` for commands, `@` for symbols, `:` for a line |
| `Ctrl+B` / `Ctrl+E` | Show or hide the file explorer / focus it |
| `Ctrl+O`, `Ctrl+S` | Open, save |
| `Ctrl+F`, `Ctrl+R`, `Ctrl+Shift+F` | Find, find and replace, search the workspace |
| `Ctrl+Z`, `Ctrl+Shift+Z` | Undo, redo |
| `F1` | Every shortcut in your current keymap, searchable |
| `Ctrl+Q` | Quit (Vix asks before discarding unsaved work) |

These are the default Apple keymap's bindings; on macOS `Command` works
wherever `Control` does. Every action is also in a menu, whatever the keymap.
For a guided start, run the built-in tutor or read the
[tutorials](docs/tutorials/).

## Features

**Editing.** Tabs and split panes; multi-cursor and column editing; soft wrap,
bracket matching, relative line numbers, indent guides, rainbow brackets,
sticky scroll and a minimap; surround, align, increment/toggle, transpose,
fill paragraph, comment toggle (`Ctrl+/`) and comment banners; Emmet;
snippets; clipboard history; sort/dedupe/case/line-ending transforms;
format-on-save (including your project's own Prettier), auto-save and
auto-reload; legacy encodings such as UTF-16, Shift_JIS and windows-1252,
saved back as they came; an undo tree that is saved and restored when you
reopen a file.

**Finding your way.** Command palette with file, command, buffer, line and
symbol modes; incremental find and replace with smart-case, whole-word, regex
and capture groups; workspace search and replace; go to definition (`F12`);
leap-style jump labels; structural selection; position history
(`Alt+Left`/`Alt+Right`); a file explorer with preview tabs and file
operations; a Todo Tree; Zen mode and a breadcrumb bar.

**Code intelligence.** LSP diagnostics, hover, completion, references, call
hierarchy, rename, code actions, code lens and inlay hints, with server
progress in the status bar and **View → LSP Status…**; a DAP debugger
with breakpoints, stepping, call stack, variables, watches and a REPL; a test
runner with jump-to-failure; dependency sources open read-only so Go to
Definition can't lead to an accidental edit. From JetBrains and Visual
Studio: Local History, Search Everywhere, Refactor… and Organize Imports,
named run configurations, Peek Definition and postfix completion
([IDE parity](docs/ide-parity/index.md)).

**Remote and together.** Open a folder on another machine over SSH with
`vix ssh://host/path` or **File → Open Remote…** — search, Git, language
servers, the debugger, tasks and the terminal run on the remote host
([remote editing](docs/remote-editing/index.md), experimental). An **IM**
menu for code-aware chat: `path:line` references that open on Enter, threads
anchored to lines, and IRCv3 channels ([IM](docs/im/index.md)). Real-time
co-editing with shared cursors, terminal, chat and notes
([Cowork](docs/cowork/index.md)) is an opt-in build feature
(`--features cowork`), not part of the released binaries.

**AI, on your terms.** An AI menu and chat panel that run the assistant CLI of
your choice, or an Anthropic, OpenAI or Ollama HTTP API, on a selection;
`ai = "off"` hides all of it without a rebuild. `vix --mcp` serves a
read-only view of a workspace to AI agents over the Model Context Protocol
([MCP server](docs/mcp-server/index.md)).

**Version control.** Git status, diff, blame, stage/unstage/revert per hunk,
branches, stash, amend, an interactive commit graph and a merge-conflict
resolver; a Jujutsu menu for log, bookmarks, new/describe/commit, squash,
rebase, undo, and push/fetch.

**Projects and tools.** Per-project-type build/test/run commands; tasks
discovered from npm/yarn/pnpm/bun, Deno, Composer, just, go-task, Rake and
Make; test-at-point; an integrated terminal; a REST client for `.http` files
with environments, variables and request chaining; a container menu for
Docker, Podman and nerdctl; FHIR and HL7 v2 tools.

**Data and documents.** A database workbench for SQLite, PostgreSQL and MySQL
with embedded drivers, a schema browser, autocomplete, a filterable result
grid, transactions, SSH tunnels and an ER diagram; edit a buffer as a CSV/TSV
table, a JSON/YAML tree, a hex dump or a folding outline; Markdown preview;
image viewing on graphics-capable terminals; a
[Large File Viewer](docs/large-file-viewer/index.md) that opens a 4 GiB log
to its first screen in under a millisecond and indexes it in about half a
second, in under 5 MiB of memory
([measured](docs/performance/index.md#large-file-viewer-t733)).

**Org mode.** Headlines, TODO cycling, checkboxes, folding, an agenda, a table
editor with `TBLFM` formulas, Column View, export to Markdown/HTML, contacts,
and Org-roam with link completion, dailies and a live backlinks panel.

**Look and language.** 21 bundled themes (including high-contrast and
Catppuccin) and a theme editor; `theme = "auto"` follows your terminal's
light or dark background; the UI is translated into 15 languages.

The full list, with every key and setting, is in [the docs](docs/index.md):
start with the [menus](docs/menus/index.md), the
[action reference](docs/reference/actions.md) and
[configuration](docs/configuration/index.md).

## TUI & GUI capabilities

Vix is one editor with eight front ends, every one on crates.io
(`cargo install <name>`):

- [**vix-tui**](https://crates.io/crates/vix-tui) is the terminal editor, the `vix` command, built on ratatui, and the App every other front end wraps. Try it first for the most complete and most tested Vix: it runs in any terminal on macOS, Windows and Linux, over SSH and inside tmux, and every release ships it on every channel.
- [**vix-bui**](https://crates.io/crates/vix-bui) is Vix in a native window drawn by BUI, Blender's user-interface toolkit used as a library. Try it if you like Blender's dense, flat look or want to see an editor built on a toolkit made for 3D tools; it is GPL-3.0-only and needs CMake to build.
- [**vix-egui**](https://crates.io/crates/vix-egui) is Vix in a native window drawn by egui through eframe, an immediate-mode GUI that redraws each frame. Try it for the simplest GPU front end, with input-method (IME) typing and a native macOS menu bar.
- [**vix-gpui**](https://crates.io/crates/vix-gpui) is Vix in a native window on GPUI, the GPU-accelerated framework behind the Zed editor, through GPUI Kit. Try it for the operating system's own text rendering (CoreText, DirectWrite) and a native menu bar on macOS, Windows and Linux; it has no IME typing yet.
- [**vix-gtk4**](https://crates.io/crates/vix-gtk4) is Vix in a GTK 4 window, with text drawn by Pango and Cairo. Try it on a GNOME or other GTK desktop, where it fits in natively, with a real menu bar and file dialog on every platform and full IME support; it needs GTK 4 installed.
- [**vix-iced**](https://crates.io/crates/vix-iced) is Vix in a native window on iced, an Elm-style Rust GUI library, with text shaped by cosmic-text. Try it for its advanced text shaping and built-in per-glyph font fallback, which handle mixed scripts well, plus IME typing.
- [**vix-slint**](https://crates.io/crates/vix-slint) is Vix in a native window on Slint, a declarative UI toolkit, drawn with OpenGL. Try it for native menu bars on macOS and Windows from a toolkit built for both desktop and embedded apps; it is GPL-3.0-only and has no IME typing yet.
- [**vix-zgui**](https://crates.io/crates/vix-zgui) is Vix in a native window on zgui, a reactive Rust GUI with CSS styling and a retained wgpu renderer. Try it for the newest front end, with text shaped by parley and an idle window that replays its last frame instead of redrawing; it is a first version.

## Desktop GUIs

The same editor — every panel, overlay and dock — also runs in a native window
on macOS, Windows and Linux. Four front ends ship with each release,
installed like `vix-tui` under their own names (for example
`brew install vixide/homebrew-tap/vix-gpui`): `vix-gpui` (GPUI), `vix-egui`
(egui), `vix-iced` (iced) and `vix-slint` (Slint, GPL-3.0-only). Three more,
`vix-gtk4` (GTK 4), `vix-bui` (BUI, Blender's UI library; GPL-3.0-only, needs
CMake) and `vix-zgui` ([zgui](https://github.com/zortax/zgui); also
`brew install vixide/tap/vix-zgui`), build from source with `cargo install`. They are **experimental**, built side
by side so the toolkits can be compared; see
[`gui/spec/index.md`](gui/spec/index.md).

## Configuration

Settings are TOML in the platform configuration directory (for example
`~/.config/vix/config.toml` on Linux); custom themes go in
`~/.config/vix/themes/*.json`. See
[`docs/configuration/index.md`](docs/configuration/index.md) for every key and
[`tui/examples/config/`](tui/examples/config/) for an annotated example.
Command-line flags — `--locale`, `--diff OLD NEW`, `vix -` to read stdin,
`--version --json` — are in [`docs/cli/index.md`](docs/cli/index.md).

## FAQ

**Do I have to learn modal editing?** No. The default keymap uses the same
shortcuts as your other apps. Vi and Spacemacs keymaps are one menu choice
away if you want them.

**Do I need plugins to get started?** No. Everything listed on this page
ships in the one `vix` binary. Rhai scripts are there for adding your own
commands, not for filling gaps.

**Does it work over SSH and in tmux?** Yes — Vix is a terminal program, so
run it on the remote machine as you would any other. Or keep Vix local and
open the remote folder over SSH (`vix ssh://host/path`); that is
[experimental](docs/remote-editing/index.md).

**Can my company use it?** Pick whichever of the five licenses suits you.
For locked-down machines, the minimal build makes no network connections
and starts no other program.

**Is it stable?** Vix is young and moving fast; read the
[changelog](CHANGELOG.md) before upgrading across a major version. The
editor itself is well tested — every crate has a spec, and CI runs the
test suite, clippy pedantic and a docs check on every change.

**How fast is it, and how much memory does it use?** On one Apple M4 Max,
Vix shows a 10,000-line Rust file in about 40 ms using about 33 MiB. That is
behind Kakoune, mg, nano, Vim and Neovim (8–32 ms, 2.5–25 MiB), level with
micro (46 ms, 28 MiB), and ahead of Emacs and Helix (189–197 ms, 54–58
MiB). A 100 MB log opens in about 87 ms holding 126 MiB, about 1.25× the
file: second to Vim (117 MiB), level with Kakoune and Neovim (130 MiB),
well under Helix (525 MiB); with no input it uses no CPU. For files too big
for a buffer, use the Large File Viewer. See the
[method, machine and full table](docs/performance/index.md#startup-and-memory-vs-other-editors-t744).

## Build from source

Needs a Rust toolchain, 1.97 or newer.

```sh
git clone https://github.com/vixide/vix.git
cd vix
cargo run                                            # open the editor here
cargo build --release                                # binary at target/release/vix
cargo build --release --features syntax-all          # with every Tree-sitter grammar
cargo build --profile minimal --no-default-features  # the minimal build
```

Tree-sitter grammars are compiled C parsers, so the default build includes a
lean set (Rust, Markdown, JSON, TOML); see
[`spec/cargo-features/index.md`](spec/cargo-features/index.md) for every
feature.

## Contributing

Vix is a Cargo workspace: a thin app shell (the `vix-tui` package in `tui/`)
over about 147 focused `vix-*` crates in `crates/`, with the desktop GUIs in
`gui/`. Development is specification-driven — each crate's spec at
`crates/<crate>/spec/index.md` is the source of truth. Start with
[`AGENTS.md`](AGENTS.md) (conventions, workflow and crate map, for humans and
AI agents alike) and
[`docs/contributing-workflow.md`](docs/contributing-workflow.md). The roadmap
lives in [`plan.md`](plan.md) and [`tasks.md`](tasks.md).

```sh
cargo test
cargo clippy --workspace --all-targets -- -D warnings
scripts/check    # the full local gate that CI runs (--tier fast|push for quicker iteration)
```

Runnable examples live in [`tui/examples/`](tui/examples/) — for instance
`cargo run --example headless_edit` (open, edit and save a file with no UI)
and `cargo run --example list_commands` (print every command). The demo GIFs
are [VHS](https://github.com/charmbracelet/vhs) scripts in
[`docs/demos/`](docs/demos/), rendered by `scripts/render-demos.sh`; the
per-theme screenshots in [`docs/demos/themes/`](docs/demos/themes/) and the
website's social card are captured from the real `vix` binary by
`scripts/render-theme-gallery.py`.

If Vix is useful to you, a ⭐ on [GitHub](https://github.com/vixide/vix)
helps other people find it.

## License

Vix's source is licensed under any of Apache-2.0, BSD-3-Clause, MIT,
GPL-2.0-only, or GPL-3.0-only, at your option; each distributed binary
declares the subset its linked dependencies allow. See [`LICENSE`](LICENSE).

## Security

Found a vulnerability? See [`SECURITY.md`](SECURITY.md) for how to
report it privately, rather than opening a public issue.

## AI Statement

Vix is developed with substantial AI assistance. See
[`AI_STATEMENT.md`](AI_STATEMENT.md) for what that means: how AI-authored
work is disclosed, what bar it has to clear, and what an AI agent is (and
isn't) authorized to do here without asking first — currently `cargo
publish` and judging that a specific release is ready, nothing beyond that.

---

Vix™ and Vix IDE™ are trademarks.
