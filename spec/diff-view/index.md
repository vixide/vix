# Diff View (Compare With File)

**Tools → Compare With File…** (action `tools.diff`) shows a read-only unified
diff between the active buffer and another file.

## Behavior

- Prompts for a file path (`prompt.compare_file`), resolved relative to the
  workspace root.
- Reads that file and diffs it against the **current buffer contents** (not the
  saved file), so unsaved edits are reflected. The other file is the "old" side,
  the buffer is the "new" side.
- Opens a scrollable overlay titled `<other> ↔ <here>`. Added lines are green
  (`+`), removed lines red (`-`), context dimmed; non-adjacent hunks are separated
  by a `⋯` line.
- `↑`/`↓` and `PageUp`/`PageDown` scroll; `Esc` or `q` close.
- Identical files report `status.diff_identical` instead of opening the overlay;
  an unreadable file reports an open error.

## As implemented in Vix

The `diff_view` module (`build`, `Line`, `Kind`) produces the unified diff from
two strings via `similar`'s grouped line ops (3 lines of context); it is pure and
unit tested. The host owns `DiffViewState`, `open_compare_prompt`,
`open_diff_with`, and `diff_view_key`; `ui::draw_diff_view` renders it.
