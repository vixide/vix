# Toggle Macro

Editor action `toggle-macro`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `toggle-macro` |
| snake | `toggle_macro` |
| Pascal | `ToggleMacro` |

Run it from the command palette or a key binding via the action id `toggle_macro`.
It is dispatched by `App::run_action("toggle_macro")` and, for editing actions, backed
by `Editor::toggle_macro` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
