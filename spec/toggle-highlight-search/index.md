# Toggle Highlight Search

Editor action `toggle-highlight-search`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `toggle-highlight-search` |
| snake | `toggle_highlight_search` |
| Pascal | `ToggleHighlightSearch` |

Run it from the command palette or a key binding via the action id `toggle_highlight_search`.
It is dispatched by `App::run_action("toggle_highlight_search")` and, for editing actions, backed
by `Editor::toggle_highlight_search` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
