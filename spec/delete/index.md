# Delete

Editor action `delete`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `delete` |
| snake | `delete` |
| Pascal | `Delete` |

Run it from the command palette or a key binding via the action id `delete`.
It is dispatched by `App::run_action("delete")` and, for editing actions, backed
by `Editor::delete` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
