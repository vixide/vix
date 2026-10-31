# Undo tree panel

Draw a buffer's undo tree as an ASCII graph, preview any state live by moving
the selection, then keep it (`Enter`) or go back to where you started (`Esc`).

**Status:** Shipped (T638). **Edit → Undo Tree…** (`edit.undo_tree`, also in
the command palette) opens the panel over the active buffer. Vix's undo
history has always been a tree that keeps every branch (see
[`vix-editor-core`](../../vix-editor-core/spec/index.md#undo-tree-inspection-t638)),
but **Edit → Switch Undo Branch** (`edit.undo_branch`) only cycles branches
blind. This panel shows the whole tree, like Vim's undotree and Emacs vundo.

## Behavior

| Key                    | Effect                                                        |
| ---------------------- | ------------------------------------------------------------- |
| `↑` / `↓`              | Select the previous / next state, and show it in the buffer   |
| `PageUp` / `PageDown`  | Move a page of states                                         |
| `Home` / `End`         | Select the original state / the newest state                  |
| `Enter`                | Keep the selected state and close                             |
| `Esc`                  | Return the buffer to the starting state and close             |
| Left click             | Select and show the clicked state                             |

- **Live preview.** Each selection change moves the real buffer to that state
  (`Editor::goto_undo_state`: undo to the common ancestor, redo down the
  selected branch), so the text, the cursor and the history's current state
  move together.
- **Enter commits.** The buffer stays at the selected state, which is now the
  current state of the history; redo follows the branch you came down. The tab
  is marked modified when the state changed.
- **Esc restores** the starting text, cursor, selection, modified flag and
  every branch's active-redo pointer (the history is snapshotted on open and
  put back after returning to the starting state).
- **Shared buffers refuse.** In a Cowork shared buffer (the edit log is on,
  `is_edit_logging`) local undo is off, so the panel does not open; the status
  bar says why. Undo there goes through the collaboration layer and reverses
  only your own edits.
- **No history.** A buffer with no edits yet has nothing to draw; the status
  bar says so instead of opening an empty panel.
- **Read-only tabs** block it, like every other edit that changes text.
- **Previews never persist.** While the panel is open, auto-save, the hot-exit
  backup pass (T629) and the reload-on-disk-change check are paused, so no
  previewed state is written to disk, to a backup, or replaced underneath the
  panel. Quitting with the panel open first restores the starting state. The
  persistent undo store (`vix-undo-store`) writes only on save, which the
  modal panel blocks; and the text and the history's current state always
  move together, so a stored history always matches its content hash.

## The graph

One row per state, **oldest first** (the root, the buffer's original state, at
the top), in creation order — the `seq` shown beside each row. Each branch of
the tree runs down its own column ("lane"):

```text
o      0  original
o      1  12:00:01
|\
o |    2  12:00:04
  @    3  12:00:09
```

- `o` is a state, `@` the state the panel opened at, `|` a lane passing a row
  that belongs to another branch.
- A node's **first** child continues its lane; every later child opens a new
  lane, drawn on a connector row right after the parent: `\` (or `-…\`) to a
  lane on the right, `/` (or `/…-`) to one on the left, crossing other lanes
  without breaking them.
- A lane is freed after its last state and reused by the next branch, so the
  graph is only as wide as the most branches alive at once.
- A linear history is one column; a deep history is just a long one.

The time is the state's local wall-clock time (`HH:MM:SS`, 24-hour); a state
with no recorded time (from an undo file written before T638) shows none.

## As implemented in Vix

- `vix-undo-tree-panel` is host-agnostic: no dependency on the editor or on
  terminal I/O. `TreeNode` is the input (`seq`, `parent`, `children`,
  `timestamp`), `layout(nodes, mark)` returns the `Row`s (graph text, plus the
  state's `seq` and timestamp, or `None` for a connector row), and `Panel`
  holds the rows, the selection (over state rows only, through
  `vix-list-state`), the scroll offset and the starting `seq`.
- `vix-editor-core` supplies `History::nodes()`, `History::current()`,
  `History::path_to()` and `Editor::goto_undo_state()`.
- In the App shell, `tui/src/app/undo_tree.rs` opens the panel (refusing a
  shared buffer), previews on every move, and commits or restores;
  `tui/src/ui/undo_tree.rs` draws it.
