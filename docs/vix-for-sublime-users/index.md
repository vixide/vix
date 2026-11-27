# Vix™ for Sublime Text users

Sublime Text is a fast, polished desktop editor with Goto Anything, multiple
selections and a Python plugin API. Vix is a terminal editor with a language
server client, a debugger and a Git client built in. This page shows which of
your Sublime keys work in Vix's **Sublime** keymap, what is different, and
where Sublime Text is still the better tool.

Claims about Sublime Text describe its documented defaults; see the
[Sublime Text documentation](https://www.sublimetext.com/docs/). Every claim
about Vix cites the test or file that checks it.

## Turn on the Sublime keymap

Pick **View → Keymap… → Sublime**, or set `keymap = "sublime"` in
`config.toml`.[^keymap-menu] In a terminal, `Ctrl` stands in for `Cmd` on
macOS (see [Keymaps](../keymaps/index.md)). `F1` lists every key the active
keymap binds.[^f1]

## Your keys

These rows are copied from the generated
[Sublime keybindings reference](../reference/keybindings-sublime.md); a test
fails if a row here stops matching it. The keys every keymap shares (`F2`
rename, `F12` go to definition, `Ctrl+Space` completion and more) are in the
[shared bindings](../reference/keybindings-shared.md).

<!-- keys-from: ../reference/keybindings-sublime.md -->

| Key | Action | Action ID |
| --- | --- | --- |
| `C-p` | Open… | `file.open` |
| `C-S-p` | Command Palette | `tools.palette` |
| `C-r` | Go to Symbol | `nav.goto_symbol` |
| `C-g` | Number | `nav.goto_line` |
| `C-d` | Select All Occurrences | `edit.select_all_occurrences` |
| `C-l` | Select Line | `edit.select_line` |
| `C-j` | Join Lines | `edit.join_lines` |
| `C-m` | Jump to Matching Bracket | `edit.match_bracket` |
| `C-S-d` | Duplicate Line | `edit.duplicate_line` |
| `C-S-k` | Cut Line | `cut_line` |
| `C-/` | Comment | `edit.toggle_comment` |
| `C-f` | Find… | `edit.find` |
| `C-h` | Find & Replace | `edit.replace` |
| `C-S-f` | Search in Workspace | `search.workspace` |
| `C-b` | Run Tests | `tools.test` |
| `C-`` | Show/Hide Terminal | `tools.terminal` |
| `C-s` | Save | `file.save` |
| `C-S-s` | Save As… | `file.save_as` |
| `C-w` | Close | `file.close` |
| `C-S-t` | Reopen Closed Tab | `file.reopen_closed` |
| `C-S-w` | Close All Tabs | `file.close_all` |

## What is different

- **`Ctrl+P` opens the file finder, and `Ctrl+Shift+P` the command palette.**
  The palette takes Goto Anything's prefixes: `@` for a symbol in the file,
  `:` for a line, `>` for a command, plus `#` for an open buffer and `@@`
  for a symbol anywhere in the workspace.[^palette]
- **`Ctrl+D` selects every occurrence at once.** Sublime Text's `Ctrl+D`
  adds the next occurrence one at a time
  ([multiple selection with the keyboard](https://www.sublimetext.com/docs/multiple_selection_with_the_keyboard.html)).
  For that behavior, bind the key to `spawn_multi_cursor` (add the next
  occurrence as a caret) in `keybindings.toml`:[^override]

  ```toml
  [[binding]]
  key_token = "C-d"
  action_id = "spawn_multi_cursor"
  ```

- **`Ctrl+Shift+K` cuts the line to the clipboard** (`cut_line`) rather than
  only deleting it.
- **`Ctrl+B` runs the project's tests** (`tools.test`); there are no Sublime
  build systems. Vix finds tasks in `Makefile`, `package.json`, a
  `justfile` and more (see [Tasks](../tasks/index.md)).
- **Column selection** is **Edit → Select → Column Select Up/Down**
  (`Alt+Shift+Up` / `Alt+Shift+Down` in the default keymap), and typing
  then edits every line of the block.[^multi]

## What Vix adds

- A language server client (diagnostics, completion, rename, references, code
  actions) that starts the usual server for a language once you trust the
  workspace.[^lsp] Sublime Text gets this from the LSP package.
- A debugger for any Debug Adapter Protocol adapter.[^dap]
- A Git panel that stages and commits, and blame.[^git]
- Integrated terminals.[^terminal]
- Persistent, branching undo with a tree you can browse.[^undo]
- Search and replace across the workspace, with results you can edit in
  place.[^search]
- It runs in a terminal, so it works over SSH and in tmux; remote folders
  open with **File → Open Remote…**.[^remote]

## Where Sublime Text is the better choice

- **A native desktop window.** Smooth scrolling, font rendering and the
  minimap of a GPU-drawn window; Vix draws a character grid.
- **Packages.** Sublime Text's Python API and its package ecosystem are far
  larger than Vix's Rhai scripts and plugins.[^script]
- **Maturity.** Vix is young.

## See also

- [Migrating from another editor](../migrating/index.md): settings, language
  servers, snippets and themes.
- [Keymaps](../keymaps/index.md) and the
  [Sublime keybindings reference](../reference/keybindings-sublime.md).
- [Comparison with other editors](../compare/index.md).

[^keymap-menu]: **View → Keymap…** sets the `keymap` setting: `tui/tests/integration/keymaps.rs`, `view_keymap_submenu_actions_set_the_keymap`.
[^f1]: `F1` lists the active keymap's keys: `tui/tests/integration/keymaps.rs`, `help_overlay_includes_the_active_keymap_chords`.
[^palette]: The prefixes are `Mode::from_input` in `crates/vix-palette/src/lib.rs`.
[^override]: A `keybindings.toml` entry wins over the active keymap: `tui/tests/integration/keymaps.rs`, `key_override_wins_over_a_builtin_keymap_binding`.
[^multi]: `tui/tests/integration/editing.rs`, `select_all_occurrences_creates_multi_carets`, `ctrl_d_adds_a_caret_and_edits_all_occurrences` and `column_select_block_edits_multiple_lines`.
[^lsp]: Every LSP action against a mock server: `tui/tests/integration/lsp.rs`, `every_lsp_action_applies_a_mock_servers_reply`; servers start only in a trusted workspace: `a_builtin_language_server_starts_only_in_a_trusted_workspace`.
[^dap]: Breakpoints and the debugger actions: `tui/tests/integration/debugger.rs`, `edit_breakpoint_sets_condition_hit_count_and_log_message`.
[^git]: Stage and commit, blame: `tui/tests/integration/git.rs`, `git_panel_stages_and_commits` and `git_blame_annotates_the_current_line`.
[^terminal]: `tui/tests/integration/terminals.rs`, `several_terminals_each_get_their_own_keys`.
[^undo]: `persistent_undo` defaults to `true` in `crates/vix-settings/src/lib.rs`; the tree view: `tui/tests/integration/undo_tree.rs`, `moving_the_selection_previews_and_enter_keeps_the_state`.
[^search]: `tui/tests/integration/workspace.rs`, `workspace_search_finds_matches_across_files` and `workspace_replace_rewrites_files`.
[^remote]: Experimental. `tui/tests/remote_ui.rs`, `open_remote_connects_saves_survives_a_lost_connection_and_reconnects`; see [remote editing](../remote-editing/index.md).
[^script]: Rhai scripts add palette commands and key bindings: `tui/tests/integration/scripting.rs`, `script_command_appears_in_the_command_palette` and `a_scripts_bind_key_request_actually_fires_through_on_key`; plugins are enabled and disabled from a chooser: `plugin_chooser_enables_and_disables_a_plugin`.

---

Vix™ and Vix IDE™ are trademarks.
