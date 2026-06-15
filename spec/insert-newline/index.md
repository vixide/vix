# Insert Newline

Editor action `insert-newline`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `insert-newline` |
| snake | `insert_newline` |
| Pascal | `InsertNewline` |

Run it from the command palette or a key binding via the action id `insert_newline`.
It is dispatched by `App::run_action("insert_newline")` and, for editing actions, backed
by `Editor::insert_newline` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
