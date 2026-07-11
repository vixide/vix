# Cursor End

Editor action `cursor-end`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `cursor-end` |
| snake | `cursor_end` |
| Pascal | `CursorEnd` |

Run it from the command palette or a key binding via the action id `cursor_end`.
It is dispatched by `App::run_action("cursor_end")` and, for editing actions, backed
by `Editor::cursor_end` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
