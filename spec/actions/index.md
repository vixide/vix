# Actions

Vix's editor and host expose a flat catalog of named **actions** — the verbs a
key binding, menu item, or command-palette entry can invoke. Each action has
three spellings of one name:

- **kebab-case** — the canonical id used in `spec/<kebab>/index.md` and in
  documentation.
- **snake_case** — the Rust method on [`vix_editor::Editor`] (see
  `vix-editor/src/named.rs`) and the id accepted by `App::run_named_action`.
- **PascalCase** — the conceptual `Action` name.

The source of truth is [`actions.tsv`](actions.tsv): one row per action, three
columns (kebab / snake / Pascal).

## Dispatch

The host routes a snake_case id through `App::run_named_action(id)`:

- **Editing / motion** actions call the matching `Editor` method on the active
  tab (e.g. `cursor_up`, `select_word_left`, `duplicate_line`).
- **App- or mode-level** actions (tabs, splits, files, search, navigation)
  delegate to the existing dotted `run_action` ids (e.g. `vsplit` →
  `view.split_vertical`).
- **Not-yet-wired** actions (macros, shell/command/overwrite modes, suspend,
  autocomplete) are accepted and report a `status.action_todo` message rather
  than failing, so every catalog id is a no-surprise call.

Every action has a smoke test in `tests/integration.rs`
(`catalog_<snake>()`), and its own page under `spec/<kebab>/index.md`.

## Catalog

| kebab-case | snake_case | PascalCase |
| --- | --- | --- |
| `cursor-up` | `cursor_up` | `CursorUp` |
| `cursor-down` | `cursor_down` | `CursorDown` |
| `cursor-page-up` | `cursor_page_up` | `CursorPageUp` |
| `cursor-page-down` | `cursor_page_down` | `CursorPageDown` |
| `cursor-left` | `cursor_left` | `CursorLeft` |
| `cursor-right` | `cursor_right` | `CursorRight` |
| `cursor-start` | `cursor_start` | `CursorStart` |
| `cursor-end` | `cursor_end` | `CursorEnd` |
| `cursor-to-view-top` | `cursor_to_view_top` | `CursorToViewTop` |
| `cursor-to-view-center` | `cursor_to_view_center` | `CursorToViewCenter` |
| `cursor-to-view-bottom` | `cursor_to_view_bottom` | `CursorToViewBottom` |
| `select-to-start` | `select_to_start` | `SelectToStart` |
| `select-to-end` | `select_to_end` | `SelectToEnd` |
| `select-up` | `select_up` | `SelectUp` |
| `select-down` | `select_down` | `SelectDown` |
| `select-left` | `select_left` | `SelectLeft` |
| `select-right` | `select_right` | `SelectRight` |
| `word-right` | `word_right` | `WordRight` |
| `word-left` | `word_left` | `WordLeft` |
| `sub-word-right` | `sub_word_right` | `SubWordRight` |
| `sub-word-left` | `sub_word_left` | `SubWordLeft` |
| `select-word-right` | `select_word_right` | `SelectWordRight` |
| `select-word-left` | `select_word_left` | `SelectWordLeft` |
| `select-sub-word-right` | `select_sub_word_right` | `SelectSubWordRight` |
| `select-sub-word-left` | `select_sub_word_left` | `SelectSubWordLeft` |
| `delete-word-right` | `delete_word_right` | `DeleteWordRight` |
| `delete-word-left` | `delete_word_left` | `DeleteWordLeft` |
| `delete-sub-word-right` | `delete_sub_word_right` | `DeleteSubWordRight` |
| `delete-sub-word-left` | `delete_sub_word_left` | `DeleteSubWordLeft` |
| `select-line` | `select_line` | `SelectLine` |
| `select-to-start-of-line` | `select_to_start_of_line` | `SelectToStartOfLine` |
| `select-to-start-of-text` | `select_to_start_of_text` | `SelectToStartOfText` |
| `select-to-start-of-text-toggle` | `select_to_start_of_text_toggle` | `SelectToStartOfTextToggle` |
| `select-to-end-of-line` | `select_to_end_of_line` | `SelectToEndOfLine` |
| `paragraph-previous` | `paragraph_previous` | `ParagraphPrevious` |
| `paragraph-next` | `paragraph_next` | `ParagraphNext` |
| `select-to-paragraph-previous` | `select_to_paragraph_previous` | `SelectToParagraphPrevious` |
| `select-to-paragraph-next` | `select_to_paragraph_next` | `SelectToParagraphNext` |
| `insert-newline` | `insert_newline` | `InsertNewline` |
| `backspace` | `backspace` | `Backspace` |
| `delete` | `delete` | `Delete` |
| `insert-tab` | `insert_tab` | `InsertTab` |
| `save` | `save` | `Save` |
| `save-all` | `save_all` | `SaveAll` |
| `save-as` | `save_as` | `SaveAs` |
| `find` | `find` | `Find` |
| `find-literal` | `find_literal` | `FindLiteral` |
| `find-next` | `find_next` | `FindNext` |
| `find-previous` | `find_previous` | `FindPrevious` |
| `diff-next` | `diff_next` | `DiffNext` |
| `diff-previous` | `diff_previous` | `DiffPrevious` |
| `center` | `center` | `Center` |
| `undo` | `undo` | `Undo` |
| `redo` | `redo` | `Redo` |
| `copy` | `copy` | `Copy` |
| `copy-line` | `copy_line` | `CopyLine` |
| `cut` | `cut` | `Cut` |
| `cut-line` | `cut_line` | `CutLine` |
| `duplicate` | `duplicate` | `Duplicate` |
| `duplicate-line` | `duplicate_line` | `DuplicateLine` |
| `delete-line` | `delete_line` | `DeleteLine` |
| `move-lines-up` | `move_lines_up` | `MoveLinesUp` |
| `move-lines-down` | `move_lines_down` | `MoveLinesDown` |
| `join-lines` | `join_lines` | `JoinLines` |
| `sort-lines` | `sort_lines` | `SortLines` |
| `sort-unique` | `sort_unique` | `SortUnique` |
| `reverse-lines` | `reverse_lines` | `ReverseLines` |
| `remove-duplicate-lines` | `remove_duplicate_lines` | `RemoveDuplicateLines` |
| `trim-trailing-whitespace` | `trim_trailing_whitespace` | `TrimTrailingWhitespace` |
| `indent-selection` | `indent_selection` | `IndentSelection` |
| `outdent-selection` | `outdent_selection` | `OutdentSelection` |
| `autocomplete` | `autocomplete` | `Autocomplete` |
| `cycle-autocomplete-back` | `cycle_autocomplete_back` | `CycleAutocompleteBack` |
| `outdent-line` | `outdent_line` | `OutdentLine` |
| `indent-line` | `indent_line` | `IndentLine` |
| `paste` | `paste` | `Paste` |
| `paste-primary` | `paste_primary` | `PastePrimary` |
| `select-all` | `select_all` | `SelectAll` |
| `open-file` | `open_file` | `OpenFile` |
| `start` | `start` | `Start` |
| `end` | `end` | `End` |
| `page-up` | `page_up` | `PageUp` |
| `page-down` | `page_down` | `PageDown` |
| `select-page-up` | `select_page_up` | `SelectPageUp` |
| `select-page-down` | `select_page_down` | `SelectPageDown` |
| `half-page-up` | `half_page_up` | `HalfPageUp` |
| `half-page-down` | `half_page_down` | `HalfPageDown` |
| `start-of-text` | `start_of_text` | `StartOfText` |
| `start-of-text-toggle` | `start_of_text_toggle` | `StartOfTextToggle` |
| `start-of-line` | `start_of_line` | `StartOfLine` |
| `end-of-line` | `end_of_line` | `EndOfLine` |
| `toggle-help` | `toggle_help` | `ToggleHelp` |
| `toggle-key-menu` | `toggle_key_menu` | `ToggleKeyMenu` |
| `toggle-diff-gutter` | `toggle_diff_gutter` | `ToggleDiffGutter` |
| `toggle-ruler` | `toggle_ruler` | `ToggleRuler` |
| `toggle-highlight-search` | `toggle_highlight_search` | `ToggleHighlightSearch` |
| `unhighlight-search` | `unhighlight_search` | `UnhighlightSearch` |
| `reset-search` | `reset_search` | `ResetSearch` |
| `clear-status` | `clear_status` | `ClearStatus` |
| `shell-mode` | `shell_mode` | `ShellMode` |
| `command-mode` | `command_mode` | `CommandMode` |
| `toggle-overwrite-mode` | `toggle_overwrite_mode` | `ToggleOverwriteMode` |
| `escape` | `escape` | `Escape` |
| `quit` | `quit` | `Quit` |
| `quit-all` | `quit_all` | `QuitAll` |
| `force-quit` | `force_quit` | `ForceQuit` |
| `add-tab` | `add_tab` | `AddTab` |
| `previous-tab` | `previous_tab` | `PreviousTab` |
| `next-tab` | `next_tab` | `NextTab` |
| `first-tab` | `first_tab` | `FirstTab` |
| `last-tab` | `last_tab` | `LastTab` |
| `next-split` | `next_split` | `NextSplit` |
| `previous-split` | `previous_split` | `PreviousSplit` |
| `first-split` | `first_split` | `FirstSplit` |
| `last-split` | `last_split` | `LastSplit` |
| `unsplit` | `unsplit` | `Unsplit` |
| `vsplit` | `vsplit` | `VSplit` |
| `hsplit` | `hsplit` | `HSplit` |
| `toggle-macro` | `toggle_macro` | `ToggleMacro` |
| `play-macro` | `play_macro` | `PlayMacro` |
| `suspend` | `suspend` | `Suspend` |
| `scroll-up` | `scroll_up` | `ScrollUp` |
| `scroll-down` | `scroll_down` | `ScrollDown` |
| `spawn-multi-cursor` | `spawn_multi_cursor` | `SpawnMultiCursor` |
| `spawn-multi-cursor-up` | `spawn_multi_cursor_up` | `SpawnMultiCursorUp` |
| `spawn-multi-cursor-down` | `spawn_multi_cursor_down` | `SpawnMultiCursorDown` |
| `spawn-multi-cursor-select` | `spawn_multi_cursor_select` | `SpawnMultiCursorSelect` |
| `remove-multi-cursor` | `remove_multi_cursor` | `RemoveMultiCursor` |
| `remove-all-multi-cursors` | `remove_all_multi_cursors` | `RemoveAllMultiCursors` |
| `skip-multi-cursor` | `skip_multi_cursor` | `SkipMultiCursor` |
| `skip-multi-cursor-back` | `skip_multi_cursor_back` | `SkipMultiCursorBack` |
| `jump-to-matching-brace` | `jump_to_matching_brace` | `JumpToMatchingBrace` |
| `jump-line` | `jump_line` | `JumpLine` |
| `deselect` | `deselect` | `Deselect` |
| `clear-info` | `clear_info` | `ClearInfo` |
| `none` | `none` | `None` |
