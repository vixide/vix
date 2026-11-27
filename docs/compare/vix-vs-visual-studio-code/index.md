# Vix™ compared with VS Code

VS Code is a desktop editor whose language support, debuggers and much else
come from extensions in its marketplace. Vix is a terminal editor with those
pieces built in and no marketplace. This page puts the two side by side, says
which to pick when, and how to switch. For the step-by-step move, see
[Vix for VS Code users](../../vix-for-visual-studio-code-users/index.md).

Claims about VS Code describe its documented defaults; see the
[VS Code documentation](https://code.visualstudio.com/docs). Every claim
about Vix cites the test or file that checks it. The wider matrix, with Vim,
Emacs, Helix, Zed, Fresh and ttt, is on the
[comparison page](../index.md); JetBrains IDEs, Neovim and Sublime Text are
in [Why Vix?](../../why-vix/index.md).

## At a glance

`✓` built in. `-` not built in (an extension may exist). `~` partial: see
the footnote.

| | Vix | VS Code |
| --- | :---: | :---: |
| Runs in a terminal | ✓ | - |
| Edits a remote folder over SSH | ~[^remote] | ~[^vscode-remote] |
| Desktop window | ~[^vix-gui] | ✓ |
| Language servers | ✓[^lsp] | ~[^vscode-lsp] |
| Debugger | ✓[^dap] | ✓ |
| Git: stage and commit | ✓[^git] | ✓ |
| Integrated terminal | ✓[^terminal] | ✓ |
| Multiple cursors | ✓[^multi] | ✓ |
| Persistent, branching undo | ✓[^undo] | - |
| Extension marketplace | - | ✓ |
| Scripting | Rhai scripts and plugins[^script] | JavaScript and TypeScript extensions |
| Live co-editing | ~[^cowork] | -[^live-share] |

## Choose VS Code when

- Your work depends on a particular extension: notebooks, a remote-container
  workflow, a proprietary language pack. The marketplace covers far more than
  Vix ever will.
- You want graphical surfaces: webviews, image diffs, drag and drop.
- You work mouse-first in a desktop window.

## Choose Vix when

- You work in a terminal, over SSH or in tmux, and want the same editor there
  as on your laptop, with nothing installed on the remote side.[^remote]
- You want the IDE pieces (language servers, a debugger, Git, a terminal) to
  work on first launch without choosing extensions.
- You would rather not maintain an extension set.

## Switching

Pick **View → Keymap… → VSCode macOS** or **VSCode Windows**.[^keymap-menu]
The rows below are copied from the generated
[VS Code keybindings reference](../../reference/keybindings-vscode-macos.md);
a test fails if a row here stops matching it.

<!-- keys-from: ../../reference/keybindings-vscode-macos.md -->

| Key | Action | Action ID |
| --- | --- | --- |
| `C-p` | Open… | `file.open` |
| `C-S-p` | Command Palette | `tools.palette` |
| `C-g` | Number | `nav.goto_line` |
| `C-S-o` | Go to Symbol | `nav.goto_symbol` |
| `C-t` | Go to Symbol in Workspace | `nav.goto_workspace_symbol` |
| `C-b` | Toggle Explorer | `view.explorer` |
| `C-j` | Show/Hide Bottom Dock | `view.bottom_dock` |
| `C-`` | Show/Hide Terminal | `tools.terminal` |
| `C-S-l` | Select All Occurrences | `edit.select_all_occurrences` |
| `C-\` | Vertical | `view.split_vertical` |
| `C-S-m` | Diagnostics… | `lsp.diagnostics` |

`Ctrl+D` adds the next occurrence as a caret, as in VS Code.[^ctrl-d] Then
bring your settings, snippets and language servers across with the
[migration guide](../../migrating/index.md).

[^vscode-remote]: Through Microsoft's Remote - SSH extension, which installs a
    VS Code server on the remote machine; see
    [Remote development over SSH](https://code.visualstudio.com/docs/remote/ssh).
[^vix-gui]: Seven experimental desktop front ends run the same editor in a
    native window. See [`gui/spec`](../../../gui/spec/index.md).
[^vscode-lsp]: VS Code speaks LSP, but each language's support comes from an
    extension; a few languages are bundled.
[^live-share]: Through Microsoft's Live Share extension.
[^ctrl-d]: `tui/tests/integration/editing.rs`, `ctrl_d_adds_a_caret_and_edits_all_occurrences`.
[^remote]: Experimental. `tui/tests/remote_ui.rs`, `open_remote_connects_saves_survives_a_lost_connection_and_reconnects`; see [remote editing](../../remote-editing/index.md).
[^lsp]: Every LSP action against a mock server: `tui/tests/integration/lsp.rs`, `every_lsp_action_applies_a_mock_servers_reply`; servers start only in a trusted workspace: `a_builtin_language_server_starts_only_in_a_trusted_workspace`.
[^dap]: Breakpoints and the debugger actions: `tui/tests/integration/debugger.rs`, `edit_breakpoint_sets_condition_hit_count_and_log_message`.
[^git]: Stage and commit, blame: `tui/tests/integration/git.rs`, `git_panel_stages_and_commits` and `git_blame_annotates_the_current_line`.
[^terminal]: `tui/tests/integration/terminals.rs`, `several_terminals_each_get_their_own_keys`.
[^multi]: `tui/tests/integration/editing.rs`, `select_all_occurrences_creates_multi_carets`, `ctrl_d_adds_a_caret_and_edits_all_occurrences` and `column_select_block_edits_multiple_lines`.
[^undo]: `persistent_undo` defaults to `true` in `crates/vix-settings/src/lib.rs`; the tree view: `tui/tests/integration/undo_tree.rs`, `moving_the_selection_previews_and_enter_keeps_the_state`.
[^script]: Rhai scripts add palette commands and key bindings: `tui/tests/integration/scripting.rs`, `script_command_appears_in_the_command_palette` and `a_scripts_bind_key_request_actually_fires_through_on_key`; plugins are enabled and disabled from a chooser: `plugin_chooser_enables_and_disables_a_plugin`.
[^cowork]: Compiled in only with the `cowork` Cargo feature, which the default `full` feature set in `tui/Cargo.toml` leaves out. See the [Cowork specification](../../../spec/cowork/index.md).
[^keymap-menu]: **View → Keymap…** sets the `keymap` setting: `tui/tests/integration/keymaps.rs`, `view_keymap_submenu_actions_set_the_keymap`.

---

Vix™ and Vix IDE™ are trademarks.
