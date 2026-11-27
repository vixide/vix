# Vix™ compared with Helix

Helix is a modal terminal editor, written in Rust, with selection-first
editing and language servers, Tree-sitter and multiple selections built in.
Vix shares the "built in, no plugins to assemble" idea but takes a wider scope
(a Git client, terminals, menus) and is not modal by default; its **Helix**
keymap brings the selection-first core along. This page puts the two side by
side, says which to pick when, and how to switch. For more, see
[Vix for Helix users](../../vix-for-helix-users/index.md).

Claims about Helix describe its documented defaults; see the
[Helix documentation](https://docs.helix-editor.com/). Every claim about Vix
cites the test or file that checks it. The wider matrix, with Vim, Emacs, Zed,
Fresh and ttt, is on the [comparison page](../index.md).

## At a glance

`✓` built in. `-` not built in. `~` partial: see the footnote.

| | Vix | Helix |
| --- | :---: | :---: |
| Selection-first modal editing | ~[^vix-helix] | ✓[^hx-keymap] |
| Modal by default | - | ✓ |
| Menu bar | ✓[^menu] | - |
| Language servers | ✓[^lsp] | ✓ |
| Tree-sitter highlighting | ✓[^tree-sitter] | ✓ |
| Multiple selections | ✓[^multi] | ✓ |
| Git: stage, commit, blame | ✓[^git] | ~[^hx-git] |
| Integrated terminal | ✓[^terminal] | - |
| Persistent, branching undo | ✓[^undo] | - |
| Scripting | Rhai scripts and plugins[^script] | -[^hx-plugins] |

## Choose Helix when

- You want the whole selection-first model: Vix's Helix keymap covers the
  core, but not named registers, view and window modes, or rotating and
  flipping selections.
- You prefer a smaller, deliberately focused editor.
- You want modal editing without switching anything on.

## Choose Vix when

- You want the Git client, terminals, a debugger[^dap] and browsable,
  persistent undo in the same binary.
- You want menus and a command palette for discovery, with modal editing
  optional.
- You want to script commands and key bindings today.

## Switching

Pick **View → Keymap… → Helix**, or set `keymap = "helix"` in
`config.toml`.[^keymap-menu] A few rows from the generated
[Helix keybindings reference](../../reference/keybindings-helix.md) (a test
fails if a row here stops matching it):

<!-- keys-from: ../../reference/keybindings-helix.md -->

| Key | Action | Action ID |
| --- | --- | --- |
| `w` | Select to Next Word | `helix.select_word_next` |
| `x` | Select Line | `helix.select_line` |
| `%` | Select All | `edit.select_all` |
| `d` | Delete Selection | `helix.delete` |
| `c` | Change Selection | `helix.change` |
| `y` | Yank Selection | `helix.yank` |
| `v` | Toggle Select Mode | `helix.toggle_select` |
| `s` | Select Regex Matches in Selection | `edit.select_regex_in_selection` |
| `C` | Add Multi-Cursor Down | `spawn_multi_cursor_down` |
| `.` | Repeat Last Change | `helix.repeat_last_change` |

After `Space`:

<!-- keys-from: ../../reference/keybindings-helix.md -->

| Key | Action | Action ID |
| --- | --- | --- |
| `f` | Open… | `file.open` |
| `/` | Search in Workspace | `search.workspace` |
| `k` | Hover | `lsp.hover` |
| `r` | Rename Symbol | `lsp.rename` |
| `a` | Code Action… | `lsp.code_action` |

Then see the [migration guide](../../migrating/index.md) for settings:
Helix's `editor.line-number = "relative"` becomes
`relative_line_numbers = true`, for one.

[^vix-helix]: The **Helix** keymap: `tui/tests/integration/keymaps.rs`,
    `helix_w_selects_a_word_and_d_deletes_it`,
    `helix_c_changes_every_match_and_dot_repeats_on_every_selection` and
    `helix_space_f_opens_the_file_picker`.
[^hx-keymap]: See [Keymap](https://docs.helix-editor.com/keymap.html).
[^hx-git]: A diff gutter shows changed lines; there is no staging, committing
    or blame.
[^hx-plugins]: No plugin system in a released version; one is in development.
[^menu]: `F10` shows and hides the menu bar: `tui/tests/integration/menu.rs`, `f10_toggles_menu_bar`.
[^lsp]: Every LSP action against a mock server: `tui/tests/integration/lsp.rs`, `every_lsp_action_applies_a_mock_servers_reply`; servers start only in a trusted workspace: `a_builtin_language_server_starts_only_in_a_trusted_workspace`.
[^tree-sitter]: The grammars compiled in by default are the `syntax-common` feature in `tui/Cargo.toml`.
[^multi]: `tui/tests/integration/editing.rs`, `select_all_occurrences_creates_multi_carets`, `ctrl_d_adds_a_caret_and_edits_all_occurrences` and `column_select_block_edits_multiple_lines`.
[^git]: Stage and commit, blame: `tui/tests/integration/git.rs`, `git_panel_stages_and_commits` and `git_blame_annotates_the_current_line`.
[^terminal]: `tui/tests/integration/terminals.rs`, `several_terminals_each_get_their_own_keys`.
[^undo]: `persistent_undo` defaults to `true` in `crates/vix-settings/src/lib.rs`; the tree view: `tui/tests/integration/undo_tree.rs`, `moving_the_selection_previews_and_enter_keeps_the_state`.
[^script]: Rhai scripts add palette commands and key bindings: `tui/tests/integration/scripting.rs`, `script_command_appears_in_the_command_palette` and `a_scripts_bind_key_request_actually_fires_through_on_key`; plugins are enabled and disabled from a chooser: `plugin_chooser_enables_and_disables_a_plugin`.
[^dap]: Breakpoints and the debugger actions: `tui/tests/integration/debugger.rs`, `edit_breakpoint_sets_condition_hit_count_and_log_message`.
[^keymap-menu]: **View → Keymap…** sets the `keymap` setting: `tui/tests/integration/keymaps.rs`, `view_keymap_submenu_actions_set_the_keymap`.

---

Vix™ and Vix IDE™ are trademarks.
