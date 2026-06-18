# Open File

Editor action `open-file`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `open-file` |
| snake | `open_file` |
| Pascal | `OpenFile` |

Run it from the command palette or a key binding via the action id `open_file`.
It is dispatched by `App::run_action("open_file")` and, for editing actions, backed
by `Editor::open_file` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
