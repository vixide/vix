# Clear Status

Editor action `clear-status`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `clear-status` |
| snake | `clear_status` |
| Pascal | `ClearStatus` |

Run it from the command palette or a key binding via the action id `clear_status`.
It is dispatched by `App::run_action("clear_status")` and, for editing actions, backed
by `Editor::clear_status` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
