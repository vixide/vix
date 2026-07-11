# Scroll Down

Editor action `scroll-down`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `scroll-down` |
| snake | `scroll_down` |
| Pascal | `ScrollDown` |

Run it from the command palette or a key binding via the action id `scroll_down`.
It is dispatched by `App::run_action("scroll_down")` and, for editing actions, backed
by `Editor::scroll_down` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
