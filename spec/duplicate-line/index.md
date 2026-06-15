# Duplicate Line

Editor action `duplicate-line`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `duplicate-line` |
| snake | `duplicate_line` |
| Pascal | `DuplicateLine` |

Run it from the command palette or a key binding via the action id `duplicate_line`.
It is dispatched by `App::run_action("duplicate_line")` and, for editing actions, backed
by `Editor::duplicate_line` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
