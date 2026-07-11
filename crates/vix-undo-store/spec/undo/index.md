# Undo

Editor action `undo`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `undo` |
| snake | `undo` |
| Pascal | `Undo` |

Run it from the command palette or a key binding via the action id `undo`.
It is dispatched by `App::run_action("undo")` and, for editing actions, backed
by `Editor::undo` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
