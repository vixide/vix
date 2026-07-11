# Jump Line

Editor action `jump-line`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `jump-line` |
| snake | `jump_line` |
| Pascal | `JumpLine` |

Run it from the command palette or a key binding via the action id `jump_line`.
It is dispatched by `App::run_action("jump_line")` and, for editing actions, backed
by `Editor::jump_line` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
