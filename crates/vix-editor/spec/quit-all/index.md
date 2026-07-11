# Quit All

Editor action `quit-all`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `quit-all` |
| snake | `quit_all` |
| Pascal | `QuitAll` |

Run it from the command palette or a key binding via the action id `quit_all`.
It is dispatched by `App::run_action("quit_all")` and, for editing actions, backed
by `Editor::quit_all` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
