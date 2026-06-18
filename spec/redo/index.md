# Redo

Editor action `redo`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `redo` |
| snake | `redo` |
| Pascal | `Redo` |

Run it from the command palette or a key binding via the action id `redo`.
It is dispatched by `App::run_action("redo")` and, for editing actions, backed
by `Editor::redo` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
