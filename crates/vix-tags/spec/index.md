# Matching Tag

Matching HTML/XML tag navigation. Navigation action `nav.matching_tag`.

Jump the cursor between an HTML/XML tag and its partner -- the closing tag for an opening one, or the opening tag for a closing one -- accounting for nested same-name tags. Self-closing and unmatched tags do nothing.

From **Go -> Matching Tag** or the command palette. Pure logic in `vix_tags::matching_tag`; jump via `Editor::goto_offset`.

## Reading from the cursor (T851)

`vix_tags::matching_tag_in(len, cursor, forward, backward)` gives `matching_tag`'s answer without a copy of the text: `forward(at)` yields the chars from `at` on and `backward(at)` the chars before `at`, nearest first (the App passes the rope's `chars_at`). A tag ends at the first `>` after its `<`, so the tag under the cursor starts at the first tag-starting `<` after the last `>` before the cursor; for an opening tag the partner search reads tags forward from its `>`, for a closing one backward (each `>` before it ends at most one tag, the one starting at the first tag-starting `<` after the `>` before that), and both stop at the partner. Go -> Matching Tag uses it. A test compares it with `matching_tag` at every cursor position of a table of texts and of proptest-generated ones.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). Tag lookup uses `get`, `take`, `skip` and `checked_*`; nesting depth that overflows ends the search with `None`. The scanner's offset arithmetic is one block with an `RCA-EXC-2` proof (each offset is an index of the char list plus a count of chars taken after it). No fallbacks are declared. `matching_tag_in` uses `checked_*` throughout: an offset that would overflow ends the search with `None`.
