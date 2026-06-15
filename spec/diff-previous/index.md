# Diff Previous

Editor action `diff-previous`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `diff-previous` |
| snake | `diff_previous` |
| Pascal | `DiffPrevious` |

Run it from the command palette or a key binding via the action id `diff_previous`.
It is dispatched by `App::run_action("diff_previous")` and, for editing actions, backed
by `Editor::diff_previous` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
