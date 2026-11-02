# Inline Suggestions (Ghost Text)

A dimmed suggestion appears after the cursor as you pause typing, and you
accept it without opening a popup. It is off by default.

## Turning it on

Set `inline_completion` in your settings file (see
[Configuration](../configuration/index.md)):

| Value | Suggestions come from |
| ----- | --------------------- |
| `"off"` (default) | Nothing is requested or drawn. |
| `"lsp"` | A language server that offers `textDocument/inlineCompletion` (LSP 3.18), such as copilot-language-server. A server that does not advertise it is never asked. |
| `"ai"` | A fill-in-the-middle prompt sent through your configured AI provider. This **sends the text around the cursor to that provider**, so it is opt-in, and `ai = "off"` still turns it off. |

Any other value behaves as `"off"`.

## Using it

| Key | Action id | Does |
| --- | --------- | ---- |
| `Tab` | `inline_completion.accept` | Accept the whole suggestion. |
| `Ctrl+Right` | `inline_completion.accept_word` | Accept one word; the rest stays as a new suggestion. |
| `Esc` | `inline_completion.dismiss` | Dismiss it. |
| (palette) | `inline_completion.trigger` | Ask for a suggestion right now. |

With no suggestion visible these keys do what they always did. Typing or
moving the cursor discards a visible suggestion, and a reply that arrives for
an older buffer or cursor position is dropped silently.

Vix asks only after the buffer or cursor has been still for the idle delay,
with an editor focused, no completion popup, no selection and one caret, and
only when the rest of the line after the cursor is blank. Merely opening a
file requests nothing.

Only the first line is drawn (cut at the view's edge, with `…` when more lines
follow). Soft-wrapped views draw none, though a suggestion can still be
accepted.

See the specification at `crates/vix-lsp/spec/index.md` (section "Inline
(ghost-text) completion") and `crates/vix-ai-core/spec/index.md` (the prompt).

---

Vix™ and Vix IDE™ are trademarks.
