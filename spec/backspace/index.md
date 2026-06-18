# Backspace

Editor action `backspace`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `backspace` |
| snake | `backspace` |
| Pascal | `Backspace` |

Run it from the command palette or a key binding via the action id `backspace`.
It is dispatched by `App::run_action("backspace")` and, for editing actions, backed
by `Editor::backspace` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
