# Vix™ for Atom users

Atom is discontinued. GitHub
[announced its sunset](https://github.blog/2022-06-08-sunsetting-atom/) in
June 2022 and archived the project and its
[repository](https://github.com/atom/atom) that December.
If you are still on Atom, or on a fork of it, this page is for moving to Vix:
which keymap to pick, where each Atom feature you used lives in Vix, and what
does not come across.

Claims about Atom describe the archived project as it was documented. Every
claim about Vix cites the test or file that checks it.

## Pick a keymap

Vix has no Atom keymap. The closest are the **VSCode macOS** and **VSCode
Windows** keymaps (the two bind the same keys in a terminal): pick one from
**View → Keymap…**, or set `keymap = "vscode-macos"` in
`config.toml`.[^keymap-menu] If your fingers know Sublime Text's keys better,
the [Sublime keymap](../vix-for-sublime-users/index.md) is the other choice.
`F1` lists every key the active keymap binds,[^f1] and any key can be
rebound in `keybindings.toml`.[^override]

These rows are copied from the generated
[VS Code keybindings reference](../reference/keybindings-vscode-macos.md); a
test fails if a row here stops matching it. Compare each one with the key you
remember from Atom before relying on it.

<!-- keys-from: ../reference/keybindings-vscode-macos.md -->

| Key | Action | Action ID |
| --- | --- | --- |
| `C-p` | Open… | `file.open` |
| `C-S-p` | Command Palette | `tools.palette` |
| `C-b` | Toggle Explorer | `view.explorer` |
| `C-S-e` | Toggle Explorer Focus | `view.toggle_explorer_focus` |
| `C-f` | Find… | `edit.find` |
| `C-r` | Find & Replace | `edit.replace` |
| `C-S-f` | Search in Workspace | `search.workspace` |
| `C-/` | Comment | `edit.toggle_comment` |
| `C-S-l` | Select All Occurrences | `edit.select_all_occurrences` |
| `C-S-k` | Cut Line | `cut_line` |
| `C-\` | Vertical | `view.split_vertical` |
| `C-`` | Show/Hide Terminal | `tools.terminal` |
| `C-S-m` | Diagnostics… | `lsp.diagnostics` |
| `C-s` | Save | `file.save` |
| `C-S-s` | Save As… | `file.save_as` |
| `C-w` | Close | `file.close` |
| `C-S-t` | Reopen Closed Tab | `file.reopen_closed` |

In this keymap `Ctrl+D` adds the next occurrence of the word as another
caret, one press at a time.[^ctrl-d]

## Where your Atom features went

Atom shipped most of its features as bundled packages (the fuzzy finder,
command palette, tree view, find and replace, and the GitHub package among
them) and took the rest from its package registry, which was archived with
the editor. In Vix the equivalents are built in:

| In Atom | In Vix |
| --- | --- |
| Fuzzy finder | The file finder (`Ctrl+P` above) and the command palette, with `@` for symbols, `:` for a line and `>` for commands[^palette] |
| Tree view | The file explorer (`Ctrl+B` above) |
| Find and replace, project find | Find, replace and workspace search and replace[^search] |
| The GitHub package's Git panel | The Git panel: stage, commit and blame[^git] |
| Multiple cursors | Select all occurrences, add the next one, column selection[^multi] |
| Language packages | Tree-sitter highlighting[^tree-sitter] and language servers[^lsp] |
| Terminal packages | Integrated terminals[^terminal] |
| The Teletype package | Cowork, opt-in and off in a default build[^cowork] |
| `init` script and your own packages | Rhai scripts and plugins[^script] |
| Themes | Bundled themes, picked from **View → Theme**[^themes] |
| `config.cson` | `config.toml`; see [Configuration](../configuration/index.md) |

Vix also has a debugger for any Debug Adapter Protocol adapter,[^dap] and
persistent, branching undo.[^undo]

## What does not come across

- **Packages.** Atom packages were JavaScript or CoffeeScript on Electron; none
  of them run in Vix. Rhai scripts and plugins are a smaller, sandboxed
  extension system.
- **Settings, keymaps, snippets and styles** are not imported. Copy settings
  by hand (see [Migrating](../migrating/index.md)); Atom's `.cson` snippets
  need converting to Vix's JSON snippets (see [Snippets](../snippets/index.md)).
- **A desktop window.** Vix runs in a terminal; its desktop front ends are
  experimental.

## See also

- [Migrating from another editor](../migrating/index.md).
- [Vix for VS Code users](../vix-for-visual-studio-code-users/index.md) and
  [Vix compared with VS Code](../compare/vix-vs-visual-studio-code/index.md).
- [Keymaps](../keymaps/index.md).

[^keymap-menu]: **View → Keymap…** sets the `keymap` setting: `tui/tests/integration/keymaps.rs`, `view_keymap_submenu_actions_set_the_keymap`.
[^f1]: `F1` lists the active keymap's keys: `tui/tests/integration/keymaps.rs`, `help_overlay_includes_the_active_keymap_chords`.
[^override]: A `keybindings.toml` entry wins over the active keymap: `tui/tests/integration/keymaps.rs`, `key_override_wins_over_a_builtin_keymap_binding`.
[^ctrl-d]: `tui/tests/integration/editing.rs`, `ctrl_d_adds_a_caret_and_edits_all_occurrences` (VSCode macOS keymap).
[^palette]: The prefixes are `Mode::from_input` in `crates/vix-palette/src/lib.rs`.
[^search]: `tui/tests/integration/workspace.rs`, `workspace_search_finds_matches_across_files` and `workspace_replace_rewrites_files`.
[^git]: Stage and commit, blame: `tui/tests/integration/git.rs`, `git_panel_stages_and_commits` and `git_blame_annotates_the_current_line`.
[^multi]: `tui/tests/integration/editing.rs`, `select_all_occurrences_creates_multi_carets`, `ctrl_d_adds_a_caret_and_edits_all_occurrences` and `column_select_block_edits_multiple_lines`.
[^tree-sitter]: The grammars compiled in by default are the `syntax-common` feature in `tui/Cargo.toml`.
[^lsp]: Every LSP action against a mock server: `tui/tests/integration/lsp.rs`, `every_lsp_action_applies_a_mock_servers_reply`; servers start only in a trusted workspace: `a_builtin_language_server_starts_only_in_a_trusted_workspace`.
[^terminal]: `tui/tests/integration/terminals.rs`, `several_terminals_each_get_their_own_keys`.
[^cowork]: Compiled in only with the `cowork` Cargo feature, which the default `full` feature set in `tui/Cargo.toml` leaves out. See the [Cowork specification](../../spec/cowork/index.md).
[^script]: Rhai scripts add palette commands and key bindings: `tui/tests/integration/scripting.rs`, `script_command_appears_in_the_command_palette` and `a_scripts_bind_key_request_actually_fires_through_on_key`; plugins are enabled and disabled from a chooser: `plugin_chooser_enables_and_disables_a_plugin`.
[^themes]: `tui/tests/integration/menu.rs`, `view_theme_submenu_lists_bundled_themes`.
[^dap]: Breakpoints and the debugger actions: `tui/tests/integration/debugger.rs`, `edit_breakpoint_sets_condition_hit_count_and_log_message`.
[^undo]: `persistent_undo` defaults to `true` in `crates/vix-settings/src/lib.rs`; the tree view: `tui/tests/integration/undo_tree.rs`, `moving_the_selection_previews_and_enter_keeps_the_state`.

---

Vix™ and Vix IDE™ are trademarks.
