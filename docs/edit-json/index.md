# Edit JSON

Edit JSON opens the active buffer as a foldable tree of objects, arrays, and
scalar values, so you can browse and edit structured data without hunting through
braces. It shares one editor with [Edit YAML](../edit-yaml/index.md) — only the
parser and serializer differ.

## Opening

Open it from **Tools → Edit JSON…** or the command palette (*Edit JSON*). The
buffer is parsed into a tree; object key order is preserved, and numbers keep
their original textual form so they round-trip unchanged. If the buffer is not
valid JSON (or YAML), Vix says so instead of opening.

## Folding and navigating

- **↑ / ↓** (or **k / j**) move to the previous / next visible row;
  **PageUp / PageDown**, **Home / End** page and jump.
- **← / →** (or **h / l**) collapse / expand the selected container, or step to
  its parent / first child. **Space** toggles the fold.

## Editing values

- **Enter** or **F2** edits the selected scalar; type the new value, **Enter**
  commits, **Esc** cancels. The text is interpreted as `true` / `false` / `null`,
  a number when it parses as one, otherwise a string. On a container, Enter
  toggles the fold.
- **u** undoes, **Ctrl + R** redoes.

## Saving and closing

**Ctrl + S** serializes the tree back to JSON (pretty-printed, key order
preserved) into the active buffer and saves it. **Esc** or **q** closes.

See the specification at `spec/edit-value/index.md`.
