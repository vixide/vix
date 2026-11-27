# Vix™ for Eclipse users

Eclipse is a desktop IDE built around workspaces, perspectives and a large
plugin ecosystem, with compiler-backed Java tooling. Vix is a terminal editor
that gets its language features from language servers and ships a debugger, a
Git client and a terminal in one binary. This page shows which of your
Eclipse keys work in Vix's **Eclipse** keymap, what is different, and where
Eclipse is still the better tool.

Claims about Eclipse describe its documented defaults; see the
[Eclipse documentation](https://help.eclipse.org/). Every claim about Vix
cites the test or file that checks it.

## Turn on the Eclipse keymap

Pick **View → Keymap… → Eclipse**, or set `keymap = "eclipse"` in
`config.toml`.[^keymap-menu] The keymap follows Eclipse's Windows defaults.
`F1` lists every key the active keymap binds.[^f1]

## Your keys

These rows are copied from the generated
[Eclipse keybindings reference](../reference/keybindings-eclipse.md); a test
fails if a row here stops matching it. The keys every keymap shares (`F2`
rename, `F12` go to definition, `Ctrl+Space` completion, `Alt+Left` back)
are in the [shared bindings](../reference/keybindings-shared.md).

<!-- keys-from: ../reference/keybindings-eclipse.md -->

| Key | Action | Action ID |
| --- | --- | --- |
| `C-3` | Command Palette | `tools.palette` |
| `C-S-r` | Open… | `file.open` |
| `C-S-t` | Go to Symbol in Workspace | `nav.goto_workspace_symbol` |
| `C-o` | Go to Symbol | `nav.goto_symbol` |
| `C-l` | Number | `nav.goto_line` |
| `C-f` | Find… | `edit.find` |
| `C-r` | Find & Replace | `edit.replace` |
| `C-k` | Find Next | `edit.find_next` |
| `C-S-k` | Find Previous | `edit.find_prev` |
| `C-h` | Search in Workspace | `search.workspace` |
| `A-/` | Autocomplete Word | `autocomplete` |
| `C-/` | Comment | `edit.toggle_comment` |
| `C-S-f` | Format Document | `lsp.format` |
| `C-S-b` | Toggle Breakpoint | `run.toggle_breakpoint` |
| `C-b` | Run Tests | `tools.test` |
| `C-s` | Save | `file.save` |
| `C-S-s` | Save As… | `file.save_as` |
| `C-w` | Close | `file.close` |
| `C-S-w` | Close All Tabs | `file.close_all` |
| `C-y` | Redo | `edit.redo` |

## What is different

- **`Ctrl+3` is the command palette.** Inside it, `@` jumps to a symbol in
  the file, `:` to a line and `>` runs a command.[^palette]
- **`Alt+/` completes a word** from the buffer, separately from `Ctrl+/`,
  which toggles a comment.[^eclipse-alt-slash]
- **`Ctrl+B` runs the project's tests** (`tools.test`) rather than building
  a workspace. Vix finds tasks in `Makefile`, `package.json`, a `justfile`
  and more (see [Tasks](../tasks/index.md)).
- **No `.metadata` workspace.** A Vix workspace is a folder; project settings
  live in `.vix/settings.toml` (see [Migrating](../migrating/index.md)).
- **Language features come from language servers**, for Java from `jdtls`.
  The usual server for a language starts once you trust the workspace.[^lsp]

## What Vix adds

- It runs in a terminal, so it works over SSH and in tmux; remote folders
  open with **File → Open Remote…**.[^remote]
- A debugger for any Debug Adapter Protocol adapter; `Ctrl+Shift+B` toggles a
  breakpoint.[^dap]
- A Git panel that stages and commits, and blame.[^git]
- Integrated terminals.[^terminal]
- Persistent, branching undo with a tree you can browse.[^undo]

## Where Eclipse is the better choice

- **Java tooling.** Eclipse's own Java compiler and refactorings are richer
  than what a language server exposes.
- **Plugins** for modeling, embedded and vendor toolchains, which Vix does not
  attempt.
- **Perspectives and a desktop GUI**, including visual diff and merge.

## See also

- [Migrating from another editor](../migrating/index.md): settings, language
  servers, snippets and themes.
- [Keymaps](../keymaps/index.md#eclipse).
- [Why Vix?](../why-vix/index.md).

[^keymap-menu]: **View → Keymap…** sets the `keymap` setting: `tui/tests/integration/keymaps.rs`, `view_keymap_submenu_actions_set_the_keymap`.
[^f1]: `F1` lists the active keymap's keys: `tui/tests/integration/keymaps.rs`, `help_overlay_includes_the_active_keymap_chords`.
[^palette]: The prefixes are `Mode::from_input` in `crates/vix-palette/src/lib.rs`.
[^eclipse-alt-slash]: `tui/tests/integration/keymaps.rs`, `eclipse_keymap_alt_slash_completes_a_word_distinct_from_ctrl_slash`.
[^lsp]: Every LSP action against a mock server: `tui/tests/integration/lsp.rs`, `every_lsp_action_applies_a_mock_servers_reply`; servers start only in a trusted workspace: `a_builtin_language_server_starts_only_in_a_trusted_workspace`.
[^remote]: Experimental. `tui/tests/remote_ui.rs`, `open_remote_connects_saves_survives_a_lost_connection_and_reconnects`; see [remote editing](../remote-editing/index.md).
[^dap]: Breakpoints and the debugger actions: `tui/tests/integration/debugger.rs`, `edit_breakpoint_sets_condition_hit_count_and_log_message`.
[^git]: Stage and commit, blame: `tui/tests/integration/git.rs`, `git_panel_stages_and_commits` and `git_blame_annotates_the_current_line`.
[^terminal]: `tui/tests/integration/terminals.rs`, `several_terminals_each_get_their_own_keys`.
[^undo]: `persistent_undo` defaults to `true` in `crates/vix-settings/src/lib.rs`; the tree view: `tui/tests/integration/undo_tree.rs`, `moving_the_selection_previews_and_enter_keeps_the_state`.

---

Vix™ and Vix IDE™ are trademarks.
