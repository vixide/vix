# AI Chat Panel

The AI chat panel is a persistent conversation surface for the assistant CLI
configured by the `ai_command` setting (Claude by default, but any CLI — Codex,
Mistral, a local `ollama` model — works). Open it from **AI → Chat…** or the
command palette (action `ai.chat`).

Unlike the one-shot AI menu commands (Summarize, Explain, …), the chat panel
keeps a running conversation: each reply is remembered and fed back as context on
the next turn, so you can ask follow-up questions.

## Using it

- **Type** a message on the input line at the bottom and press **Enter** to send.
- While a reply is in flight the title shows **Thinking…** and further input is
  declined until it returns (one request at a time, like the AI menu).
- If you open the panel with text selected in the editor, that selection seeds the
  input line — "ask about this" is one keystroke away.

### Keybindings

| Key                   | Action                                         |
| --------------------- | ---------------------------------------------- |
| `Enter`               | Send the current line                          |
| `↑` / `↓`             | Scroll the transcript one line                 |
| `PageUp` / `PageDown` | Scroll the transcript a page                   |
| `Alt+T`               | Open the most recent reply in a new editor tab |
| `Alt+C`               | Copy the most recent reply to the clipboard    |
| `Esc`                 | Close the panel                                |

## How it works

The panel reuses the shared `spawn_ai` machinery: your message becomes the
`{prompt}` in the `ai_command` template and the prior conversation is supplied on
stdin as context. The reply is captured in the background and appended to the
transcript when it arrives (the same async path the AI menu uses). See
`spec/ai/index.md` and the [configuration](../configuration/index.md) docs for
`ai_command`.

---

Vix™ and Vix IDE™ are trademarks.
