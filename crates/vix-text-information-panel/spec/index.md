# Text Information Panel

Statistics about a span of text, plus the panel's row-selection state.

Vix's Tools → About → Text… panel reports counts for the selection (or the
whole buffer when nothing is selected): characters, words, lines, sentences,
and paragraphs. The host gathers the text and opens a [`Panel`]; this crate
computes the [`Stats`], formats them into [`Row`]s, and tracks the selection
so a value can be inserted into the editor.

The heuristics are deliberately simple and language-agnostic:
- **characters**: Unicode scalar values.
- **words**: whitespace-separated runs.
- **lines**: newline-separated lines ([`str::lines`] semantics).
- **sentences**: runs of sentence-ending punctuation (`.`, `!`, `?`); text
with content but no terminator counts as one sentence.
- **paragraphs**: maximal runs of non-blank lines.

## See also

- [file-information-panel spec](../../vix-file-information-panel/spec/) — shared info-panel behavior

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). Row selection uses `checked_*`; the old `saturating_sub` is gone. The sentence and paragraph counters are plain increments under `RCA-EXC-2` expectations: they count characters and lines of an in-memory string.

| ID | Failure | Declared fallback | Why it is correct |
| -- | ------- | ----------------- | ----------------- |
| `TEXT-F2` | The highlight moves above the first row | the first row | Moving up stops at the first row. |
