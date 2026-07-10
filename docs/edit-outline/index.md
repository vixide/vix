# Edit Outline

Edit Outline turns the active buffer into a foldable, reorderable tree — code
folding, but for prose, and a file-explorer-style hierarchy of the buffer's
lines. It is one of Vix™'s *edit surfaces*: a full-screen overlay that owns its own
keys and saves back to the buffer.

## Opening

Open it from **Edit → Mode → Outline…** or the command palette (*Edit Outline*).
Each line becomes an outline item whose **level** is its indentation depth —
leading tabs, or two spaces per level, auto-detected from the buffer.

## Folding and moving

- **↑ / ↓** (or **k / j**) move to the previous / next visible item.
- **← / →** (or **h / l**) close / open the item: `←` collapses it (or jumps to
  the parent when there is nothing to collapse), `→` expands it (or steps into the
  first child). **Space** toggles the fold. Collapsing an item hides its whole
  subtree.
- **Home / End**, **PageUp / PageDown** jump and page through visible items.

## Restructuring

- **Tab** / **Shift + Tab** (or **Alt + → / ←**) indent / outdent the item and
  its subtree. Indenting needs a preceding sibling to nest under.
- **Alt + ↑ / ↓** move the item (with its subtree) up / down among its siblings.
  (Terminals can't send Tab+arrow, so these moves use Alt+arrows.)
- **u** undoes, **Ctrl + R** redoes.

## Saving and closing

**Ctrl + S** writes the restructured outline back to the buffer (indentation is
regenerated from each item's level) and saves it. **Esc** or **q** closes.

This is distinct from the read-only **Code Outline** (Ctrl+Shift+O), which lists
code symbols. See the specification at `spec/edit-outline/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
