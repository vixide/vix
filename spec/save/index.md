# Save

Editor action `save`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `save` |
| snake | `save` |
| Pascal | `Save` |

Run it from the command palette or a key binding via the action id `save`.
It is dispatched by `App::run_action("save")` and, for editing actions, backed
by `Editor::save` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
