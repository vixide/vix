# Save

Editor action `save`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `save` |
| snake | `save` |
| Pascal | `Save` |

Run it from the command palette or a key binding via the action id `save`.
It is dispatched by `App::run_action("save")` and, for editing actions, backed
by `Editor::save` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.

A tab whose path is a synthetic Cowork path (`cowork:<path>`,
`REMOTE_PATH_PREFIX`, a buffer shared in from someone else's session) is never
written by `Editor::save_active` or `save_active_as`: both refuse with
`PermissionDenied`, and the App asks the session's host to save instead
(`spec/cowork`).
