# Toggle Overwrite Mode

Editor action `toggle-overwrite-mode`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `toggle-overwrite-mode` |
| snake | `toggle_overwrite_mode` |
| Pascal | `ToggleOverwriteMode` |

Run it from the command palette or a key binding via the action id `toggle_overwrite_mode`.
It is dispatched by `App::run_action("toggle_overwrite_mode")` and, for editing actions, backed
by `Editor::toggle_overwrite_mode` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
