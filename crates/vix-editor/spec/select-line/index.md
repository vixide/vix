# Select Line

Editor action `select-line`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `select-line` |
| snake | `select_line` |
| Pascal | `SelectLine` |

Run it from the command palette or a key binding via the action id `select_line`.
It is dispatched by `App::run_action("select_line")` and, for editing actions, backed
by `Editor::select_line` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
