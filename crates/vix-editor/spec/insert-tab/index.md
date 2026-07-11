# Insert Tab

Editor action `insert-tab`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `insert-tab` |
| snake | `insert_tab` |
| Pascal | `InsertTab` |

Run it from the command palette or a key binding via the action id `insert_tab`.
It is dispatched by `App::run_action("insert_tab")` and, for editing actions, backed
by `Editor::insert_tab` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
