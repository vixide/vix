# Compare Files

See how two files differ, and move changes between them, without leaving Vix™.
Both sides are real, editable buffers.

## Opening a comparison

- **Tools → Compare → Compare With File…** (or the command palette): type the
  path of the file to compare the current file against (relative paths resolve
  from the workspace root). The other file opens on the left, the current file
  on the right, in a side-by-side split.
- From the shell: `vix --diff OLD NEW`, which is also how `git difftool` runs
  Vix (see [the CLI page](../cli/index.md)).

## Working in it

- The gutter marks each side's changes: yellow for a changed line, green for a
  line only on that side, red where lines exist only on the other side.
- The two panes scroll together: the other pane follows the one you're in.
- **Next Difference** and **Previous Difference** move the cursor between changes.
- **Copy Change to Left** and **Copy Change to Right** copy the change at the
  cursor to the other side, as one undo step there (like vimdiff `do`/`dp`).
- Edit either side as normal. Saving a side saves that file, and the comparison
  updates as you type.
- **Show as Unified Diff** opens the read-only unified view (green `+` added,
  red `-` removed, `⋯` between changes). Scroll it with `↑`/`↓` or
  `PageUp`/`PageDown`, and close it with `Esc` or `q`.
- **Close Comparison** removes the split. Both files stay open. Closing either
  tab also ends the comparison.

An unsaved, untitled buffer has no file to put in a pane, so comparing one
opens the unified view instead.

See the specification at `crates/vix-diff-view/spec/index.md`. For changes against the Git
HEAD version of a file, use the [diff gutter and per-hunk tools](../git-panel/index.md)
instead.

---

Vix™ and Vix IDE™ are trademarks.
