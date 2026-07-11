# Clipboard History (Kill Ring)

Editor action `edit.paste_from_history`.

Every copy and cut records the text into a ring (most-recent first, de-duplicated, capped at 30). The picker lists recent entries with one-line previews; Enter or a click pastes the chosen entry at the cursor and re-promotes it to the front.

From **Edit -> Paste from History...** or the command palette. State in `App::clipboard_ring`; overlay `ClipboardChooser`; recorded by `App::record_clipboard`.

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
