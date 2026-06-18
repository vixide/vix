# Select Page Up

Editor action `select-page-up`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `select-page-up` |
| snake | `select_page_up` |
| Pascal | `SelectPageUp` |

Run it from the command palette or a key binding via the action id `select_page_up`.
It is dispatched by `App::run_action("select_page_up")` and, for editing actions, backed
by `Editor::select_page_up` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
