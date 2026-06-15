# Select To Start

Editor action `select-to-start`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `select-to-start` |
| snake | `select_to_start` |
| Pascal | `SelectToStart` |

Run it from the command palette or a key binding via the action id `select_to_start`.
It is dispatched by `App::run_action("select_to_start")` and, for editing actions, backed
by `Editor::select_to_start` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
