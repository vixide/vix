# IM panel

The IM dock panel's state: selected conversation, message list, input line
and Enter-to-open code references, over [`vix-im`](../../vix-im/spec/index.md);
drawing-free.

**Status:** T750 (improvement plan Run S). Design:
[`spec/instant-message`](../../../spec/instant-message/index.md). The App
draws it (`tui/src/ui/`) and carries out its outcomes (T752).

## Shape

`Panel` holds the selected conversation's index, the focus (`Focus::Input`
or `Focus::Messages`), the highlighted message, the scroll offset, and the
input line with a byte cursor that always sits on a character boundary. It
reads and marks read through a `vix_im::Inbox` the App owns; it never sends,
opens or draws anything.

- Conversations: `select(inbox, id)`, `next_conversation`,
  `prev_conversation` (wrapping); each marks the newly selected
  conversation read and puts focus on the input. `clamp` keeps the
  selection valid after conversations are removed.
- Input: `insert_char` (control characters ignored), `insert_str` (pasted
  text, sanitized to one line), `backspace`, `delete`, `left`, `right`,
  `home`, `end`, `set_input`, `clear_input`.
- `submit()` returns `Submit::Send(text)`, `Submit::Command(cmd)`,
  `Submit::Error(err)` or `Submit::Nothing`. Text and commands clear the
  line; an error leaves it to be fixed; a blank line does nothing.
- Messages: `up` from an *empty* input enters the list at the newest
  message (in a non-empty input it does nothing); `down` past the newest
  returns to the input. `ensure_visible(inbox, height)` keeps the highlight
  (or, with the input focused, the newest message) on screen, through
  `vix-list-state`. `selected_code_ref(inbox)` is the highlighted
  message's reference (`vix_im::Message::code_ref`).

## Keys

`Panel::key(key, inbox)` returns an `Outcome`: `None`, `Submit(Submit)`,
`Open(CodeRef)`, `Selected(id)` or `Close`.

| Key | Action |
| --- | ------ |
| Enter | in the input, submit; on a message with a code reference, `Open` it |
| Tab / Shift+Tab | next / previous conversation |
| Up / Down | into and through the message list, back to the input |
| Left / Right / Home / End | move the cursor |
| Backspace / Delete | delete before / under the cursor |
| a character | insert it (moving focus to the input) |
| Esc | `Close`: focus returns to the editor |

`Open` carries the reference as written; the App must pass it through
`vix_im::resolve_in_root` and open it only inside the workspace root.
