# Unhighlight Search

Editor action `unhighlight-search`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `unhighlight-search` |
| snake | `unhighlight_search` |
| Pascal | `UnhighlightSearch` |

Run it from the command palette or a key binding via the action id `unhighlight_search`.
It is dispatched by `App::run_action("unhighlight_search")` and, for editing actions, backed
by `Editor::unhighlight_search` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
