# Delete Word Left

Editor action `delete-word-left`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `delete-word-left` |
| snake | `delete_word_left` |
| Pascal | `DeleteWordLeft` |

Run it from the command palette or a key binding via the action id `delete_word_left`.
It is dispatched by `App::run_action("delete_word_left")` and, for editing actions, backed
by `Editor::delete_word_left` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
