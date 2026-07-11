# Delete Line

Editor action `delete-line`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `delete-line` |
| snake | `delete_line` |
| Pascal | `DeleteLine` |

Run it from the command palette or a key binding via the action id `delete_line`.
It is dispatched by `App::run_action("delete_line")` and, for editing actions, backed
by `Editor::delete_line` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
