# Outdent Selection

Editor action `outdent-selection`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `outdent-selection` |
| snake | `outdent_selection` |
| Pascal | `OutdentSelection` |

Run it from the command palette or a key binding via the action id `outdent_selection`.
It is dispatched by `App::run_action("outdent_selection")` and, for editing actions, backed
by `Editor::outdent_selection` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
