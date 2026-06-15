# Select Page Down

Editor action `select-page-down`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `select-page-down` |
| snake | `select_page_down` |
| Pascal | `SelectPageDown` |

Run it from the command palette or a key binding via the action id `select_page_down`.
It is dispatched by `App::run_action("select_page_down")` and, for editing actions, backed
by `Editor::select_page_down` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
