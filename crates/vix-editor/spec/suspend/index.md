# Suspend

Editor action `suspend`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `suspend` |
| snake | `suspend` |
| Pascal | `Suspend` |

Run it from the command palette or a key binding via the action id `suspend`.
It is dispatched by `App::run_action("suspend")` and, for editing actions, backed
by `Editor::suspend` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
