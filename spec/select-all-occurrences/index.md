# Select All Occurrences

Editor action `edit.select_all_occurrences`.

Select every occurrence of the current selection (or the word at the cursor) in
the buffer at once, placing a selected caret on each match — the last becomes the
primary caret — so a single keystroke turns into a multi-cursor edit of all
matches.

Run it from **Edit → Select → Select All Occurrences**, the command palette, or
the action id `edit.select_all_occurrences`. It is dispatched by
`App::run_action` and backed by `Editor::add_all_occurrences` in `editor_core`
(the all-at-once counterpart of the Ctrl+D add-next-occurrence flow). See
`spec/actions/index.md` for the full catalog.
