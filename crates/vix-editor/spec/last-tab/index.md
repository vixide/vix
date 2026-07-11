# Last Tab

Editor action `last-tab`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `last-tab` |
| snake | `last_tab` |
| Pascal | `LastTab` |

Run it from the command palette or a key binding via the action id `last_tab`.
It is dispatched by `App::run_action("last_tab")` and, for editing actions, backed
by `Editor::last_tab` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
