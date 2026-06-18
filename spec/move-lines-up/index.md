# Move Lines Up

Editor action `move-lines-up`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `move-lines-up` |
| snake | `move_lines_up` |
| Pascal | `MoveLinesUp` |

Run it from the command palette or a key binding via the action id `move_lines_up`.
It is dispatched by `App::run_action("move_lines_up")` and, for editing actions, backed
by `Editor::move_lines_up` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
