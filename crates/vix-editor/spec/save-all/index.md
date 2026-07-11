# Save All

Editor action `save-all`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `save-all` |
| snake | `save_all` |
| Pascal | `SaveAll` |

Run it from the command palette or a key binding via the action id `save_all`.
It is dispatched by `App::run_action("save_all")` and, for editing actions, backed
by `Editor::save_all` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
