# Select To Start Of Line

Editor action `select-to-start-of-line`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `select-to-start-of-line` |
| snake | `select_to_start_of_line` |
| Pascal | `SelectToStartOfLine` |

Run it from the command palette or a key binding via the action id `select_to_start_of_line`.
It is dispatched by `App::run_action("select_to_start_of_line")` and, for editing actions, backed
by `Editor::select_to_start_of_line` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
