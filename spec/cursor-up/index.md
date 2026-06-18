# Cursor Up

Editor action `cursor-up`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `cursor-up` |
| snake | `cursor_up` |
| Pascal | `CursorUp` |

Run it from the command palette or a key binding via the action id `cursor_up`.
It is dispatched by `App::run_action("cursor_up")` and, for editing actions, backed
by `Editor::cursor_up` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
