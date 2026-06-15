# Page Up

Editor action `page-up`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `page-up` |
| snake | `page_up` |
| Pascal | `PageUp` |

Run it from the command palette or a key binding via the action id `page_up`.
It is dispatched by `App::run_action("page_up")` and, for editing actions, backed
by `Editor::page_up` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
