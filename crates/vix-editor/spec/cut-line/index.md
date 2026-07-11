# Cut Line

Editor action `cut-line`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `cut-line` |
| snake | `cut_line` |
| Pascal | `CutLine` |

Run it from the command palette or a key binding via the action id `cut_line`.
It is dispatched by `App::run_action("cut_line")` and, for editing actions, backed
by `Editor::cut_line` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
