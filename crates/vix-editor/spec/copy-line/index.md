# Copy Line

Editor action `copy-line`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `copy-line` |
| snake | `copy_line` |
| Pascal | `CopyLine` |

Run it from the command palette or a key binding via the action id `copy_line`.
It is dispatched by `App::run_action("copy_line")` and, for editing actions, backed
by `Editor::copy_line` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
