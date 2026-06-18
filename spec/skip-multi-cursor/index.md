# Skip Multi Cursor

Editor action `skip-multi-cursor`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `skip-multi-cursor` |
| snake | `skip_multi_cursor` |
| Pascal | `SkipMultiCursor` |

Run it from the command palette or a key binding via the action id `skip_multi_cursor`.
It is dispatched by `App::run_action("skip_multi_cursor")` and, for editing actions, backed
by `Editor::skip_multi_cursor` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
