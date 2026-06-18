# Toggle Diff Gutter

Editor action `toggle-diff-gutter`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `toggle-diff-gutter` |
| snake | `toggle_diff_gutter` |
| Pascal | `ToggleDiffGutter` |

Run it from the command palette or a key binding via the action id `toggle_diff_gutter`.
It is dispatched by `App::run_action("toggle_diff_gutter")` and, for editing actions, backed
by `Editor::toggle_diff_gutter` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
