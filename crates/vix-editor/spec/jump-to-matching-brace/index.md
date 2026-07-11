# Jump To Matching Brace

Editor action `jump-to-matching-brace`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `jump-to-matching-brace` |
| snake | `jump_to_matching_brace` |
| Pascal | `JumpToMatchingBrace` |

Run it from the command palette or a key binding via the action id `jump_to_matching_brace`.
It is dispatched by `App::run_action("jump_to_matching_brace")` and, for editing actions, backed
by `Editor::jump_to_matching_brace` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.
