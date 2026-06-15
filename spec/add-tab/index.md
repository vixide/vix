# Add Tab

Editor action `add-tab`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `add-tab` |
| snake | `add_tab` |
| Pascal | `AddTab` |

Run it from the command palette or a key binding via the action id `add_tab`.
It is dispatched by `App::run_action("add_tab")` and, for editing actions, backed
by `Editor::add_tab` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
