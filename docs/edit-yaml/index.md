# Edit YAML

Edit YAML opens the active buffer as a foldable tree of mappings, sequences, and
scalar values. It is the YAML twin of [Edit JSON](../edit-json/index.md): the same
editor, navigation, and editing keys — only the file format differs.

## Opening

Open it from **Edit → Mode → YAML…** or the command palette (*Edit YAML*). The
buffer is parsed into a tree (mapping key order preserved). If it is not valid
YAML, Vix™ says so instead of opening.

## Using it

Navigation, folding, and value editing are identical to Edit JSON — see
[Edit JSON](../edit-json/index.md) for the full key reference:

- **↑ / ↓** / **← / →** (or **k / j / h / l**) to move and fold,
- **Enter** / **F2** to edit a scalar, **Space** to toggle a fold,
- **u** / **Ctrl + R** to undo / redo.

## Saving and closing

**Ctrl + S** serializes the tree back to YAML into the active buffer and saves it.
**Esc** or **q** closes. See the specification at `spec/edit-value/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
