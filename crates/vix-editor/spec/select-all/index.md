# Select All

Editor action `select-all`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `select-all` |
| snake | `select_all` |
| Pascal | `SelectAll` |

Run it from the command palette or a key binding via the action id `select_all`.
It is dispatched by `App::run_action("select_all")` and, for editing actions, backed
by `Editor::select_all` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
