# Indent Selection

Editor action `indent-selection`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `indent-selection` |
| snake | `indent_selection` |
| Pascal | `IndentSelection` |

Run it from the command palette or a key binding via the action id `indent_selection`.
It is dispatched by `App::run_action("indent_selection")` and, for editing actions, backed
by `Editor::indent_selection` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
