# Highlight Word Occurrences

Editor action `view.highlight_word`; setting `highlight_word`.

Passively mark every whole-word occurrence of the identifier under the cursor. Off by default. Uses its own render channel so it never clobbers (sticky) search highlights; recomputed only when the buffer or cursor changes.

The identifier is the run of word chars (alphanumeric or `_`) touching the cursor, if it starts with a letter or `_`; at most the first 500 whole-word occurrences in the buffer are marked, and a lone occurrence is not. Since T851 neither step copies the buffer: the identifier is read from the rope out from the cursor, and the occurrences are found with `vix_textops::whole_word_matches` over the rope's chunks (a byte search, about 1 ms per cursor move in an 8 MiB file with the identifier found once; 20 ms before, from two whole copies and a char-by-char scan).

From **View -> Editor -> Highlight Word Occurrences**. `App::refresh_word_highlight` (per event-loop tick); the `word_marks` channel on `editor_core::Editor`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
