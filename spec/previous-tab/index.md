# Previous Tab

Editor action `previous-tab`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `previous-tab` |
| snake | `previous_tab` |
| Pascal | `PreviousTab` |

Run it from the command palette or a key binding via the action id `previous_tab`.
It is dispatched by `App::run_action("previous_tab")` and, for editing actions, backed
by `Editor::previous_tab` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
