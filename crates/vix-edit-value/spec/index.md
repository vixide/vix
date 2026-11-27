# Edit JSON / Edit YAML

The structured-value editor: a foldable tree for JSON and YAML. The **Edit
JSON** and **Edit YAML** commands open it over the active
buffer's structured data — objects, arrays, and scalars — and let the user
collapse/expand containers, navigate, and edit scalar values. It is code folding,
but for structured data. Open them with **Edit → Mode → JSON…** / **Edit → Mode → YAML…**
or the command palette. The logic lives in the `edit_value` module; the host
(`app`/`ui`) renders the rows, syncs the scroll window, and persists saves.

## Model

- Both JSON and YAML parse into one model ([`Val`]): scalars (null, bool, number,
  string), arrays, and objects (object key order is preserved). Parsing uses
  `serde_norway`, which also reads JSON.
- A flattened view of the tree drives display; collapsed containers omit their
  descendants. A cursor selects one visible row. Numbers keep their textual form
  so they round-trip unchanged.
- Saving serializes back to the chosen [`Format`]: a key-order-preserving JSON
  pretty-printer, or `serde_norway` for YAML. Scalar edits are undoable.
- **Known limitation:** object keys are kept as strings, so YAML keys that are
  distinct values but render alike (`1` and `"1"`; a tagged or complex key,
  shown as `?`) collide on save: YAML keeps one of them, and JSON writes the
  key twice, which does not parse again. Any other document saves to text that
  reopens as the same tree and saves to the same text again; the `edit_value`
  fuzz target (`fuzz/fuzz_targets/edit_value.rs`) checks both.

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

## Errors (T610)

`ParseError`; `Tree::from_text` returns `Result<Tree, ParseError>`:

| Variant | Meaning |
| --- | --- |
| `Syntax { message }` | the text is not valid JSON or YAML; `message` is the parser's own, line and column included |

The type is `#[non_exhaustive]` with a hand-written `Display`/`Error` (no `thiserror`). `Display` is the same text the earlier `String` error carried, so the translated `status.convert_failed` line is unchanged.

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). **Changed:** `Tree::to_text` returns `Result<String, serde_norway::Error>`. A YAML tree that cannot be serialized used to give an empty text, which the editor then saved over the file; the editor now reports `msg.edit_value_serialize` and saves nothing. **Declared (VALUE-F1):** selection movement stops at the first and last row, and the last row of an empty list is 0. Nesting depth carries an `RCA-EXC-2` proof (a parsed document's recursion limit).
