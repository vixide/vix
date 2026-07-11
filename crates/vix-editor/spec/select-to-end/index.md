# Select To End

Editor action `select-to-end`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `select-to-end` |
| snake | `select_to_end` |
| Pascal | `SelectToEnd` |

Run it from the command palette or a key binding via the action id `select_to_end`.
It is dispatched by `App::run_action("select_to_end")` and, for editing actions, backed
by `Editor::select_to_end` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
