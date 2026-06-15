# Remove Multi Cursor

Editor action `remove-multi-cursor`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `remove-multi-cursor` |
| snake | `remove_multi_cursor` |
| Pascal | `RemoveMultiCursor` |

Run it from the command palette or a key binding via the action id `remove_multi_cursor`.
It is dispatched by `App::run_action("remove_multi_cursor")` and, for editing actions, backed
by `Editor::remove_multi_cursor` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
