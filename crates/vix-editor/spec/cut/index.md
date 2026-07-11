# Cut

Editor action `cut`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `cut` |
| snake | `cut` |
| Pascal | `Cut` |

Run it from the command palette or a key binding via the action id `cut`.
It is dispatched by `App::run_action("cut")` and, for editing actions, backed
by `Editor::cut` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
