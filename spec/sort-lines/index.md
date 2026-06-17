# Sort Lines

Editor action `sort-lines`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `sort-lines` |
| snake | `sort_lines` |
| Pascal | `SortLines` |

Sort the selected lines ascending (byte order), or the whole buffer when nothing
is selected. The sort is stable, so equal lines keep their relative order.

Run it from the command palette or a key binding via the action id `sort_lines`.
It is dispatched by `App::run_action("sort_lines")` and backed by
`Editor::sort_lines` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
