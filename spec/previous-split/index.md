# Previous Split

Editor action `previous-split`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `previous-split` |
| snake | `previous_split` |
| Pascal | `PreviousSplit` |

Run it from the command palette or a key binding via the action id `previous_split`.
It is dispatched by `App::run_action("previous_split")` and, for editing actions, backed
by `Editor::previous_split` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
