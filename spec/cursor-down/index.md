# Cursor Down

Editor action `cursor-down`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `cursor-down` |
| snake | `cursor_down` |
| Pascal | `CursorDown` |

Run it from the command palette or a key binding via the action id `cursor_down`.
It is dispatched by `App::run_action("cursor_down")` and, for editing actions, backed
by `Editor::cursor_down` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
