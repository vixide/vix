# Git Graph

**Git → Log → Graph…** shows your repository's history as a graph: one row per
commit, with lanes on the left that follow the branches and merges, the
branches and tags that point at each commit, and its subject, author and date.
It is the interactive sibling of **Git → Log → Graph**, which only prints
`git log --graph` text in the bottom dock.

```text
 ●─╮ (*main, #v1) merge topic     Ann 2026-10-27
 │ ● (topic) side                 Ann 2026-10-26
 ● │ mainline                     Ann 2026-10-25
 ╰─╯ ...
```

(`*` marks the branch `HEAD` is on and `#` a tag.)

## Using it

- **Move** with the arrow keys and PageUp/PageDown, or click a row.
- **Search** by just typing: rows are narrowed to commits whose hash, author,
  subject or ref names contain the text. Backspace edits the search.
- **Filter by branch or tag** with `Tab` (and `Shift+Tab` to go back). It steps
  through *all refs*, then each local branch, remote branch and tag.
- **Open a commit** with `Enter` or by clicking the highlighted row: its diff
  opens in a read-only tab titled `<hash> graph`, the same way
  **Browse Log…** does.
- **Check out** the commit with `Ctrl+O` (after a yes/no). If a local branch
  points at it, Vix switches to that branch; otherwise `HEAD` is detached
  there.
- **Create a branch** at the commit with `Ctrl+B`: type the name and press
  `Enter`.
- **Cherry-pick** the commit onto the current branch with `Ctrl+P` (after a
  yes/no). If the pick stops on a conflict, it is undone and the error is
  shown, so the repository is never left half-done.
- **Copy the hash** with `Ctrl+Y`.
- `Esc` cancels a pending question, and otherwise closes the graph.

History loads 200 commits at a time; the next page loads as you scroll
toward the end of the list.

## See also

- [Git Panel](../git-panel/index.md) for staging and committing.
- The `vix-git-graph` crate spec (`crates/vix-git-graph/spec/index.md`) for how
  the lanes are assigned.
