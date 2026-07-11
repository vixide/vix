# Find Next

Editor action `find-next`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `find-next` |
| snake | `find_next` |
| Pascal | `FindNext` |

Run it from the command palette or a key binding via the action id `find_next`.
It is dispatched by `App::run_action("find_next")` and, for editing actions, backed
by `Editor::find_next` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
