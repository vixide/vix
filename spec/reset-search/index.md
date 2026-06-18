# Reset Search

Editor action `reset-search`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `reset-search` |
| snake | `reset_search` |
| Pascal | `ResetSearch` |

Run it from the command palette or a key binding via the action id `reset_search`.
It is dispatched by `App::run_action("reset_search")` and, for editing actions, backed
by `Editor::reset_search` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
