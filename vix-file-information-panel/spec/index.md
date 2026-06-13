# File Information Panel

A Tools-menu panel of facts about the file in the active editor tab. **Tools →
File Information…** opens a small table; pressing Enter (or clicking a row)
inserts that row's value into the editor.

## As implemented in Vix

**Status:** Shipped. The `vix-file-information-panel` crate formats the rows and
holds the row-selection + scroll state; the host (`src/app.rs`, `src/ui.rs`)
gathers the raw values — counts from the buffer, and size / permissions /
modified-time from the filesystem — into a `FileInfo`, then renders the overlay.

## Rows

| Row          | Source                                                       |
| ------------ | ------------------------------------------------------------ |
| Name         | File name, or `(unsaved)` for a never-saved buffer           |
| Path         | Full path, or `(unsaved)`                                    |
| Language     | The editor's detected language id                            |
| Modified     | Whether the buffer has unsaved changes                       |
| Characters   | Character count of the buffer                                |
| Words        | Whitespace-separated word count                             |
| Lines        | Line count                                                   |
| Size         | On-disk size (human-readable + exact bytes); saved files only |
| Permissions  | Unix mode as `rwxr-xr-x` + octal; Unix + saved files only    |
| Last modified | File mtime as `YYYY-MM-DD HH:MM:SS UTC`; saved files only   |

Rows that need a saved file on disk (Size, Permissions, Last modified) are
omitted for unsaved buffers.

| Key / action  | Effect                                      |
| ------------- | ------------------------------------------- |
| `↑` / `↓`     | Move the highlight                          |
| `PgUp`/`PgDn` | Move one page                               |
| `Home`/`End`  | Jump to the first / last row                |
| `Enter` / click | Insert the highlighted row's value        |
| `Esc`         | Close the panel                             |
