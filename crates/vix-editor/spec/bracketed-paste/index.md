# Bracketed Paste

Pasting *into the terminal* — `Cmd+V` in Terminal.app or iTerm, a middle-click
paste, `Ctrl+Shift+V` — is not a Vix key binding: the terminal replays the
pasted text at the program on its input stream. Vix asks the terminal to
**bracket** those pastes (`EnableBracketedPaste`, restored on exit and around
suspend), so they arrive as one `Event::Paste(text)` instead of as one key event
per character, and `App::on_paste` handles them.

Why it matters — with the paste delivered as keystrokes:

- **Undo went one character at a time.** Every character was its own edit, so
  each undo removed a single character of the paste.
- **Auto-indent re-indented every pasted line.** A pasted newline was `Enter`,
  which carries the previous line's indentation, so an indented block walked
  further right with each line and closing braces landed indented.
- **Auto-pairing doubled brackets and quotes**, since each `(`, `[`, or `"` was
  a typed character.

`App::on_paste` routes the chunk:

| Target | Behavior |
| ------ | -------- |
| The editor, with nothing layered over it | The whole chunk is inserted as one edit at the cursor, replacing the selection — `Editor::paste_str` → `editor_core::Editor::paste_text`, the same path the `paste` action takes for the clipboard, so a terminal paste and `Ctrl+V` behave alike and both undo in one step |
| A prompt, the palette, the find bar, a panel, the explorer, a dock | Replayed as key events, which is what those inputs expect; `\r\n` counts as one line break |
| A read-only buffer | Refused, with the usual read-only status message |

`App::overlay_capturing_keys` decides which of the two applies; it mirrors the
conditions of the key-dispatch chain (`try_overlay_key`, `try_tool_dialog_key`,
`try_panel_key`, plus jump-label mode) and must be kept in step with it.

A terminal that does not support bracketed paste keeps sending keys, and Vix
keeps handling them as before — the paste simply is not atomic there.

See `crates/vix-editor/spec/paste/index.md` for the clipboard `paste` action and
`spec/index/index.md` for the project overview.
