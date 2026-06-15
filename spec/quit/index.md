# Quit

Editor action `quit`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `quit` |
| snake | `quit` |
| Pascal | `Quit` |

Run it from the command palette or a key binding via the action id `quit`.
It is dispatched by `App::run_action("quit")` and, for editing actions, backed
by `Editor::quit` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
