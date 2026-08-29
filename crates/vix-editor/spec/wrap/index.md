# Wrap

Editor action `edit.wrap`.

Hard-wrap (fill) text at the `wrap_column` setting (default `80`, see
`crates/vix-settings/spec/index.md`): the selection when there is one, otherwise
the paragraph around the cursor — the run of non-blank lines holding it.

Wrapping re-flows the words of each chunk greedily onto lines of at most
`wrap_column` characters, and keeps the shape of what it wraps:

- **Blank lines** separate chunks and are preserved verbatim, so wrapping a
  multi-paragraph selection wraps each paragraph on its own.
- **Indentation** of a chunk's first line is repeated on every wrapped line.
- **A comment or quote marker** shared by every line of the chunk (`///`, `//`,
  `#`, `--`, `;;`, `;`, `>`) is kept as the fill prefix rather than treated as a
  word.
- **List items** (`-`, `*`, `+`, `1.`, `1)`) each start their own chunk; the
  bullet stays on the first line and the rest hangs under it.
- A word longer than the column still gets its own line: text is never split
  mid-word and never lost.

No-op (with a status note) when the text is already wrapped or the cursor is not
in a paragraph. Widths count characters, not terminal columns.

From the **Edit -> Wrap** menu item. Pure logic in `crate::textops::wrap` (whole
text) and `crate::textops::wrap_paragraph_at` (cursor-relative); host method
`App::wrap_text`, which applies the former via
`App::transform_selection_or_buffer` and the latter via `App::rewrite_at_cursor`.

Distinct from **View -> Editor -> Soft Wrap**, which only changes how long lines
are displayed and never edits the buffer.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
