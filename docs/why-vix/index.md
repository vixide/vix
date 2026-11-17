# Why Vix?

Most people choosing an editor today are choosing between VS Code, Neovim, a
JetBrains IDE and Sublime Text. This page says where Vix fits among them:
what it does that they don't, what they do better, and who should pick which.
For Vim, Emacs, Helix, Zed, Fresh and ttt, see the
[full feature comparison](../compare/index.md).

## The short answer

- **An IDE in your terminal, with nothing to assemble.** Language servers, a
  debugger, Tree-sitter highlighting, a Git client, a test runner, tasks and
  an integrated terminal ship in one binary and work on first launch. In
  Neovim most of that is plugins you choose and configure; in VS Code it is
  extensions.
- **Ordinary keys by default.** `Ctrl+C` copies, `Ctrl+S` saves, and there is
  a menu bar. Fourteen keymaps (VS Code, IntelliJ, Sublime, Vi, Helix, Emacs,
  Spacemacs and more) are one menu choice away: **View → Keymap…**.
- **It goes where a terminal goes.** SSH, tmux, a container, a Raspberry Pi:
  the whole IDE runs there, with no server component to install and no GUI.
- **Things the others leave to extensions.** An Org-mode suite with an
  agenda, tables and Org-roam; a SQLite, PostgreSQL and MySQL workbench; a
  REST client for `.http` files; a Jujutsu client; a persistent, branching
  undo tree.
- **Private and careful by default.** No telemetry, no accounts, no update
  check. No `unsafe` code. Scripts are sandboxed, and a project's own
  scripts run only after you trust the workspace. See
  [`SECURITY.md`](../../SECURITY.md).
- **Free and open source,** under your choice of Apache-2.0, BSD-3-Clause,
  MIT, GPL-2.0 or GPL-3.0.

## Compared with VS Code, Neovim, JetBrains and Sublime

`✓` built in, no extension or plugin needed. `-` not built in (an extension
or plugin may exist). `~` partial: see the footnote. The other editors are
marked from their own documentation as of November 2026; they change fast,
so check anything that matters to you.

| | Vix | VS Code | Neovim | JetBrains IDEs | Sublime Text |
| --- | :---: | :---: | :---: | :---: | :---: |
| Runs in a terminal, over SSH | ✓ | -[^vscode-remote] | ✓ | -[^jb-remote] | - |
| Desktop window | ~[^vix-gui] | ✓ | ~[^nvim-gui] | ✓ | ✓ |
| Familiar keys and a menu bar by default | ✓ | ✓ | - | ✓ | ✓ |
| Modal (Vim-style) editing | ✓[^vix-modal] | -[^ext] | ✓ | -[^idea-vim] | ~[^vintage] |
| Language servers (LSP) | ✓ | ~[^vscode-lsp] | ✓[^nvim-lsp] | ~[^jb-lsp] | -[^st-lsp] |
| Debugger | ✓ | ✓ | -[^nvim-dap] | ✓ | - |
| Tree-sitter highlighting | ✓ | -[^textmate] | ✓ | -[^jb-hl] | -[^st-syntax] |
| Fuzzy file finder and command palette | ✓ | ✓ | -[^nvim-fuzzy] | ✓ | ✓ |
| Multiple cursors | ✓ | ✓ | - | ✓ | ✓ |
| Git client (stage, commit, log, blame) | ✓ | ✓ | - | ✓ | ~[^st-git] |
| Jujutsu client | ✓ | - | - | - | - |
| Integrated terminal | ✓ | ✓ | ✓ | ✓ | - |
| Database workbench | ✓ | - | - | ~[^jb-db] | - |
| HTTP client for `.http` files | ✓ | - | - | ~[^jb-db] | - |
| Org mode (outline, TODO, agenda) | ✓ | - | - | - | - |
| Persistent, branching undo | ✓ | - | ~[^nvim-undo] | - | - |
| Scripting | Rhai, sandboxed | JS/TS extensions | Lua | Java/Kotlin plugins | Python plugins |
| Price | Free | Free[^vscode-license] | Free | ~[^jb-price] | Paid[^st-price] |

[^vscode-remote]: Remote-SSH, a Microsoft extension, installs a VS Code
    server on the remote machine and drives it from the desktop app. VS Code
    itself does not run in a terminal.
[^jb-remote]: JetBrains Gateway runs the IDE backend on a remote machine
    behind a desktop client.
[^vix-gui]: Seven experimental desktop front ends run the same editor in a
    native window. See [`gui/spec`](../../gui/spec/index.md).
[^nvim-gui]: Through third-party front ends such as Neovide.
[^vix-modal]: Off by default. The **Vi**, **Spacemacs** and **Helix**
    keymaps are a menu choice away. See [keymaps](../keymaps/index.md).
[^ext]: Through extensions such as VSCodeVim.
[^idea-vim]: Through the IdeaVim plugin, maintained by JetBrains.
[^vintage]: Vintage mode ships with Sublime Text but is off by default.
[^vscode-lsp]: VS Code speaks LSP to language servers, but each language's
    support comes from an extension; a few (TypeScript, JSON, HTML, CSS) are
    bundled.
[^nvim-lsp]: The client is built in; you install and configure each server
    yourself.
[^jb-lsp]: JetBrains IDEs analyze code with their own engines, which go
    deeper than most language servers for the languages they target.
[^st-lsp]: Through the LSP package.
[^nvim-dap]: Through plugins such as nvim-dap.
[^textmate]: VS Code highlights with TextMate grammars.
[^jb-hl]: Highlighting comes from the IDE's own parsers.
[^st-syntax]: Sublime Text uses its own syntax definitions.
[^nvim-fuzzy]: Command-line completion is built in; fuzzy pickers are
    plugins such as Telescope or fzf-lua.
[^st-git]: A Git diff gutter is built in; staging and history are in Sublime
    Merge, a separate app.
[^jb-db]: In the paid editions (IntelliJ IDEA Ultimate and the database
    tools from DataGrip).
[^nvim-undo]: `undofile` keeps undo history across sessions and the history
    branches, but browsing the branches needs a plugin.
[^vscode-license]: The source is MIT-licensed; Microsoft's builds are under a
    product license and send telemetry by default (`telemetry.telemetryLevel`).
[^jb-price]: Community editions are free; Ultimate editions are paid.
[^st-price]: A paid license, with an unlimited evaluation.

## Where each one is the better choice

Vix does not win every row, and it is the wrong tool for some people.

- **VS Code**, when your work depends on a particular extension (Jupyter
  notebooks, Live Share, a proprietary language pack), or when you want rich
  graphical surfaces: webviews, image diffs, drag and drop. Its marketplace
  covers needs Vix never will. See
  [Vix for VS Code users](../vix-for-visual-studio-code-users/index.md).
- **Neovim**, when you want to build your own editor. Lua reaches far deeper
  than Vix's Rhai scripts, and its plugin ecosystem is vast. It also starts
  faster and uses less memory (see below). See
  [Vix for Vim users](../vix-for-vim-users/index.md).
- **A JetBrains IDE**, for large Java, Kotlin or C# codebases, where its own
  code analysis and refactorings go further than a language server. See
  [Vix for IntelliJ users](../vix-for-intellij-users/index.md).
- **Sublime Text**, when you want a fast, polished desktop editor and little
  else. See [Vix for Sublime users](../vix-for-sublime-users/index.md).
- **Vix**, when you want a full IDE in a terminal, over SSH or in tmux, that
  works the day you install it; when you would rather not maintain an editor
  configuration; or when you want Org mode, a database client and a REST
  client next to your code.

Vix's own gaps: it is young, remote editing over SSH is experimental, its
desktop windows are experimental, and its scripting is far younger than Lua,
Python or the VS Code extension API.

## Speed and memory

Measured on one Apple M4 Max with each editor's configuration disabled (see
[the method and the full table](../performance/index.md)):

| | Vix | Neovim |
| --- | ---: | ---: |
| 10,000-line Rust file: time to first screen | 40 ms | 32 ms |
| 10,000-line Rust file: memory | 33 MiB | 25 MiB |
| 100 MB log file: time to first screen | 87 ms | 121 ms |
| 100 MB log file: memory | 126 MiB | 130 MiB |

Both use no CPU when idle. The table covers terminal editors only, so VS Code,
JetBrains IDEs and Sublime Text were not measured.

## Try it

```sh
brew install vixide/homebrew-tap/vix-tui    # or: cargo install vix-tui
vix .
```

Then pick the keys you already know from **View → Keymap…**, and see the
[migration guide](../migrating/index.md) for bringing your settings, keys,
themes and snippets across. [Getting Started](../getting-started/index.md)
has every install option.

## See also

- [Comparison with other editors](../compare/index.md): Vim, Emacs, Helix,
  Zed, Fresh and ttt.
- [Keymaps](../keymaps/index.md): every keymap and binding.
- [Performance](../performance/index.md): benchmarks and how they are run.

---

Vix™ and Vix IDE™ are trademarks.
