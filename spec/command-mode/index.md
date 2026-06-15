# Command Mode

Editor action `command-mode`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `command-mode` |
| snake | `command_mode` |
| Pascal | `CommandMode` |

Run it from the command palette or a key binding via the action id `command_mode`.
It is dispatched by `App::run_action("command_mode")` and, for editing actions, backed
by `Editor::command_mode` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
