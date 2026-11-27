# Vix™ for IntelliJ and JetBrains users

IntelliJ IDEA and the other JetBrains IDEs analyze code with their own engines
and offer deep refactorings, database tools and framework support. Vix is a
terminal editor that gets its language features from language servers and
ships a debugger, a Git client and a terminal in one binary. This page shows
which of your IntelliJ keys work in Vix's two **IntelliJ** keymaps, what is
different, and where a JetBrains IDE is still the better tool.

Claims about JetBrains IDEs describe their documented defaults; see the
[IntelliJ IDEA documentation](https://www.jetbrains.com/help/idea/). Every
claim about Vix cites the test or file that checks it.

## Turn on an IntelliJ keymap

Pick **View → Keymap… → IntelliJ macOS** or **IntelliJ Windows**, or set
`keymap = "intellij-macos"` or `"intellij-windows"` in
`config.toml`.[^keymap-menu] In a terminal, `Ctrl` stands in for `Cmd` on
macOS. `F1` lists every key the active keymap binds.[^f1]

## Your keys

These rows are copied from the generated keybindings references; a test fails
if a row here stops matching them. The keys every keymap shares (`F2`
rename, `F12` go to definition, `Ctrl+Space` completion, `Alt+Left` back)
are in the [shared bindings](../reference/keybindings-shared.md).

### IntelliJ macOS

From the [IntelliJ macOS reference](../reference/keybindings-intellij-macos.md):

<!-- keys-from: ../reference/keybindings-intellij-macos.md -->

| Key | Action | Action ID |
| --- | --- | --- |
| `C-S-a` | Command Palette | `tools.palette` |
| `C-o` | Go to Symbol | `nav.goto_symbol` |
| `C-S-o` | Open… | `file.open` |
| `C-A-o` | Go to Symbol in Workspace | `nav.goto_workspace_symbol` |
| `C-l` | Number | `nav.goto_line` |
| `C-b` | Go to Definition | `nav.goto_definition` |
| `C-f` | Find… | `edit.find` |
| `C-r` | Find & Replace | `edit.replace` |
| `C-g` | Find Next | `edit.find_next` |
| `C-S-g` | Find Previous | `edit.find_prev` |
| `C-S-f` | Search in Workspace | `search.workspace` |
| `C-S-r` | Search and Replace in Workspace | `search.workspace_replace` |
| `C-d` | Duplicate Line | `edit.duplicate_line` |
| `C-/` | Comment | `edit.toggle_comment` |
| `C-A-l` | Format Document | `lsp.format` |
| `C-,` | Settings… | `vix.settings` |
| `C-s` | Save | `file.save` |
| `C-S-s` | Save As… | `file.save_as` |
| `C-w` | Close | `file.close` |

### IntelliJ Windows

From the [IntelliJ Windows reference](../reference/keybindings-intellij-windows.md):

<!-- keys-from: ../reference/keybindings-intellij-windows.md -->

| Key | Action | Action ID |
| --- | --- | --- |
| `C-S-a` | Command Palette | `tools.palette` |
| `C-n` | Go to Symbol | `nav.goto_symbol` |
| `C-S-n` | Open… | `file.open` |
| `C-A-o` | Go to Symbol in Workspace | `nav.goto_workspace_symbol` |
| `C-g` | Number | `nav.goto_line` |
| `C-b` | Go to Definition | `nav.goto_definition` |
| `C-f` | Find… | `edit.find` |
| `C-r` | Find & Replace | `edit.replace` |
| `C-S-f` | Search in Workspace | `search.workspace` |
| `C-S-r` | Search and Replace in Workspace | `search.workspace_replace` |
| `C-d` | Duplicate Line | `edit.duplicate_line` |
| `C-/` | Comment | `edit.toggle_comment` |
| `C-A-l` | Format Document | `lsp.format` |
| `C-s` | Save | `file.save` |
| `C-S-s` | Save As… | `file.save_as` |
| `C-w` | Close | `file.close` |

The two keymaps differ in the "go to" family: `Ctrl+O` / `Ctrl+L` on macOS,
`Ctrl+N` / `Ctrl+G` on Windows.[^intellij-go-to]

## What is different

- **Language features come from language servers.** Completion, rename,
  references, code actions, call hierarchy and formatting work through LSP, and
  the usual server for a language starts once you trust the workspace.[^lsp]
  Install the server yourself (`rust-analyzer`, `jdtls`, `pyright`, …); see
  [Language servers](../language-server-protocol/index.md).
- **The debugger is menu-driven.** Breakpoints, conditions, hit counts and log
  messages are set from the **Run** menu, for any Debug Adapter Protocol
  adapter.[^dap]
- **Local history is the undo tree.** Undo history persists across sessions
  and branches, and **Edit → Undo Tree…** browses it.[^undo]
- **Settings are TOML**, in `config.toml` (`Ctrl+,` runs **Settings…** in the
  macOS keymap); see [Configuration](../configuration/index.md).

## What Vix adds

- It runs in a terminal, so it works over SSH and in tmux; remote folders
  open with **File → Open Remote…**.[^remote]
- A Git panel that stages and commits, and blame.[^git]
- Integrated terminals.[^terminal]
- Search and replace across the workspace, with results you can edit in
  place.[^search]

## Where a JetBrains IDE is the better choice

- **Code analysis and refactoring** for the languages it targets, especially
  Java and Kotlin, go further than a language server.
- **Framework, database and profiling tools**, which Vix does not attempt at
  the same depth.
- **A desktop GUI** with visual diff and merge, and graphical debugger views.

## See also

- [Migrating from another editor](../migrating/index.md): settings, language
  servers, snippets and themes.
- [Keymaps](../keymaps/index.md#intellij).
- [Why Vix?](../why-vix/index.md): Vix next to VS Code, Neovim, JetBrains IDEs
  and Sublime Text.

[^keymap-menu]: **View → Keymap…** sets the `keymap` setting: `tui/tests/integration/keymaps.rs`, `view_keymap_submenu_actions_set_the_keymap`.
[^f1]: `F1` lists the active keymap's keys: `tui/tests/integration/keymaps.rs`, `help_overlay_includes_the_active_keymap_chords`.
[^intellij-go-to]: `tui/tests/integration/keymaps.rs`, `intellij_go_to_family_differs_by_platform`.
[^lsp]: Every LSP action against a mock server: `tui/tests/integration/lsp.rs`, `every_lsp_action_applies_a_mock_servers_reply`; servers start only in a trusted workspace: `a_builtin_language_server_starts_only_in_a_trusted_workspace`.
[^dap]: Breakpoints and the debugger actions: `tui/tests/integration/debugger.rs`, `edit_breakpoint_sets_condition_hit_count_and_log_message`.
[^undo]: `persistent_undo` defaults to `true` in `crates/vix-settings/src/lib.rs`; the tree view: `tui/tests/integration/undo_tree.rs`, `moving_the_selection_previews_and_enter_keeps_the_state`.
[^remote]: Experimental. `tui/tests/remote_ui.rs`, `open_remote_connects_saves_survives_a_lost_connection_and_reconnects`; see [remote editing](../remote-editing/index.md).
[^git]: Stage and commit, blame: `tui/tests/integration/git.rs`, `git_panel_stages_and_commits` and `git_blame_annotates_the_current_line`.
[^terminal]: `tui/tests/integration/terminals.rs`, `several_terminals_each_get_their_own_keys`.
[^search]: `tui/tests/integration/workspace.rs`, `workspace_search_finds_matches_across_files` and `workspace_replace_rewrites_files`.

---

Vix™ and Vix IDE™ are trademarks.
