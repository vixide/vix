# Toggle Key Menu

Editor action `toggle-key-menu`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `toggle-key-menu` |
| snake | `toggle_key_menu` |
| Pascal | `ToggleKeyMenu` |

Run it from the command palette or a key binding via the action id `toggle_key_menu`.
It is dispatched by `App::run_action("toggle_key_menu")` and, for editing actions, backed
by `Editor::toggle_key_menu` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
