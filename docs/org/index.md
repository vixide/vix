# Org

Vix includes the **basics of [Org mode](https://orgmode.org/)** for working with
outline-style notes and task lists. Open the **Org** menu (`Alt+O`) with a buffer
of Org text.

It is a pragmatic subset — headline structure, TODO and checkbox toggling,
folding, and quick export — not a full Org implementation.

## Headlines

Org outlines are built from headlines: lines starting with one or more `*`.

```
* Project
** Task one
** Task two
```

- **Promote / Demote** (Org → Headline) remove or add a leading `*` across the
  whole subtree, so a branch and its children shift together.
- **Move Subtree Up / Down** reorder a headline and everything under it among its
  siblings. The cursor follows the moved subtree.
- **Cycle Visibility (Fold)** collapses or expands the section at the cursor.

## Tasks

- **Cycle TODO** turns a plain headline into `* TODO …`, then `* DONE …`, then
  back to plain.
- **Toggle Checkbox** flips a list item between `- [ ]` and `- [x]`.

## Export

- **Export → Markdown** and **Export → HTML** convert the current buffer and open
  the result in a new tab. Headlines, emphasis (`*bold*`, `/italic/`, …), links,
  and bullet lists are translated; it is a quick conversion, not a typesetter.

## Inserting Org content

To drop in Org snippets, emphasis markers, or `#+BEGIN_…` blocks, use
**Tools → Insert → Org / Markers / Begin-End** (see [Insert](../insert/index.md)).
Markers and blocks **toggle** around the current selection.

See the specification at [`spec/org/index.md`](../../spec/org/index.md).
