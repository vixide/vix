# Paste

Editor action `paste`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `paste` |
| snake | `paste` |
| Pascal | `Paste` |

Run it from the command palette or a key binding via the action id `paste`.
It is dispatched by `App::run_action("paste")` and, for editing actions, backed
by `Editor::paste` in `vix-editor`. See `spec/actions/index.md` for the full
catalog.
