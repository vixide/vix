# Spawn Multi Cursor Select

Editor action `spawn-multi-cursor-select`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `spawn-multi-cursor-select` |
| snake | `spawn_multi_cursor_select` |
| Pascal | `SpawnMultiCursorSelect` |

Run it from the command palette or a key binding via the action id `spawn_multi_cursor_select`.
It is dispatched by `App::run_action("spawn_multi_cursor_select")` and, for editing actions, backed
by `Editor::spawn_multi_cursor_select` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
