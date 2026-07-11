# Diff Next

Editor action `diff-next`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `diff-next` |
| snake | `diff_next` |
| Pascal | `DiffNext` |

Run it from the command palette or a key binding via the action id `diff_next`.
It is dispatched by `App::run_action("diff_next")` and, for editing actions, backed
by `Editor::diff_next` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
