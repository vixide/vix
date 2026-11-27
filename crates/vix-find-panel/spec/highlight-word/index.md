# Highlight Word Occurrences

Editor action `view.highlight_word`; setting `highlight_word`.

Passively mark the whole-word occurrences of the identifier under the cursor on and around the visible rows. Off by default. Uses its own render channel so it never clobbers (sticky) search highlights; recomputed only when the buffer, the cursor or the viewport (its top line or height) changes, so scrolling without moving the cursor moves the marks, and an idle frame recomputes nothing.

The identifier is the run of word chars (alphanumeric or `_`) touching the cursor, if it starts with a letter or `_`. A lone occurrence is not marked.

Which occurrences (T852): at most 500, chosen from the view outward rather than from the top of the buffer, so the ones on screen are always marked:

1. every occurrence on the visible rows (the editor area's height from its top line);
2. when the cursor's line is off those rows (a jump whose scroll the next frame draws), the occurrences within as many lines of the cursor's line;
3. then outward from the visible rows, alternately above and below in steps of 256 lines, nearest first, up to **3,000 lines past the visible rows each way**.

The search stops as soon as 500 are found. Nothing in the UI shows a total count, so occurrences more than 3,000 lines from the view are never searched for: a word whose only other occurrence is farther away counts as a lone occurrence and is not marked. (Vim has no built-in equivalent; plugins such as vim-illuminate likewise mark only near the view.) The marks are handed to the renderer sorted by position. Each search covers whole lines, so the whole-word test is the same as over the whole text. Since T851 neither step copies the buffer: the identifier is read from the rope out from the cursor, and the occurrences are found with `vix_textops::whole_word_matches` over the rope's chunks (a byte search, about 1 ms per cursor move in an 8 MiB file with the identifier found once; 20 ms before, from two whole copies and a char-by-char scan). Since T852 the bounded window makes that about 34 µs, whatever the buffer's size.

From **View -> Editor -> Highlight Word Occurrences**. `App::refresh_word_highlight` (per event-loop tick; the window search is `word_marks_near` in `tui/src/app.rs`); the `word_marks` channel on `editor_core::Editor`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
