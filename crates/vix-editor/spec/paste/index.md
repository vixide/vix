# Paste

Editor action `paste`.

| Form | Identifier |
| ---- | ---------- |
| kebab | `paste` |
| snake | `paste` |
| Pascal | `Paste` |

Run it from the command palette or a key binding via the action id `paste`.
It is dispatched by `App::run_action("paste")` and, for editing actions, backed
by `Editor::paste` in `editor_core`. See `crates/vix-editor-core/spec/index.md` for the full
catalog.

The clipboard text is inserted by `editor_core::Editor::paste_text`: one edit
(one undo step), the selection replaced, and the block re-indented to the
cursor. A paste made *in the terminal* (`Cmd+V`) takes the same path — see
`crates/vix-editor/spec/bracketed-paste/index.md`.
