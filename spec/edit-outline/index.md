# Edit Outline

The **Edit Outline** command opens a hierarchical hide/show and restructuring
surface for prose text — code folding, but for prose, and a file-explorer-style
tree of the buffer's lines. Open it with **Tools → Edit Outline…** or the command
palette (*Edit Outline*); it reads the active buffer as an indented outline. Its
logic lives in the `edit_outline` module; the host (`app`/`ui`) renders the
visible items, syncs the scroll window, and persists saves.

It complements the read-only **code outline** (Ctrl+Shift+O, the `outline_panel`
module, which lists code symbols): the outline editor is a general prose
outliner that edits structure.

## Model

- Each line of the buffer becomes an **item** whose **level** is its indentation
  depth — leading tabs, or leading spaces / 2 (auto-detected per buffer).
- An item's **children** are the following deeper items; an item with children
  can be **collapsed** to hide its subtree (a view-only state, not saved).
- A **cursor** selects one visible item. The tree keeps an undo/redo history and
  a dirty flag.
- Saving regenerates each line's indentation from its level (tabs or two spaces)
  and writes through the normal file-save flow.

## Layout

- A bordered, near-full-screen overlay titled `Edit Outline` (with a `*` when
  there are unsaved edits).
- A scrolling list of visible items: each shows its indentation, a fold marker
  (`▾` expanded, `▸` collapsed, `·` leaf), and its text. The selected item is
  reverse-highlighted. A bottom hint line summarizes the keys.

## Keys

- **↑ / ↓** (or `k` / `j`): move to the previous / next visible item.
- **← / →** (or `h` / `l`): close / open. `←` collapses the item, or jumps to the
  parent when there is nothing to collapse; `→` expands it, or steps into the
  first child when already expanded.
- **Tab / Shift+Tab** (or **Alt+→ / Alt+←**): indent / outdent the item and its
  subtree one level. Indenting requires a preceding sibling to nest under.
- **Alt+↑ / Alt+↓**: move the item (with its subtree) up / down among its
  siblings. (Terminals cannot send Tab+arrow, so these "tab-up/tab-down" moves
  use Alt+arrows.)
- **Space**: toggle the current item's collapse state.
- **Home / End**, **PageUp / PageDown**: jump and page through visible items.
- **u** / **Ctrl+R**: undo / redo. **Ctrl+S**: save. **Esc** / **q**: close.

## As implemented in Vix

`edit_outline::Tree` owns the items (text + level + collapsed), the cursor, the
scroll offset, and undo/redo history. `Tree::handle_key` interprets key events
itself and returns an `Outcome` (`Consumed` / `Close` / `Save`) so the host stays
thin: `app` routes keys to the open tree, maps the outcome, and on save copies
the serialized text into the active editor buffer before invoking `file.save`.
`ui` renders the visible items (computed by `Tree::visible`, which omits items
under a collapsed ancestor) and records the body rectangle for paging.
