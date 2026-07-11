# End

Editor action `end`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `end` |
| snake | `end` |
| Pascal | `End` |

Run it from the command palette or a key binding via the action id `end`.
It is dispatched by `App::run_action("end")` and, for editing actions, backed
by `Editor::end` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
