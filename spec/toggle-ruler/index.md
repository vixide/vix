# Toggle Ruler

Editor action `toggle-ruler`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `toggle-ruler` |
| snake | `toggle_ruler` |
| Pascal | `ToggleRuler` |

Run it from the command palette or a key binding via the action id `toggle_ruler`.
It is dispatched by `App::run_action("toggle_ruler")` and, for editing actions, backed
by `Editor::toggle_ruler` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
