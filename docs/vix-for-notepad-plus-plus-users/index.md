# Vix™ for Notepad++ users

Notepad++ is a free Windows text editor: fast, tabbed, with macros, column
editing, encoding conversion and a plugin manager. Vix keeps much of that
everyday toolkit and adds a language server client, a debugger, a Git client
and a terminal, in a terminal editor that also runs on macOS and Linux. This
page shows which keymap to pick, where your Notepad++ features live in Vix,
and where Notepad++ is still the better tool.

Claims about Notepad++ describe its documented defaults; see the
[Notepad++ User Manual](https://npp-user-manual.org/). Every claim about Vix
cites the test or file that checks it.

## Pick a keymap

Vix has no Notepad++ keymap. The closest is the **Sublime** keymap: like
Notepad++, it binds `Ctrl+F` to Find, `Ctrl+H` to Replace, `Ctrl+Shift+F`
to Find in Files and `Ctrl+G` to Go to Line. Pick it from **View → Keymap…**,
or set `keymap = "sublime"` in `config.toml`.[^keymap-menu] The default
**Apple** keymap is the other choice if you mostly use `Ctrl+O`, `Ctrl+S`
and `Ctrl+F`. `F1` lists every key the active keymap binds,[^f1] and any key
can be rebound in `keybindings.toml` (the counterpart of Notepad++'s
Shortcut Mapper).[^override]

These rows are copied from the generated
[Sublime keybindings reference](../reference/keybindings-sublime.md); a test
fails if a row here stops matching it. Compare each one with the key you
remember from Notepad++ before relying on it: `Ctrl+D`, for one, selects
every occurrence of the word here.

<!-- keys-from: ../reference/keybindings-sublime.md -->

| Key | Action | Action ID |
| --- | --- | --- |
| `C-n` | New | `file.new` |
| `C-s` | Save | `file.save` |
| `C-S-s` | Save As… | `file.save_as` |
| `C-w` | Close | `file.close` |
| `C-S-w` | Close All Tabs | `file.close_all` |
| `C-S-t` | Reopen Closed Tab | `file.reopen_closed` |
| `C-f` | Find… | `edit.find` |
| `C-h` | Find & Replace | `edit.replace` |
| `C-S-f` | Search in Workspace | `search.workspace` |
| `C-g` | Number | `nav.goto_line` |
| `C-d` | Select All Occurrences | `edit.select_all_occurrences` |
| `C-S-d` | Duplicate Line | `edit.duplicate_line` |
| `C-S-k` | Cut Line | `cut_line` |
| `C-j` | Join Lines | `edit.join_lines` |
| `C-/` | Comment | `edit.toggle_comment` |
| `C-m` | Jump to Matching Bracket | `edit.match_bracket` |

## Where your Notepad++ features went

| In Notepad++ | In Vix |
| --- | --- |
| Tabs, and reopening the last session | Tabs; the last session reopens by default[^session] |
| Find in Files, Replace in Files | Workspace search and replace (`Ctrl+Shift+F` above)[^search] |
| Macro recording and saved macros | **Edit → Macro → Record**, **Play** and **Save…**[^macro] |
| Column mode | Column selection: **Edit → Select → Column Select Up/Down**, `Alt+Shift+Up`/`Down` by default[^multi] |
| Multi-editing | Multiple carets and select all occurrences[^multi] |
| Encoding menu | **File → Reopen with Encoding…** and **Save with Encoding…**; the encoding is kept byte for byte[^encoding] |
| EOL conversion | Line endings are detected and kept, and the **Edit** menu converts them (`edit.eol_lf`, `edit.eol_crlf`)[^eol] |
| Session snapshot and periodic backup | Hot exit: unsaved buffers come back after a crash[^hot-exit] |
| Compare plugin | **Compare With File**, side by side[^compare] |
| Opening very large files | The large file viewer[^large] |
| Plugins Admin | Rhai scripts and plugins[^script] |
| Syntax highlighting | Tree-sitter highlighting[^tree-sitter] |

On top of those, Vix has language servers (diagnostics, completion,
rename),[^lsp] a debugger,[^dap] a Git panel,[^git] integrated
terminals,[^terminal] and persistent, branching undo.[^undo] It runs on
macOS, Linux and Windows,[^platforms] and edits remote folders over
SSH.[^remote]

## Where Notepad++ is the better choice

- **A native Windows window** with mouse-first menus, toolbars and dialogs;
  Vix draws a character grid in a terminal.
- **Its plugin catalog**, which covers far more than Vix's Rhai scripts and
  plugins.
- **User-defined languages.** Notepad++ lets you define highlighting for a new
  language from a dialog; Vix highlights the languages it has Tree-sitter
  grammars for.

## See also

- [Migrating from another editor](../migrating/index.md).
- [Vix for Sublime Text users](../vix-for-sublime-users/index.md): the keymap
  this page recommends.
- [Macros](../macros/index.md), [File encodings](../encoding/index.md) and
  [Multiple cursors](../multiple-cursors/index.md).

[^keymap-menu]: **View → Keymap…** sets the `keymap` setting: `tui/tests/integration/keymaps.rs`, `view_keymap_submenu_actions_set_the_keymap`.
[^f1]: `F1` lists the active keymap's keys: `tui/tests/integration/keymaps.rs`, `help_overlay_includes_the_active_keymap_chords`.
[^override]: A `keybindings.toml` entry wins over the active keymap: `tui/tests/integration/keymaps.rs`, `key_override_wins_over_a_builtin_keymap_binding`.
[^session]: `restore_session` defaults to `true` in `crates/vix-settings/src/lib.rs`; the round trip: `tui/tests/integration/workspace.rs`, `session_snapshot_and_restore_round_trip`.
[^search]: `tui/tests/integration/workspace.rs`, `workspace_search_finds_matches_across_files` and `workspace_replace_rewrites_files`.
[^macro]: `tui/tests/integration/editing.rs`, `macro_records_and_replays_editor_keys`; saved macros: `crates/vix-macros/src/lib.rs`, `upsert_writes_and_replaces_by_name`.
[^multi]: `tui/tests/integration/editing.rs`, `select_all_occurrences_creates_multi_carets`, `ctrl_d_adds_a_caret_and_edits_all_occurrences` and `column_select_block_edits_multiple_lines`.
[^encoding]: `crates/vix-encoding/src/lib.rs`, `every_choice_round_trips_byte_for_byte` and `unmappable_characters_refuse_to_encode`; the choosers: `tui/tests/integration/editing.rs`, `encoding_choosers_convert_and_reopen`.
[^eol]: Detection: `tui/tests/integration/editing.rs`, `line_ending_detects_crlf`; conversion: `text_transforms_squeeze_eol_and_rot13`.
[^hot-exit]: `tui/tests/integration/hot_exit.rs`, `a_crashed_buffer_round_trips_through_its_backup`.
[^compare]: `tui/tests/integration/compare.rs`, `opens_both_files_side_by_side_with_marks`.
[^large]: `tui/tests/integration/large_file.rs`, `open_in_large_file_viewer_navigates_a_multi_megabyte_file`.
[^script]: Rhai scripts add palette commands and key bindings: `tui/tests/integration/scripting.rs`, `script_command_appears_in_the_command_palette` and `a_scripts_bind_key_request_actually_fires_through_on_key`; plugins are enabled and disabled from a chooser: `plugin_chooser_enables_and_disables_a_plugin`.
[^tree-sitter]: The grammars compiled in by default are the `syntax-common` feature in `tui/Cargo.toml`.
[^lsp]: Every LSP action against a mock server: `tui/tests/integration/lsp.rs`, `every_lsp_action_applies_a_mock_servers_reply`; servers start only in a trusted workspace: `a_builtin_language_server_starts_only_in_a_trusted_workspace`.
[^dap]: Breakpoints and the debugger actions: `tui/tests/integration/debugger.rs`, `edit_breakpoint_sets_condition_hit_count_and_log_message`.
[^git]: Stage and commit, blame: `tui/tests/integration/git.rs`, `git_panel_stages_and_commits` and `git_blame_annotates_the_current_line`.
[^terminal]: `tui/tests/integration/terminals.rs`, `several_terminals_each_get_their_own_keys`.
[^undo]: `persistent_undo` defaults to `true` in `crates/vix-settings/src/lib.rs`; the tree view: `tui/tests/integration/undo_tree.rs`, `moving_the_selection_previews_and_enter_keeps_the_state`.
[^platforms]: Release targets for macOS, Linux and Windows on x86-64 and ARM: `dist-workspace.toml`.
[^remote]: Experimental. `tui/tests/remote_ui.rs`, `open_remote_connects_saves_survives_a_lost_connection_and_reconnects`; see [remote editing](../remote-editing/index.md).

---

Vix™ and Vix IDE™ are trademarks.
