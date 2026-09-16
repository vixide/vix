# AI Chat Panel

The AI chat panel is a persistent conversation surface for the configured
assistant: by default the CLI named by `ai_command` (Claude by default, but any
CLI — Codex, Mistral, a local `ollama` model — works), or a provider's HTTP API
called directly when `ai_provider` is set to `"anthropic"`, `"openai"`, or
`"ollama"` (see [configuration](../configuration/index.md) and
`crates/vix-ai-core/spec/index.md`). Open it from **AI → Chat…** or the command
palette (action `ai.chat`).

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

The panel reuses the shared `spawn_ai_cmd` machinery: your message becomes the
`{prompt}` in the `ai_command` template (or the HTTP request's system
instruction, for a direct provider) and the prior conversation is supplied as
the input text. The reply is captured in the background and appended to the
transcript when it arrives (the same async path the AI menu uses). See
`crates/vix-ai-panel/spec/ai/index.md` and the [configuration](../configuration/index.md) docs for
`ai_command`/`ai_provider`.

---

Vix™ and Vix IDE™ are trademarks.
