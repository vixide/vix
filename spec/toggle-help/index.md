# Toggle Help

Editor action `toggle-help`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `toggle-help` |
| snake | `toggle_help` |
| Pascal | `ToggleHelp` |

Run it from the command palette or a key binding via the action id `toggle_help`.
It is dispatched by `App::run_action("toggle_help")` and, for editing actions, backed
by `Editor::toggle_help` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
