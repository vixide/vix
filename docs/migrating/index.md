# Migrating from another editor

How to bring your keys, settings, language servers, snippets, themes and
tasks to Vix from VS Code, Neovim or Vim, Emacs, Helix, Sublime Text, a
JetBrains IDE, Eclipse, Zed, Atom or Notepad++. Most of it is a keymap choice and a few lines of TOML.
Vix does not import other editors' configuration files, so this page shows
what each setting becomes.

For what Vix does differently from your old editor, and where your old
editor is still better, see the "Vix for … users" pages listed at the end.

## Where Vix keeps its configuration

| What | Where |
| --- | --- |
| Settings | `config.toml`: `~/.config/vix/` on Linux, `~/Library/Application Support/rs.vix/` on macOS, `%APPDATA%\vix\config\` on Windows. **Vix → Settings…** opens it. |
| Project settings | `<project>/.vix/settings.toml`, the same format, read only after you trust the workspace |
| Key overrides | `keybindings.toml` in the same directory as `config.toml`, or **Vix → Keybindings…** |
| Themes | `themes/*.json` in the same directory |
| Snippets | `global/snippets/*.json`, and `media-types/<type>/<subtype>/snippets/*.json` per language |
| Scripts | `scripts/*.rhai`, and `<project>/.vix/scripts/` |
| Tasks | `<project>/tasks.toml` or `<project>/.vix/tasks.toml` |

Settings apply in layers: defaults, then your `config.toml`, then the
project's `.vix/settings.toml`, then its `[languages.<id>]` tables. Vix
also reads `.editorconfig` files (indent style and size, tab width, trailing
whitespace, final newline), and a file's own Vim modelines and Emacs file
variables (`vim: set sw=2 et:`, `-*- indent-tabs-mode: nil -*-`; see
[Modelines](../modelines/index.md)), so files that already carry them need
nothing more. **Vix → Reload Settings** applies an edited file without a restart.
Every setting is listed in [Configuration](../configuration/index.md).

## 1. Choose your keys

Open **View → Keymap…** and pick the one you know. The choice is saved as
the `keymap` setting.

| Coming from | Keymap | `keymap =` |
| --- | --- | --- |
| VS Code | VSCode macOS or VSCode Windows | `"vscode-macos"`, `"vscode-windows"` |
| Neovim, Vim | Vi | `"vi"` |
| Emacs | Emacs | `"emacs"` |
| Spacemacs, Doom Emacs | Spacemacs | `"spacemacs"` |
| Helix | Helix | `"helix"` |
| IntelliJ, other JetBrains IDEs | IntelliJ macOS or IntelliJ Windows | `"intellij-macos"`, `"intellij-windows"` |
| Eclipse | Eclipse | `"eclipse"` |
| Sublime Text | Sublime | `"sublime"` |
| Zed | Zed | `"zed"` |
| Atom (no Atom keymap) | VSCode macOS or VSCode Windows | `"vscode-macos"`, `"vscode-windows"` |
| Notepad++ (no Notepad++ keymap) | Sublime | `"sublime"` |
| Anything else | Apple (the default) | `"apple"` |

To change a single key, add an override to `keybindings.toml`, or use
**Vix → Keybindings…**:

```toml
[[binding]]
key_token = "C-S-k"
action_id = "edit.duplicate_line"
```

Then run **Tools → Reload Keybindings**. Action ids are listed in the
[action reference](../reference/actions.md), and the full key syntax is in
[Keybindings](../keybindings/index.md). `F1` shows every key in the active
keymap.

## 2. Copy your settings

The usual ones, by editor. Put them in `config.toml`.

| Setting | VS Code | Neovim, Vim | Emacs | Vix |
| --- | --- | --- | --- | --- |
| Tab width | `editor.tabSize` | `tabstop`, `shiftwidth` | `tab-width` | `tab_width = 4` |
| Spaces or tabs | `editor.insertSpaces` | `expandtab` | `indent-tabs-mode` | `indent_style = "spaces"` (or `"tabs"`) |
| Line numbers | `editor.lineNumbers` | `number` | `display-line-numbers-mode` | `line_numbers = true` |
| Relative numbers | `editor.lineNumbers: "relative"` | `relativenumber` | `display-line-numbers-type 'relative` | `relative_line_numbers = true` |
| Soft wrap | `editor.wordWrap` | `wrap` | `visual-line-mode` | `soft_wrap = true` |
| Format on save | `editor.formatOnSave` | a plugin or autocommand | a hook | `format_on_save = true` |
| Auto save | `files.autoSave` | a plugin or autocommand | `auto-save-visited-mode` | `auto_save = "focus_lost"` (or `"interval"`) |
| Persistent undo | not built in | `undofile` | a package | `persistent_undo = true` (the default) |
| Reopen last files | `window.restoreWindows` | sessions | `desktop-save-mode` | `restore_session = true` |
| Theme | `workbench.colorTheme` | `colorscheme` | `load-theme` | `theme = "Nord"` |

Helix users: `editor.line-number = "relative"` becomes
`relative_line_numbers = true`, and `editor.soft-wrap.enable` becomes
`soft_wrap = true`. A per-language setting, like a language block in
`languages.toml` or VS Code's `"[python]"`, goes in a `[languages.<id>]` table
in `.vix/settings.toml`.

Font, font size and ligatures belong to your terminal, not to Vix.

## 3. Language servers

If the usual server for your language is installed (`rust-analyzer`,
`gopls`, `pyright` or `pylsp`, `typescript-language-server`, `clangd`, …),
Vix starts it once you trust the workspace: the first time you open a
matching file, the trust prompt names the server, and **y** starts it. To use
another server, list it in `config.toml`; your entry replaces the built-in
for its extensions:

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

A server starts the first time you open a matching file. Install the server
itself with your package manager (the same binaries your old editor used:
`rust-analyzer`, `pyright` or `pylsp`, `gopls`, `typescript-language-server`,
…). Run `vix --doctor` to check that every configured server starts. See
[Language servers](../language-server-protocol/index.md).

## 4. Debugging

Vix does not read VS Code's `launch.json` or nvim-dap configurations. Each
debug adapter is listed in `config.toml`, with the `launch` request's
arguments beside it:

```toml
[[debug_adapters]]
adapter_id = "debugpy"
extensions = ["py"]
command = ["python", "-m", "debugpy.adapter"]
[debug_adapters.launch]
console = "internalConsole"
```

`{program}` in a value becomes the current file. Start and attach from the
**Run** menu. See [tutorial 10](../tutorials/10-debugging-with-dap/index.md).

## 5. Snippets

Vix snippets use the VS Code JSON shape (`prefix`, `body`, `description`), so
most VS Code snippet files work as they are:

1. Copy a language's snippet file into
   `media-types/<type>/<subtype>/snippets/`, for example
   `<config>/media-types/text/rust/snippets/rust.json`, or into `global/snippets/`
   for snippets you want everywhere.
2. Rename a `.code-snippets` file to `.json`: only `.json` files are read.
3. Remove any comments; the file must be plain JSON.

Tab stops and placeholders (`$1`, `${1:name}`, `$0`) work. Variables such as
`$TM_FILENAME` are inserted as written, not expanded. UltiSnips, LuaSnip and
yasnippet files need converting by hand. Insert a snippet by typing its
prefix and `Tab`, or from **Tools → Snippets…**. See
[Snippets](../snippets/index.md).

## 6. Themes

Vix bundles 21 themes, including Solarized, Dracula, Nord, Gruvbox, Monokai,
One Dark, Tokyo Night, Catppuccin Mocha and a high-contrast theme, plus the
base16 schemes. Pick one from **View → Theme**. VS Code, TextMate and Vim
color schemes are not imported; to match one, start from the closest bundled
theme in **View → Edit Theme…**, which saves your version to `themes/`. See
[Themes](../themes/index.md).

## 7. Tasks and builds

Vix finds tasks without configuration in `package.json`, `deno.json`,
`composer.json`, a `justfile`, `Taskfile.yml`, a `Rakefile` and a
`Makefile`. VS Code's `tasks.json` is not read: add anything else to
`tasks.toml` in the project. The **Project** menu's configure, compile,
test, install, package and run commands can be fixed for a team in
`.vix/project.toml`. See [Tasks](../tasks/index.md).

## 8. Plugins and scripts

There is no plugin marketplace. Much of what you used plugins for is built
in: LSP, a debugger, Git with hunk staging, fuzzy finding, multiple cursors,
a terminal, a file explorer, Org mode, a database client and a REST client.
For the rest, Rhai scripts in `scripts/` add commands and key bindings
without a rebuild. A project's own `.vix/scripts/` run only after you trust
the workspace. See [Scripting](../scripting/index.md).

## What does not come across

- Extensions and plugins, and any configuration written in Lua, Vimscript,
  Emacs Lisp, Python or JavaScript.
- VS Code's `settings.json`, `keybindings.json`, `tasks.json` and
  `launch.json`. Copy the settings by hand (step 2).
- Color schemes from other editors (step 6).
- Snippet variables and other editors' snippet formats (step 5).

## Editor by editor

What is different, and what your old editor still does better:

- [VS Code](../vix-for-visual-studio-code-users/index.md)
- [Vim and Neovim](../vix-for-vim-users/index.md)
- [Emacs](../vix-for-emacs-users/index.md) and
  [Spacemacs](../vix-for-spacemacs-users/index.md)
- [Helix](../vix-for-helix-users/index.md)
- [IntelliJ and other JetBrains IDEs](../vix-for-intellij-users/index.md)
- [Eclipse](../vix-for-eclipse-users/index.md)
- [Sublime Text](../vix-for-sublime-users/index.md)
- [Zed](../vix-for-zed-users/index.md)
- [Atom](../vix-for-atom-users/index.md), which is discontinued
- [Notepad++](../vix-for-notepad-plus-plus-users/index.md)
- [Fresh](../vix-for-fresh-users/index.md) and [ttt](../vix-for-ttt-users/index.md)

Side-by-side comparisons: [VS Code](../compare/vix-vs-visual-studio-code/index.md),
[Neovim](../compare/vix-vs-neovim/index.md) and
[Helix](../compare/vix-vs-helix/index.md).

---

Vix™ and Vix IDE™ are trademarks.
