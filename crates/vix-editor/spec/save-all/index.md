# Save All

Editor action `save-all`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `save-all` |
| snake | `save_all` |
| Pascal | `SaveAll` |

Run it from **File → Save All**, the command palette, or a key binding. The
menu and palette dispatch `file.save_all`; the catalog id `save_all` is an
alias that forwards to it, so both run `App::save_all` (T654).

## Behavior

- Saves **every dirty, file-backed tab**, each through the same write path as
  **Save** (`write_active_to_disk`: atomic write, the on-save options, the
  language server's `didSave`, persistent undo).
- **Skipped, and counted** in the status line: untitled and scratch buffers
  (no path; Save All never opens a Save As prompt), the editable
  search-results buffer, images, read-only tabs (View-only, lossy, synthetic)
  and files this user can't write.
- **Stops at the first failure**: the error is reported, the status line names
  the file and how many were saved before it, and that tab is left active.
  Otherwise the originally active tab is re-activated and the status line
  reports how many files were saved.
- Format on save is not run: that is a per-file language-server round trip
  that **Save** owns for the active buffer.

See `crates/vix-editor-core/spec/index.md` for the full catalog.
