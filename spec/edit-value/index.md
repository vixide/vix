# Edit JSON / Edit YAML

The **Edit JSON** and **Edit YAML** commands open a foldable tree over the active
buffer's structured data — objects, arrays, and scalars — and let the user
collapse/expand containers, navigate, and edit scalar values. It is code folding,
but for structured data. Open them with **Tools → Edit JSON…** / **Edit YAML…**
or the command palette. The logic lives in the `edit_value` module; the host
(`app`/`ui`) renders the rows, syncs the scroll window, and persists saves.

## Model

- Both JSON and YAML parse into one model ([`Val`]): scalars (null, bool, number,
  string), arrays, and objects (object key order is preserved). Parsing uses
  `serde_yaml`, which also reads JSON.
- A flattened view of the tree drives display; collapsed containers omit their
  descendants. A cursor selects one visible row. Numbers keep their textual form
  so they round-trip unchanged.
- Saving serializes back to the chosen [`Format`]: a key-order-preserving JSON
  pretty-printer, or `serde_yaml` for YAML. Scalar edits are undoable.

## Keys

- **↑/↓** (or `k`/`j`): previous / next visible row. **PageUp/PageDown**,
  **Home/End** to page and jump.
- **←/→** (or `h`/`l`): collapse / expand the selected container (or step to its
  parent / first child). **Space** toggles fold.
- **Enter** / **F2**: edit the selected scalar (type, Enter commits, Esc cancels);
  on a container, toggles fold. Edited text becomes `true`/`false`/`null`, a
  number when it parses as one, else a string.
- **u** / **Ctrl+R**: undo / redo. **Ctrl+S**: save. **Esc** / **q**: close.

## As implemented in Vix

`edit_value::Tree` owns the parsed value, the flattened rows, the cursor, fold
state, and an undo history, and interprets keys itself, returning an `Outcome`
(`Consumed`/`Close`/`Save`). `app` opens it with `Format::Json` or `Format::Yaml`,
routes keys, and on save serializes into the active buffer before `file.save`.
`ui` renders each row as indentation + a fold marker + `label: value`.
