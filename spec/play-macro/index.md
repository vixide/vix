# Play Macro

Editor action `play-macro`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `play-macro` |
| snake | `play_macro` |
| Pascal | `PlayMacro` |

Run it from the command palette or a key binding via the action id `play_macro`.
It is dispatched by `App::run_action("play_macro")` and, for editing actions, backed
by `Editor::play_macro` in `editor_core`. See `spec/actions/index.md` for the full
catalog.
