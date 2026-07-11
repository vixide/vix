# Remove All Multi Cursors

Editor action `remove-all-multi-cursors`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `remove-all-multi-cursors` |
| snake | `remove_all_multi_cursors` |
| Pascal | `RemoveAllMultiCursors` |

Run it from the command palette or a key binding via the action id `remove_all_multi_cursors`.
It is dispatched by `App::run_action("remove_all_multi_cursors")` and, for editing actions, backed
by `Editor::remove_all_multi_cursors` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
