# Scroll Up

Editor action `scroll-up`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `scroll-up` |
| snake | `scroll_up` |
| Pascal | `ScrollUp` |

Run it from the command palette or a key binding via the action id `scroll_up`.
It is dispatched by `App::run_action("scroll_up")` and, for editing actions, backed
by `Editor::scroll_up` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
