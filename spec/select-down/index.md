# Select Down

Editor action `select-down`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `select-down` |
| snake | `select_down` |
| Pascal | `SelectDown` |

Run it from the command palette or a key binding via the action id `select_down`.
It is dispatched by `App::run_action("select_down")` and, for editing actions, backed
by `Editor::select_down` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
