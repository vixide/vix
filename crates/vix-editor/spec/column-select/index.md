# Column / Rectangular Selection

Editor actions `edit.column_select_down` and `edit.column_select_up`.

Extend a vertical (rectangular) selection one line at a time: a caret is added on
the line past the current block frontier, spanning the same column range as the
block's first (oldest) caret's selection (or a bare caret when there is no
selection), so a clamped short line doesn't narrow the lines after it. Columns
clamp to each line's length, and the new caret becomes primary (T622). The resulting carets edit together, so typing or
deleting applies to the whole block.

Bound to **Alt+Shift+↑** / **Alt+Shift+↓**; also **Edit → Select → Column Select
Up/Down** and the command palette. Dispatched by `App::run_action` and backed by
`Editor::column_select` in `editor_core`, which rides the existing multi-caret
edit path. See `crates/vix-editor-core/spec/index.md` for the full catalog.
