# Compare With File

Quickly see how the file you're editing differs from another file — a backup, a
sibling config, the version on another path — without leaving Vix.

## Using it

Open **Tools → Compare With File…** (or the command palette), then type the path
to compare against (relative paths resolve from the workspace root). Vix shows a
read-only unified diff between that file and your **current buffer** (including
unsaved edits):

- **Green `+`** lines are in your buffer but not the other file.
- **Red `-`** lines are in the other file but not your buffer.
- Dimmed lines are unchanged context; a `⋯` marks a gap between changes.

Scroll with `↑` / `↓` or `PageUp` / `PageDown`, and close with `Esc` (or `q`). If
the two are identical, Vix says so in the status bar instead of opening the view.

See the specification at `spec/diff-view/index.md`. For changes against the Git
HEAD version of a file, use the [diff gutter and per-hunk tools](../git-panel/index.md)
instead.
