# IM (instant messaging)

The **IM** menu lets you talk about code without leaving Vix. It is a
code-aware chat rather than a general chat client: every message can point at
code — a `path:line` that opens on Enter, a shared selection, a shared cursor
position — and it never interrupts you. IM never opens, focuses or pops
anything up on its own; new messages raise a count in the status bar, and you
open the panel when you choose.

There are two kinds of conversation today:

- **Notes** — local, only you, always there. It works with no network and no
  account, so every menu item has somewhere to go. Use it as a scratch log of
  code references.
- **IRC** rooms and direct messages, through an account you configure. IRC is
  an open, vendor-neutral protocol; Vix speaks IRCv3 over TLS.

Session chat with the people in a Cowork session, threads anchored to lines,
and Matrix are planned (see `spec/instant-message/index.md`).

## The panel

**IM → Chat Panel** shows the panel beside the editor and gives it the
keyboard; choosing it again while the panel has the keyboard hides it. The
panel has the conversation list at the top, the messages of the selected
conversation, the input line and a key hint.

| Key | Action |
| --- | ------ |
| Enter | Send the input line; on a highlighted message with a code reference, open it |
| Tab / Shift+Tab | Next / previous conversation |
| Up / Down | From an empty input, move into the messages; Down past the newest returns to the input |
| Left, Right, Home, End, Backspace, Delete | Edit the input |
| Esc | Give the keyboard back to the editor (the panel stays shown) |

Shortcuts with Ctrl or Alt and the function keys still reach the menus and
the palette while the panel has the keyboard. A click in the panel gives it
the keyboard; a click on a conversation selects it. A paste goes into the
input as one line and is never sent by itself.

Each message is one line in reading order — `time author: text` — with the
time in your system's time zone. Unread counts and mentions are written as
text (`3 unread`, `@`), not shown by color alone. A nick in an IRC room shows
`(unverified)` unless the server confirmed its account.

## Commands in the input

| Command | Does |
| ------- | ---- |
| `/me TEXT` | Send an action (`* ana waves`) |
| `/join #ROOM` | Join a room |
| `/part [#ROOM] [REASON]` | Leave a room (the selected one by default) |
| `/msg NICK TEXT` | Message one person directly |
| `/nick NEW` | Change your nick |
| `/topic [TEXT]` | Show or set the room's topic |
| `/status available\|away\|busy` | Set your presence |
| `/clear` | Clear the conversation on screen |

An unknown command is an error and is never sent as text; the line stays so
you can fix it. To send text that starts with `/`, double it: `//etc/hosts`
sends `/etc/hosts`.

## Code references

Any `path:line`, `path:line:col` or `path:line-line` in a message is a code
reference. Highlight the message and press Enter to open the file there. A
reference opens only a file that exists inside the workspace root — never a
path outside it — and never runs anything. URLs are shown, not fetched.

- **IM → Share Selection** sends the selected lines as a snippet to the
  selected conversation. It first shows exactly what will be sent, and to
  whom, and sends only on `y` or Enter. A selection is limited to 50 lines.
- **IM → Share Location** sends the cursor's `path:line:col`.

Paths are relative to the workspace root, so the people you share with can
open them in their own checkout.

## Connecting to IRC

Add an account to `config.toml`:

```toml
[im]
nick = "ana"

[[im.accounts]]
name = "Libera"
server = "irc.libera.chat"
port = 6697
tls = true
channels = ["#vix"]
sasl_user = "ana"
```

**IM → Connect…** connects the account (with several, pick one from a list).
The password for SASL sign-in comes only from the environment variable
`VIX_IM_PASSWORD_<NAME>` — the account name upper-cased, other characters as
`_`, so `VIX_IM_PASSWORD_LIBERA` here. Vix never reads a password from a file,
never writes one, and never shows or logs it. Without the variable Vix signs
in without SASL and says so in the account's conversation.

Each account has a server conversation for connection news and errors.
**Join Room…**, **Leave Room** and **Direct Message…** act on the account of
the selected conversation. **Disconnect** closes it.

TLS is the default and the certificate is checked against the Mozilla root
store bundled with Vix. `tls = false` connects in plaintext and puts a warning
in the conversation.

## Status and notifications

- **IM → Status ▸ Available / Away / Busy** sets your presence. It is local:
  Vix does not broadcast it. **Busy** silences notifications.
- **IM → Notifications ▸ All Messages / Mentions Only / Off** sets when a new
  message notifies you. The default is **Mentions Only**: a message that
  `@`-mentions your nick, or any direct message.

A notification is a short note in the status line. The status bar's IM
segment (`IM 3 unread @`) shows the unread total, with `@` while a mention is
unread, whatever the notification setting. A message in the conversation you
have open on screen is read straight away. **IM → Next Unread** jumps to the
next conversation with unread messages, mentions first.

## History

By default messages live only in memory and are gone when Vix exits. With
`history = "disk"`, each conversation is written as JSON Lines under the
platform data directory (for example `~/.local/share/vix/im/`), never inside
the workspace, readable only by you. Messages older than `history_days` are
dropped when a conversation is loaded. **IM → Clear History** deletes the
selected conversation's messages and its file, after asking.

## Settings

| Setting | Default | Meaning |
| ------- | ------- | ------- |
| `im.nick` | `""` | Your nick; empty uses the operating system user name. |
| `im.notify` | `"mentions"` | `"all"`, `"mentions"` or `"off"`. |
| `im.history` | `"session"` | `"session"` (memory only) or `"disk"`. |
| `im.history_days` | `30` | Days of disk history to keep. |
| `im.blocked` | `[]` | Nicks whose messages and mentions are hidden. |
| `im.accounts` | `[]` | IRC accounts: `name`, `server`, `port` (6697), `tls` (true), `channels`, `sasl_user`. |

## Security

- Everything received is cleaned before it is stored or drawn: control
  characters, terminal escape sequences and the bidirectional override
  characters used in "Trojan Source" spoofing are removed. Text in any script,
  left-to-right or right-to-left, is otherwise shown as sent.
- Remote text never becomes a terminal hyperlink or any other escape
  sequence.
- Nothing is sent without you asking: no file names, no presence.
- Blocked nicks are hidden locally.

## Building without it

`im` is a Cargo capability feature, on by default and part of the `net` and
`full` umbrellas. Without it Vix opens no chat socket: Connect, Disconnect,
Join Room, Leave Room and Direct Message are hidden, and everything else —
the panel, Notes, sharing into Notes, status, notifications and history —
still works. See [Cargo features](../../spec/cargo-features/index.md).

## See also

- The design, `spec/instant-message/index.md`, and the crate specs
  `crates/vix-im/spec/index.md`, `crates/vix-im-panel/spec/index.md` and
  `crates/vix-im-irc/spec/index.md`.
