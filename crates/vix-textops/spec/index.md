# Textops

Small pure text transforms used by Edit/Tools actions.

Two shapes live here: whole-text transforms (`&str -> String`: line-ending
conversion, blank-line squeezing, ROT13, hard wrap) and cursor-relative rewrites
(`(&str, usize) -> Option<(String, usize)>`: increment number, smart toggle,
transpose characters/words/lines/sentences/paragraphs/sections, delete the
character/word/sentence/paragraph/section at the cursor, wrap the paragraph at
the cursor). The host applies the former via
`App::transform_selection_or_buffer` and the latter via
`App::rewrite_at_cursor`; everything here is unit-tested without a terminal.

The transpose and delete families share their unit builders: `word_units`,
`sentence_units`, `paragraph_units`, and `section_units` return the `(start,
end)` char ranges of the units in a text. `transpose_units_at` then swaps the
unit holding the cursor with its predecessor, leaving the text between them in
place, while `delete_unit_at` removes that unit together with the separator
after it (or, for the last unit, the one before it) so the surrounding text
closes up — bounded to the line for words and sentences, blank lines included
for paragraphs and sections. `sentence_starts` is public because the editor's
Go -> Sentence navigation uses it too, so navigation, transposition, and
deletion agree on where a sentence begins.

`wrap` refills text at a given width, treating blank lines and list bullets as
chunk boundaries and keeping each chunk's indentation, shared comment/quote
marker, and hanging bullet indent (see `crates/vix-editor/spec/wrap/index.md`).
