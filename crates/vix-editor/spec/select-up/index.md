# Select Up

Editor action `select-up`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `select-up` |
| snake | `select_up` |
| Pascal | `SelectUp` |

Run it from the command palette or a key binding via the action id `select_up`.
It is dispatched by `App::run_action("select_up")` and, for editing actions, backed
by `Editor::select_up` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
