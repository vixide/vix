# Align on Delimiter

Align lines on a delimiter (`=`, `:`, `,`, `|`, …). Editor actions
`edit.align.equals` / `colon` / `comma` / `pipe`.

Pad the selected lines (or the whole buffer) so each line's first occurrence of the delimiter lands in a common column, normalized to one space on each side. Lines without the delimiter are left unchanged; a trailing newline is preserved.

From **Edit -> Align** or the command palette. Pure logic in `crate::align::on_delimiter`, applied via `App::transform_selection_or_buffer`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). The padding width uses a subtraction with a declared fallback.

| ID | Failure | Declared fallback | Why it is correct |
| -- | ------- | ----------------- | ----------------- |
| `ALIGN-F1` | The padding `width - left` goes below zero | no padding | The width is the maximum over all split lines, so this never happens; if it did, the line would simply not be padded. |
