# Switch Project

**File → Switch Project…** (action `file.switch_project`, also in the command
palette) re-roots the running editor at another recently-used workspace without
relaunching.

## Behavior

- The chooser lists the workspace roots recorded in the session file
  (`Session::workspaces`, most-recent first), excluding the current one. Reports
  `status.no_recent_projects` when there is nowhere else to go.
- `↑`/`↓` move, `Enter` (or a click) switches, `Esc` cancels.
- A chosen project whose folder no longer exists reports `status.project_missing`.

## Switching

`switch_workspace` performs the re-root:

1. Save the current workspace session (open files, cursors, scroll, split).
2. Shut down the LSP and clear the per-file sync map.
3. Set the new root, rebuild the file explorer, and start a fresh LSP rooted there.
4. Reset the tabs to a single blank buffer and refresh the cached git state.
5. Restore the new workspace's saved session (reopen its files/cursors/layout).

## As implemented in Vix

The host owns `WorkspaceChooser`, `open_workspace_chooser`,
`workspace_chooser_key`/`_mouse`, `switch_to_selected_workspace`, and
`switch_workspace`; `ui::draw_workspace_chooser` renders it with the shared list
chooser. Recent roots come from [session restore](../session-restore/index.md).
