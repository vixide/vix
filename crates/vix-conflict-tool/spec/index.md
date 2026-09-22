# Conflict Tool

Parse Git merge-conflict markers, resolve the conflict under the cursor,
and list every conflict in a buffer for the Conflict overlay. Module
`conflict_tool`.

- menu "Git"
  - menuitem "Resolve Conflict at Cursor: Ours"
  - menuitem "Resolve Conflict at Cursor: Theirs"
  - menuitem "Resolve Conflict at Cursor: Both"
  - menuitem "Next Conflict"
  - menuitem "List Conflicts…"

A conflict block looks like:
```text
<<<<<<< HEAD
our lines
=======
their lines
>>>>>>> other-branch
```
[`find`] locates the block containing a given line; the host then replaces
that line range with the chosen side (ours / theirs / both) via
[`Resolution`]. This is the pre-existing per-cursor flow (`git.
conflict_ours`/`theirs`/`both` plus `git.conflict_next` to jump linearly);
it has no overview of how many conflicts remain or where they are.

## Conflict overlay (T556)

[`find_all`] walks the whole buffer once, collecting every conflict block
in source order (`find`'s own single-block scan, generalized). [`List`]
wraps that in the same shape every other list-and-jump panel in this
codebase uses (`vix-outline-panel`'s `Outline`, most directly) — a
`Vec<Conflict>` plus `selected`/`scroll`, built on the shared
`vix-list-state` navigation helpers (`up`/`down`/`page_up`/`page_down`/
`select_index`/`ensure_visible`), so the overlay's key handling is
mechanical, not bespoke.

**Git → List Conflicts…** (`git.conflict_list`) opens the overlay over
the active buffer. Each row previews both sides (their first line,
truncated to fit). `Enter` jumps the cursor to the selected conflict and
closes the overlay (mirroring `git.conflict_next`'s single-conflict jump,
but any-row instead of only "the next one"). A resolve key (`o`/`t`/`b`
for ours/theirs/both, matching the per-cursor actions' own letters)
resolves the highlighted conflict **without leaving the overlay** —
after any resolve, the host re-runs [`find_all`] against the buffer's
new content (a resolved block's line range no longer exists, so the
list can't just be patched in place) and clamps `selected` to the new,
shorter list. `Esc` closes without acting.

Workspace-wide (every conflicted file across the whole `git status`
output, not just the active buffer) is a deliberate non-goal for this
overlay — a natural follow-on, sized separately if wanted, not folded in
here to keep this change reviewable.
