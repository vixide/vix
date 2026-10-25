# Save As

Editor action `save-as`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `save-as` |
| snake | `save_as` |
| Pascal | `SaveAs` |

Run it from **File → Save As…** (`Ctrl+Shift+S`), the command palette, or a
key binding via the action id `save_as` (an alias of `file.save_as`). It opens
a path prompt seeded with the buffer's current path; accepting it calls
`App::request_save_as`, then `Editor::save_active_as`.

## Target guards (T654)

- A **dangling symlink** at the target is refused with an error: writing
  would create a file wherever the link points.
- An **existing file** (or a live symlink) other than the buffer's own file
  asks a y/n confirmation ("already exists. Overwrite it?",
  `ConfirmKind::OverwriteSaveAs`). `y` writes; `n`/Esc cancels and leaves the
  existing file untouched.
- A new path, or the buffer's own path, saves straight away.

See `crates/vix-editor-core/spec/index.md` for the full catalog.
