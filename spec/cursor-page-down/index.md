# Cursor Page Down

Editor action `cursor-page-down`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `cursor-page-down` |
| snake | `cursor_page_down` |
| Pascal | `CursorPageDown` |

Run it from the command palette or a key binding via the action id `cursor_page_down`.
It is dispatched by `App::run_action("cursor_page_down")` and, for editing actions, backed
by `Editor::cursor_page_down` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
