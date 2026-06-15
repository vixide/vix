# Find

Editor action `find`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `find` |
| snake | `find` |
| Pascal | `Find` |

Run it from the command palette or a key binding via the action id `find`.
It is dispatched by `App::run_action("find")` and, for editing actions, backed
by `Editor::find` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
