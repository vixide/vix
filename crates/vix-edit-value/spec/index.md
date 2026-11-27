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
  pretty-printer, or `serde_norway` for YAML, writing each key as it was read
  (see Object keys). Scalar edits are undoable.

## Object keys (T850)

A key is kept in the form it was read: its type (number, string, boolean,
null), any tag, or a whole sequence or mapping used as a key. The row shows a
one-line label (`1`, `true`, `!k x` for a tagged key, `?` for a sequence or
mapping key), but labels are only for display: rows locate their entry by
position, and saving writes the key as read, never the label. So YAML keys
that look alike, such as `1` and `"1"`, `true` and `'true'`, `!!str 1` and `1`,
or two complex keys both shown as `?`, are each saved back as they were, and
editing one changes only its own value.

- **Scalar style.** The parser keeps what makes a key a different key (type and
  tag), not its quoting: `'1'` may be written back as `"1"` or `'1'`, which is
  the same key. Core-schema tags (`!!str`, `!!int`) are resolved into the key's
  type when read, so `!!str 1` is written back as the string `'1'`; other tags
  (`!k`) are written back as they were.
- **JSON names.** In JSON every key is a string. A number, boolean or null key
  is written as its text and a tagged key as its value. Per RFC 8259 names
  compare after unescaping, so `"a"` and the same letter written as the
  escape for U+0061 are one name: a JSON document naming it twice does not open
  (the parser reports the duplicate entry), and nothing can be half-saved.
- **Refusing to save.** `Tree::save_text` returns `SaveError` and the editor
  saves nothing when two keys of one object would be written as the same key
  (`DuplicateKey { key }`, translated as `msg.edit_value_duplicate_key`; in
  JSON this happens for YAML keys `1` and `"1"`), when a sequence or mapping
  key would have to be written as JSON (`ComplexKey`,
  `msg.edit_value_complex_key`), or when the YAML writer fails (`Yaml`,
  `msg.edit_value_serialize`). YAML keys are compared as parsed values; the
  parser already rejects a document with a repeated key, so this guards the
  writer, which would otherwise keep only one entry. `Tree::to_text` keeps its
  signature and returns the same refusals as a `serde_norway::Error` carrying
  the English message. `SaveError` is `#[non_exhaustive]`.
- **Writer limit.** `serde_norway` cannot write a one-entry mapping whose only
  key is tagged (`!k x: 0`); saving such a file reports
  `msg.edit_value_serialize` and leaves it unchanged.
- Values (not keys) still drop their tag when read.

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
