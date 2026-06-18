# Delete Word Right

Editor action `delete-word-right`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `delete-word-right` |
| snake | `delete_word_right` |
| Pascal | `DeleteWordRight` |

Run it from the command palette or a key binding via the action id `delete_word_right`.
It is dispatched by `App::run_action("delete_word_right")` and, for editing actions, backed
by `Editor::delete_word_right` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
