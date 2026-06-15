# Page Down

Editor action `page-down`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `page-down` |
| snake | `page_down` |
| Pascal | `PageDown` |

Run it from the command palette or a key binding via the action id `page_down`.
It is dispatched by `App::run_action("page_down")` and, for editing actions, backed
by `Editor::page_down` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
