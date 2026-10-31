# Save Automation

Settings `format_on_save`, `auto_save`; plus external-change auto-reload.

Format on save runs the configured LSP formatter after each save. Auto-reload detects files changed on disk by another process and reloads clean buffers (warning once on buffers with unsaved edits).

## Auto-save modes

`auto_save` takes one of three modes (`vix_settings::AutoSave`):

| Mode | What is saved, and when |
| --- | --- |
| `"off"` (default) | Nothing. |
| `"interval"` | The active dirty, file-backed buffer, every few seconds (`App::poll_auto_save`). |
| `"focus_lost"` | Every dirty buffer Save All could write, when the terminal window loses focus (`App::on_focus_lost`, T628). The tab that was active stays active; a failed write stops there, on that tab. |

Older config files wrote a boolean: `true` still loads as `"interval"` and
`false` as `"off"`. Saving the settings writes the mode name.

**View -> Editor -> Auto Save** toggles: any mode turns off, and off turns
on `"interval"`.

Auto-save writes plainly (no format-on-save round trip). Untitled buffers,
images, remote and read-only tabs are never auto-saved.

## Focus events

The terminal front end asks for focus reports (xterm mode 1004,
crossterm's `EnableFocusChange`) and passes them to `App::on_focus_gained`
and `App::on_focus_lost` (T628). Gaining focus runs the external-change
check at once, bypassing its one-second throttle, so a file edited in
another program is already reloaded when the user looks back. Terminals
without focus reporting never send the events; the one-second poll still
runs either way.

Toggled from **View -> Editor**. `App::poll_auto_save`, `App::poll_file_changes`, and the format-on-save path in `App::save` (`format_save_pending`).

Tests: `tui/tests/integration/terminal_protocol.rs` (focus-gained reload,
focus-lost saves, the toggle) and `vix-settings`' unit tests (old booleans
load, the mode name round-trips).

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
