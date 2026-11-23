# Emmet Expansion

A pragmatic subset of Emmet abbreviation expansion. Editor action `edit.emmet_expand`.

Expand the Emmet abbreviation before the cursor into HTML -- child `>`, sibling `+`, multiply `*N`, `#id`, `.class`, `{text}`, and `$` numbering (e.g. `ul>li.item$*3`). Grouping `()` is unsupported.

From **Edit -> Emmet Expand** or the command palette. Pure logic in `crate::emmet::expand`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). Node lookups use `get` (a missing node ends the expansion with `None`), depth uses `checked_add`, and the parser's one cursor step carries an `RCA-EXC-2` proof. No fallbacks are declared.
