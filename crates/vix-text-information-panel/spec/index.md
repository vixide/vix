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
