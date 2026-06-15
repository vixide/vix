# Find Previous

Editor action `find-previous`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `find-previous` |
| snake | `find_previous` |
| Pascal | `FindPrevious` |

Run it from the command palette or a key binding via the action id `find_previous`.
It is dispatched by `App::run_action("find_previous")` and, for editing actions, backed
by `Editor::find_previous` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
