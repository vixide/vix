# Diff View (Compare Files)

Compare two texts: the aligned two-pane model for side-by-side Compare Files, and a unified diff.

Side-by-side Compare Files (T660) is editable on both sides; the unified diff
is a read-only overlay.

## Side-by-side Compare Files

Both sides are real tabs in a vertical split: the left (old) file and the right
(new) file. Each is an ordinary buffer: editable, saved like any other tab.

- **Opening.** `vix --diff OLD NEW` (also `git difftool`), and **Tools → Compare
  → Compare With File…** (`tools.diff`) when the active buffer is a file, with
  the prompted file on the left and the active file on the right. The left pane
  is focused.
- **Row map.** `compare::hunks` lists the changed regions as line ranges on
  each side (a pure insertion or deletion has an empty range on one side). The
  host recomputes it whenever either buffer's edit revision changes.
- **Locked scrolling.** Each frame, the unfocused pane's top line is set to the
  line that matches the focused pane's top line (`compare::map_line`).
- **Gutter.** Each side's changed lines are marked: changed (a line on both
  sides), only on this side, and a gap mark where lines exist only on the other
  side. For a compared tab these marks replace the git and coverage gutters.
- **Actions.** `diff.next` and `diff.prev` move the cursor to the start of the
  next or previous change on the focused side; at the end, the status says
  there are no more. `diff.copy_to_left` and `diff.copy_to_right` replace the
  other side's lines of the change at the cursor with this side's lines, as one
  undo step there (vimdiff `do`/`dp`); a read-only target refuses. Line endings
  follow the target's. `diff.unified` opens the unified overlay for the pair's
  current text. `diff.close` removes the split and keeps both tabs.
- **Lifetime.** The comparison tracks its tabs by path and ends when either tab
  closes. Without an open comparison the `diff.*` actions report that in the
  status line.
- **Menu.** Tools → Compare holds Compare With File… and every `diff.*` action;
  they are in the command palette too.

### Not yet: filler rows

The two panes are aligned at their top lines, not row by row: below a change
that gives one side more lines than the other, the rows no longer line up. Filler
rows on the shorter side need the editor widget to draw rows with no buffer
line behind them, in every render layer (text, syntax, selections, marks,
guides, cursor, clicks, soft wrap), so they are tracked separately as T671.

## Unified diff overlay

- **Tools → Compare → Compare With File…** from an untitled buffer, and
  `diff.unified` from a comparison. `App::open_diff_files` opens it for two
  paths directly.
- Diffs the **current buffer contents** (not the saved file), so unsaved edits
  are reflected. The other file is the "old" side.
- Opens a scrollable overlay titled `<other> ↔ <here>`. Added lines are green
  (`+`), removed lines red (`-`), context dimmed; non-adjacent hunks are
  separated by a `⋯` line.
- `↑`/`↓` and `PageUp`/`PageDown` scroll; `Esc` or `q` close.
- Identical files report `status.diff_identical` instead of opening the overlay;
  an unreadable file reports an open error.

## As implemented in Vix

`compare` (`hunks`, `map_line`, `hunk_at`, `next_hunk`, `prev_hunk`, `marks`,
`copy_hunk`) is the pure two-pane model; the host's `tui/src/app/compare.rs`
owns the split, the per-frame refresh and the actions. The crate root's `build`,
`Line` and `Kind` produce the unified diff from `similar`'s grouped line ops
(3 lines of context). Both are pure and unit tested. The host owns
`DiffViewState`, `open_compare_prompt`, `open_diff_with`, and `diff_view_key`;
`ui::draw_diff_view` renders the overlay.
