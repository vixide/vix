# Vix™ compared with Neovim

Neovim is a modal terminal editor that you build into an IDE with Lua
plugins. Vix is a terminal editor with the IDE pieces built in, ordinary keys
by default, and a **Vi** keymap one menu choice away. This page puts the two
side by side, says which to pick when, and how to switch. For the exact
account of which Vi keys behave the same, see
[Vix for Vim users](../../vix-for-vim-users/index.md), whose parity table is
generated from a test.

Claims about Neovim describe its documented defaults, with no plugins; see the
[Neovim documentation](https://neovim.io/doc/user/). Every claim about Vix
cites the test or file that checks it. The wider matrix, with Vim, Emacs,
Helix, Zed, Fresh and ttt, is on the [comparison page](../index.md).

## At a glance

`✓` built in. `-` not built in (a plugin may exist). `~` partial: see the
footnote.

| | Vix | Neovim |
| --- | :---: | :---: |
| Modal editing by default | -[^vix-modal] | ✓ |
| Menu bar | ✓[^menu] | - |
| Language server client | ✓[^lsp] | ✓[^nvim-lsp] |
| Tree-sitter highlighting | ✓[^tree-sitter] | ✓[^nvim-ts] |
| Debugger | ✓[^dap] | -[^nvim-dap] |
| Fuzzy file finder and command palette | ✓[^palette] | -[^nvim-fuzzy] |
| Multiple cursors | ✓[^multi] | - |
| Git: stage, commit, blame | ✓[^git] | - |
| Integrated terminal | ✓[^terminal] | ✓[^nvim-term] |
| Persistent, branching undo | ✓[^undo] | ~[^nvim-undo] |
| Scripting | Rhai scripts and plugins[^script] | Lua and Vimscript |

## Choose Neovim when

- You want to build your own editor. Lua reaches far deeper than Vix's Rhai
  scripts, and the plugin ecosystem is vast.
- You need the full Vi language: Vix's Vi keymap covers the common core but
  not all of it (see the
  [known differences](../../vix-for-vim-users/index.md#known-differences-in-short)).
- Startup time and memory matter most: in Vix's own measurements Neovim
  starts faster and uses less memory on a 10,000-line source file (see
  [Performance](../../performance/index.md)).

## Choose Vix when

- You want an IDE in the terminal on first launch, without choosing and
  configuring plugins.
- You want menus, a palette and `F1` help to discover commands, with modal
  editing optional.
- You want a Git client, a debugger and persistent, browsable undo without
  plugins.

## Switching

Pick **View → Keymap… → Vi**, or set `keymap = "vi"` in
`config.toml`.[^keymap-menu] A few rows from the generated
[Vi keybindings reference](../../reference/keybindings-vi.md) (a test fails if
a row here stops matching it):

<!-- keys-from: ../../reference/keybindings-vi.md -->

| Key | Action | Action ID |
| --- | --- | --- |
| `i` | Insert Before Cursor | `vim.insert` |
| `a` | Append After Cursor | `vim.append` |
| `o` | Open Line Below | `vim.open_below` |
| `u` | Undo | `edit.undo` |
| `/` | Find… | `edit.find` |
| `n` | Find Next | `edit.find_next` |
| `%` | Jump to Matching Bracket | `edit.match_bracket` |

Operators, counts, text objects, `.` and `:` commands go beyond that table;
the [Vim guide](../../vix-for-vim-users/index.md) lists them. Then bring your
settings across with the [migration guide](../../migrating/index.md): Vix does
not read `init.lua` or `init.vim`.

[^vix-modal]: Off by default; the **Vi**, **Spacemacs** and **Helix** keymaps
    are modal. Vi Normal mode: `tui/tests/integration/keymaps.rs`,
    `vim_keymap_normal_mode_is_modal`.
[^nvim-lsp]: The client is built in; you install and configure each server.
    See [LSP](https://neovim.io/doc/user/lsp.html).
[^nvim-ts]: See [Tree-sitter](https://neovim.io/doc/user/treesitter.html).
[^nvim-dap]: Through plugins such as nvim-dap.
[^nvim-fuzzy]: Command-line completion is built in; fuzzy pickers are
    plugins.
[^nvim-term]: `:terminal`; see [Terminal](https://neovim.io/doc/user/terminal.html).
[^nvim-undo]: `undofile` keeps undo history across sessions and the history
    branches, but browsing the branches needs a plugin. See
    [Undo](https://neovim.io/doc/user/undo.html).
[^menu]: `F10` shows and hides the menu bar: `tui/tests/integration/menu.rs`, `f10_toggles_menu_bar`.
[^lsp]: Every LSP action against a mock server: `tui/tests/integration/lsp.rs`, `every_lsp_action_applies_a_mock_servers_reply`; servers start only in a trusted workspace: `a_builtin_language_server_starts_only_in_a_trusted_workspace`.
[^tree-sitter]: The grammars compiled in by default are the `syntax-common` feature in `tui/Cargo.toml`.
[^dap]: Breakpoints and the debugger actions: `tui/tests/integration/debugger.rs`, `edit_breakpoint_sets_condition_hit_count_and_log_message`.
[^palette]: The prefixes are `Mode::from_input` in `crates/vix-palette/src/lib.rs`.
[^multi]: `tui/tests/integration/editing.rs`, `select_all_occurrences_creates_multi_carets`, `ctrl_d_adds_a_caret_and_edits_all_occurrences` and `column_select_block_edits_multiple_lines`.
[^git]: Stage and commit, blame: `tui/tests/integration/git.rs`, `git_panel_stages_and_commits` and `git_blame_annotates_the_current_line`.
[^terminal]: `tui/tests/integration/terminals.rs`, `several_terminals_each_get_their_own_keys`.
[^undo]: `persistent_undo` defaults to `true` in `crates/vix-settings/src/lib.rs`; the tree view: `tui/tests/integration/undo_tree.rs`, `moving_the_selection_previews_and_enter_keeps_the_state`.
[^script]: Rhai scripts add palette commands and key bindings: `tui/tests/integration/scripting.rs`, `script_command_appears_in_the_command_palette` and `a_scripts_bind_key_request_actually_fires_through_on_key`; plugins are enabled and disabled from a chooser: `plugin_chooser_enables_and_disables_a_plugin`.
[^keymap-menu]: **View → Keymap…** sets the `keymap` setting: `tui/tests/integration/keymaps.rs`, `view_keymap_submenu_actions_set_the_keymap`.

---

Vix™ and Vix IDE™ are trademarks.
