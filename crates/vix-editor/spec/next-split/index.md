# Next Split

Editor action `next-split`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `next-split` |
| snake | `next_split` |
| Pascal | `NextSplit` |

Run it from the command palette or a key binding via the action id `next_split`.
It is dispatched by `App::run_action("next_split")` and, for editing actions, backed
by `Editor::next_split` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
