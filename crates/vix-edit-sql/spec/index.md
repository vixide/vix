# Edit SQL

The SQL edit surface: a statement-oriented view of a `.sql` buffer
(`vix-edit-sql` mode). Open it with **Edit → Mode → SQL…** (`tools.edit_sql`)
or the command palette. The
logic lives in the pure `crate::edit_sql` module; the host renders the overlay
and saves back through the normal file-save flow.

## What it does

The buffer is parsed into individual SQL **statements** by splitting on top-level
semicolons — semicolons inside single/double-quoted strings, line comments
(`-- …`), and block comments (`/* … */`) are ignored. Each statement is shown as
a row with a **kind** label (its leading keyword: `SELECT`, `INSERT`, `CREATE`,
…) and a one-line preview.

From the overlay you can:

- **Navigate** — `↑`/`↓` (or `k`/`j`), `PgUp`/`PgDn`, `Home`/`End`.
- **Reorder** — `Alt+↑`/`Alt+↓` (or `K`/`J`) move the statement among its peers.
- **Delete** — `d` (or `Delete`) removes the statement.
- **Format** — `f` uppercases SQL keywords in the selected statement; `F` does
  every statement. Keywords inside strings/comments are left alone.
- **Undo / redo** — `u` / `Ctrl+R`.
- **Save** — `Ctrl+S` serializes the statements back into the buffer (one per
  block, `;`-terminated) and saves via the normal flow. A statement that ends
  in a `--` line comment gets its `;` on the next line, so the comment cannot
  swallow it. A statement that ends inside an unclosed quoted string or
  `/* …` block comment (which the splitter only produces as the last
  statement, since it runs to the end of the text) gets no `;` at all: one
  would become part of the string or comment, and every save would add
  another. Saving then reopening gives the same statements. **Esc** / `q`
  closes.

This is a pragmatic helper for organizing and tidying SQL files, not a full SQL
parser or query engine. The `edit_sql` module is unit-tested
(`split_statements`, `format_sql`, kind detection, reorder/delete/undo).
Property tests (`crates/vix-edit-sql/tests/props.rs`, T831) check that the splitter never panics
and returns trimmed pieces of its input in order, that `to_text` splits back
to the same statements for any mix of strings and comments holding `;`, and
that `format_sql` changes only letter case and is idempotent. The
`sql_split` fuzz target (`fuzz/fuzz_targets/sql_split.rs`) checks the same
round trip and formatter properties on arbitrary text, including unclosed
strings and comments.

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). The keyword formatter walks with `get` and a checked step; selection movement and statement reordering use `checked_*`. **Declared (SELECT-F1):** selection stops at the first and last statement, and the last index of an empty list is 0.
