# Force Quit

Editor action `force-quit`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `force-quit` |
| snake | `force_quit` |
| Pascal | `ForceQuit` |

Run it from the command palette or a key binding via the action id `force_quit`.
It is dispatched by `App::run_action("force_quit")` and, for editing actions, backed
by `Editor::force_quit` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
