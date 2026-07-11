# Indent Line

Editor action `indent-line`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `indent-line` |
| snake | `indent_line` |
| Pascal | `IndentLine` |

Run it from the command palette or a key binding via the action id `indent_line`.
It is dispatched by `App::run_action("indent_line")` and, for editing actions, backed
by `Editor::indent_line` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
