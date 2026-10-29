# IM core

The pure core of Vix's **IM** menu: conversations, messages, presence,
mentions, unread tracking, slash commands, code references, the
received-text sanitizer, anchored threads and JSONL history.

**Status:** T750 (improvement plan Run S). Design:
[`spec/instant-message`](../../../spec/instant-message/index.md). The panel
state over it is [`vix-im-panel`](../../vix-im-panel/spec/index.md).

No I/O, no threads, no clock: the App owns sockets, files and time and
hands this crate plain values (times are Unix milliseconds, UTC). Nothing
here returns user-facing text; results are enums the App translates.

## Model

- `ConversationId` (an App-chosen string), `ConversationKind` (`Session`,
  `Room`, `Direct`, `Notes`) and `Conversation` (id, kind, title, messages,
  `unread`, `first_unread`, `mentioned`, `topic`). Titles and topics are
  sanitized when set.
- `Message { author, time_utc, body, kind }` with `MessageKind` `Text`,
  `Action`, `Snippet { language, path, start_line, end_line, code }`,
  `Location { path, line, column }` and `System`. `Message::code_ref` is
  what Enter opens: a location's or snippet's own place, else the first
  reference in the body.
- `Presence` (`Available`, `Away`, `Busy`) and `NotifyPolicy` (`All`,
  `Mentions` — the default — `Off`), with their setting strings.
  `should_notify(policy, presence, is_mention)`: Busy and Off never notify,
  All always does, Mentions only on a mention.

## Inbox

`Inbox::new(nick, case_mapping)` holds the conversations. `push(id,
message)` sanitizes the message, drops it if the author is blocked
(`set_blocked`, compared under the case mapping), and returns a
`Delivery`:

- `Read` — your own message (which also marks the conversation read, since
  you are answering in it) or a `System` message; neither counts as unread.
- `Unread { mention }` — counted; sets the first-unread marker if clear.
  `mention` when the text `@`-mentions your nick, or the conversation is
  `Direct`.
- `Blocked`, `NoConversation` — nothing stored.

`Delivery::notifies(policy, presence)` applies `should_notify`. Also
`mark_read`, `next_unread(after)` (wraps; conversations with a mention
first), `total_unread`, `has_mention`, and a per-conversation cap
(`DEFAULT_MESSAGE_LIMIT`, oldest dropped, first-unread kept in step).

## Mentions and case mapping

`CaseMapping` is `Rfc1459` (default), `Ascii` or `StrictRfc1459`, read from
the server's `CASEMAPPING` by `from_isupport`. Folding applies to ASCII
only. `find_mentions(body)` returns each `@nick` with its byte range: it
starts at the beginning of the text or after a character that cannot be in
a nick (so an e-mail address is not a mention). Nick characters are ASCII
letters, digits and IRC's specials, plus any non-ASCII character that is
not white space or common punctuation, so combining marks and the
zero-width joiners stay inside a nick in any script.

## Code references

`find_refs(body)` finds `path:line`, `path:line:col` and `path:line-end`
words, less surrounding quotes, brackets (ASCII and the common CJK and
full-width ones) and trailing punctuation, and returns each byte range with
its `CodeRef`. The path must hold a letter and a `/`, `\` or `.`, so times
and ratios are not references; URLs are never references; lines and
columns start at 1; `end < line` is rejected. `CodeRef`'s `Display` writes
the same form back.

`resolve_in_root(root, ref)` is pure path logic: `.` dropped, `..` may not
climb above the root, an absolute path must normalize to somewhere under
the root, and the root itself is not a file. The caller checks existence
(and compares canonical paths if symbolic links matter).

## Slash commands

`parse_input(line)` returns `Input::Text` or `Input::Command`: `/me TEXT`,
`/join ROOM [KEY]`, `/part [ROOM] [REASON]` (the first word is a room only
with a channel prefix `#&+!`), `/msg NICK TEXT`, `/nick NEW`,
`/topic [TEXT]`, `/status available|away|busy`, `/clear`. Names ignore ASCII
case. `//text` is the text `/text`. Errors: `Unknown(name)` (never sent as
text), `MissingArgument(command)`, `InvalidStatus(value)`.

## Sanitizer

`sanitize` (one line) and `sanitize_multiline` (keeps `'\n'`, for a
snippet's code) implement the design spec's Security section:

- C0 controls, DEL and C1 controls are removed; a tab becomes four spaces;
  a line break becomes a space (or `'\n'` in the multiline form).
- `ESC` and the whole sequence it starts are removed: CSI, OSC (ending at
  `BEL` or `ST`), DCS/SOS/PM/APC (ending at `ST`), `nF` and two-character
  escapes. The C1 introducers (U+009B, U+009D, U+0090, …) are treated the
  same. A broken CSI stops at the first character outside its grammar
  rather than swallowing the rest of the message.
- The bidi embedding, override and isolate controls U+202A–U+202E and
  U+2066–U+2069 are removed. The marks U+200E/U+200F/U+061C are kept: they
  cannot reorder other text.
- Everything else, including U+200C/U+200D, is kept.

Property tests: the output holds no control (other than `'\n'` from the
multiline form), no `ESC`, no listed bidi control; sanitizing is
idempotent; printable ASCII, Arabic, Devanagari, Han and Hebrew text and
emoji ZWJ sequences pass unchanged. A fuzz target (`im_message`) checks the
same invariants over arbitrary input.

## Anchored threads

`Thread { id, path, anchor_text, context_before, context_after,
start_line, end_line, resolved, messages }` is stored as pretty JSON in
`.vix/im/threads/<id>.json` (`to_json`, `from_json`, `file_name`;
`is_valid_id` allows 1–64 ASCII letters, digits, `-`, `_`). Optional fields
default and unknown fields are ignored, so T665's saved line comments can
share the format. `from_json` and `reply` sanitize message text.
`Thread::new(id, path, lines, start, end)` captures the anchored lines and
`CONTEXT_LINES` (3) lines of context on each side.

`reanchor(thread, lines)` returns:

1. `Anchor::Exact(line)` — the anchored text verbatim (trailing white space
   ignored), the occurrence nearest the stored line;
2. `Anchor::Moved(line)` — the text changed, but the most non-blank context
   lines match here (at least two, or one when only one was stored; the
   end may have shifted by up to 20 lines); nearest wins a tie;
3. `Anchor::Outdated`.

`Thread::apply(anchor)` moves the stored range, keeping its length.
`Thread::covers(line)` says whether the stored range holds a line.

Two helpers for the App (T753): `thread::new_id(time_utc_ms, taken)` makes
an id from the time in base 36 (`t` + digits), adding `-2`, `-3`, … while
`taken` says the id is in use, so it always passes `is_valid_id`; and
`thread::step(lines, from, forward)` gives the next (or previous) line in a
set after `from`, wrapping around, for **Next Thread** / **Previous
Thread**.

## History

`history::encode`/`encode_line` write JSONL (one message object per line);
`history::decode` reads it, skipping blank or bad lines and sanitizing what
it reads; `history::prune(messages, now_ms, days)` drops messages older
than the cutoff and returns how many.
