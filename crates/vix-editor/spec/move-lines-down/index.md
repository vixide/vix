# Move Lines Down

Editor action `move-lines-down`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `move-lines-down` |
| snake | `move_lines_down` |
| Pascal | `MoveLinesDown` |

Run it from the command palette or a key binding via the action id `move_lines_down`.
It is dispatched by `App::run_action("move_lines_down")` and, for editing actions, backed
by `Editor::move_lines_down` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
