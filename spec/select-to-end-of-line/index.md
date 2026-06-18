# Select To End Of Line

Editor action `select-to-end-of-line`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `select-to-end-of-line` |
| snake | `select_to_end_of_line` |
| Pascal | `SelectToEndOfLine` |

Run it from the command palette or a key binding via the action id `select_to_end_of_line`.
It is dispatched by `App::run_action("select_to_end_of_line")` and, for editing actions, backed
by `Editor::select_to_end_of_line` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
