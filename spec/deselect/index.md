# Deselect

Editor action `deselect`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `deselect` |
| snake | `deselect` |
| Pascal | `Deselect` |

Run it from the command palette or a key binding via the action id `deselect`.
It is dispatched by `App::run_action("deselect")` and, for editing actions, backed
by `Editor::deselect` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
