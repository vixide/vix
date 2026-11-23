# Matching Tag

Matching HTML/XML tag navigation. Navigation action `nav.matching_tag`.

Jump the cursor between an HTML/XML tag and its partner -- the closing tag for an opening one, or the opening tag for a closing one -- accounting for nested same-name tags. Self-closing and unmatched tags do nothing.

From **Go -> Matching Tag** or the command palette. Pure logic in `vix_tags::matching_tag`; jump via `Editor::goto_offset`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). Tag lookup uses `get`, `take`, `skip` and `checked_*`; nesting depth that overflows ends the search with `None`. The scanner's offset arithmetic is one block with an `RCA-EXC-2` proof (each offset is an index of the char list plus a count of chars taken after it). No fallbacks are declared.
