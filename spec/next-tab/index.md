# Next Tab

Editor action `next-tab`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `next-tab` |
| snake | `next_tab` |
| Pascal | `NextTab` |

Run it from the command palette or a key binding via the action id `next_tab`.
It is dispatched by `App::run_action("next_tab")` and, for editing actions, backed
by `Editor::next_tab` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
