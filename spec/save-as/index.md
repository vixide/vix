# Save As

Editor action `save-as`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `save-as` |
| snake | `save_as` |
| Pascal | `SaveAs` |

Run it from the command palette or a key binding via the action id `save_as`.
It is dispatched by `App::run_action("save_as")` and, for editing actions, backed
by `Editor::save_as` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
