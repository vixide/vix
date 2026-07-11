# Find Literal

Editor action `find-literal`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `find-literal` |
| snake | `find_literal` |
| Pascal | `FindLiteral` |

Run it from the command palette or a key binding via the action id `find_literal`.
It is dispatched by `App::run_action("find_literal")` and, for editing actions, backed
by `Editor::find_literal` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
