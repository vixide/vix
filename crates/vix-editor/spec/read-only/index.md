# Read-Only Buffer

Editor action `view.read_only`; per-tab `Tab::read_only`.

Lock the active buffer against edits: typing, deletes, and editing commands are blocked (with a status note), while navigation, selection, copy, and search still work. Toggling it off restores editing.

From **View -> Editor -> Read-Only** or the command palette. Gated at `editor_key`, `insert_str`, `transform_selection_or_buffer`, and `run_edit_action` (default-deny over `edit.*` with a read-only-safe allowlist).

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
