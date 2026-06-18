# Outdent Line

Editor action `outdent-line`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `outdent-line` |
| snake | `outdent_line` |
| Pascal | `OutdentLine` |

Run it from the command palette or a key binding via the action id `outdent_line`.
It is dispatched by `App::run_action("outdent_line")` and, for editing actions, backed
by `Editor::outdent_line` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
