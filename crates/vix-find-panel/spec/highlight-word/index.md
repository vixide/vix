# Highlight Word Occurrences

Editor action `view.highlight_word`; setting `highlight_word`.

Passively mark every whole-word occurrence of the identifier under the cursor. Off by default. Uses its own render channel so it never clobbers (sticky) search highlights; recomputed only when the buffer or cursor changes.

From **View -> Editor -> Highlight Word Occurrences**. `App::refresh_word_highlight` (per event-loop tick); the `word_marks` channel on `editor_core::Editor`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
