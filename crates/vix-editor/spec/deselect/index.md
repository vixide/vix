# Deselect

Editor action `deselect`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `deselect` |
| snake | `deselect` |
| Pascal | `Deselect` |

Run it from the command palette or a key binding via the action id `deselect`.
It is dispatched by `App::run_action("deselect")` and, for editing actions, backed
by `Editor::deselect` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
