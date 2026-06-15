# Copy

Editor action `copy`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `copy` |
| snake | `copy` |
| Pascal | `Copy` |

Run it from the command palette or a key binding via the action id `copy`.
It is dispatched by `App::run_action("copy")` and, for editing actions, backed
by `Editor::copy` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
