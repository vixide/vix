# Org

Vix™ includes the **basics of [Org mode](https://orgmode.org/)** for working with
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

## Capture, agenda & time tracking

- **Capture…** pops a quick prompt; whatever you type is dropped in as a
  `* TODO …` headline at the cursor — a fast way to jot an idea or task.
- **Agenda Tracker** scans every `.org` file in the project and compiles their
  `DEADLINE:`/`SCHEDULED:` items and `TODO` headlines into one dated agenda
  buffer.
- **Clock In** drops an open `CLOCK: [timestamp]` entry at the cursor; **Clock
  Out** closes the most recent open entry with the end time and the elapsed
  `=> H:MM` duration.
- **Time Tracker** reads the `CLOCK:` lines in the current buffer and totals the
  time per headline into a report table.

## Export

- **Export → Markdown** and **Export → HTML** convert the current buffer and open
  the result in a new tab. Headlines, emphasis (`*bold*`, `/italic/`, …), links,
  and bullet lists are translated; it is a quick conversion, not a typesetter.

## Inserting Org content

To drop in Org snippets, emphasis markers, or `#+BEGIN_…` blocks, use
**Tools → Insert → Org / Markers / Begin-End** (see [Insert](../insert/index.md)).
Markers and blocks **toggle** around the current selection.

See the specification at [`crates/vix-org/spec/index.md`](../../crates/vix-org/spec/index.md).

---

Vix™ and Vix IDE™ are trademarks.
