# agent_panel

The **AI chat panel** is a persistent conversation surface for the configurable
assistant CLI (the `ai_command` setting; see `spec/ai/index.md`). Opened from
**AI → Chat…** (action `ai.chat`) or the command palette.

## Behavior

- A modal overlay with a scrollable transcript and a one-line input field.
- **Enter** sends the input line: it becomes the `{prompt}` and the prior
  conversation is fed on stdin as context, so the assistant has memory across
  turns. The input then clears and the panel is marked busy.
- Only one request runs at a time (shares the host's single `ai_replace` slot
  with the AI menu); a second send while busy is declined with `status.ai_busy`.
- The reply is captured in the background and appended as an assistant turn by
  `poll_ai_replace`; a failure or empty output appends an error turn.
- Opening the panel with an editor selection seeds the input with that text.

## Keys

| Key                   | Action                                  |
| --------------------- | --------------------------------------- |
| `Enter`               | Send the current input line             |
| `↑` / `↓`             | Scroll the transcript one line          |
| `PageUp` / `PageDown` | Scroll the transcript a page (10 lines) |
| `Alt+T`               | Open the last reply in a new editor tab |
| `Alt+C`               | Copy the last reply to the clipboard    |
| `Esc`                 | Close the panel                         |

## As implemented in Vix

Pure transcript state lives in the `ai_panel` module (`Panel`, `Turn`, `Role`)
with unit-tested word wrapping and a bottom-anchored, clamped scroll window. The
host (`app.rs`) owns `open_ai_panel`, `ai_panel_key`, and `ai_panel_send`, routes
the result through `AiDest::Panel`, and draws it with `ui::draw_ai_panel`.
