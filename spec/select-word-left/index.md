# Select Word Left

Editor action `select-word-left`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `select-word-left` |
| snake | `select_word_left` |
| Pascal | `SelectWordLeft` |

Run it from the command palette or a key binding via the action id `select_word_left`.
It is dispatched by `App::run_action("select_word_left")` and, for editing actions, backed
by `Editor::select_word_left` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
