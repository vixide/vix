# Join Lines

Editor action `join-lines`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `join-lines` |
| snake | `join_lines` |
| Pascal | `JoinLines` |

Join the current line with the next, or — when the selection spans several lines
— join all of them into one. Adjacent lines are merged with a single space,
trimming the trailing space of each line and the leading space of the next.

Run it from the command palette or a key binding via the action id `join_lines`.
It is dispatched by `App::run_action("join_lines")` and backed by
`Editor::join_lines` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
