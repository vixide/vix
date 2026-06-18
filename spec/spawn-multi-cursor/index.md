# Spawn Multi Cursor

Editor action `spawn-multi-cursor`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `spawn-multi-cursor` |
| snake | `spawn_multi_cursor` |
| Pascal | `SpawnMultiCursor` |

Run it from the command palette or a key binding via the action id `spawn_multi_cursor`.
It is dispatched by `App::run_action("spawn_multi_cursor")` and, for editing actions, backed
by `Editor::spawn_multi_cursor` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
