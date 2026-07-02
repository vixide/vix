# Save Automation

Settings `format_on_save`, `auto_save`; plus external-change auto-reload.

Format on save runs the configured LSP formatter after each save. Auto-save writes the active buffer on an interval. Auto-reload detects files changed on disk by another process and reloads clean buffers (warning once on buffers with unsaved edits).

Toggled from **View -> Editor**. `App::poll_auto_save`, `App::poll_file_changes`, and the format-on-save path in `App::save` (`format_save_pending`).

See `spec/index/index.md` for the project overview and `spec/actions/index.md` for the full action catalog.
