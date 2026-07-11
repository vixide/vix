# Autocomplete

Editor action `autocomplete`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `autocomplete` |
| snake | `autocomplete` |
| Pascal | `Autocomplete` |

Run it from the command palette or a key binding via the action id `autocomplete`.
It is dispatched by `App::run_action("autocomplete")` and, for editing actions, backed
by `Editor::autocomplete` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
