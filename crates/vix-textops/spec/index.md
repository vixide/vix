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

**Streaming sentence scan (T830).** Whether a char starts a sentence depends
only on the chars before it: a small state (leading whitespace, inside plain
text, inside a run of closers `.`/`!`/`?`/`"`/`'`/`)`/`]`/`}` with or
without a sentence end, in whitespace after a sentence end or not) carried
forward over `text.chars()`. `sentence_starts` runs that scan without
decoding the text into a `Vec<char>`. `sentence_start_from(before, from,
at)` returns the first start at or after `at` and
`sentence_start_before(before, at)` the last one before it, both over char
iterators (`before` nearest first), so a caller holding a rope (Vi `(`/`)`
in `vix-modal`, Go -> Sentence -> Next/Previous in `vix-editor`) reads only from the whitespace and closers just behind `at`
to the start it finds. All three give exactly the starts of the pre-T830
whole-text scan, which the tests keep as an oracle and compare at every
offset of a table of texts and of proptest-generated ones.

`wrap` refills text at a given width, treating blank lines and list bullets as
chunk boundaries and keeping each chunk's indentation, shared comment/quote
marker, and hanging bullet indent (see `crates/vix-editor/spec/wrap/index.md`).

## Cleanup (T723)

`cleanup::cleanup(text)` is the whitespace half of Code Cleanup: trims
trailing spaces and tabs, drops blank lines at the end of the file and ends a
non-empty file in exactly one newline, keeping CRLF if the file uses it. It
returns the new text and a `Report` of what changed. The language-server half
(organize imports) is the host's.
