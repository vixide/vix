# Select Word Right

Editor action `select-word-right`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `select-word-right` |
| snake | `select_word_right` |
| Pascal | `SelectWordRight` |

Run it from the command palette or a key binding via the action id `select_word_right`.
It is dispatched by `App::run_action("select_word_right")` and, for editing actions, backed
by `Editor::select_word_right` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
