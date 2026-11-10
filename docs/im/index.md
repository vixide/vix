# IM (instant messaging)

The **IM** menu lets you talk about code without leaving Vix. It is a
code-aware chat rather than a general chat client: every message can point at
code — a `path:line` that opens on Enter, a shared selection, a shared cursor
position — and it never interrupts you. IM never opens, focuses or pops
anything up on its own; new messages raise a count in the status bar, and you
open the panel when you choose.

There are three kinds of conversation today:

- **Notes** — local, only you, always there. It works with no network and no
  account, so every menu item has somewhere to go. Use it as a scratch log of
  code references.
- **Session** — the people in your Cowork session (see below). It appears
  when a session starts and is gone when it ends; there is nothing to set up.
- **IRC** rooms and direct messages, through an account you configure. IRC is
  an open, vendor-neutral protocol; Vix speaks IRCv3 over TLS.

Beside conversations there are **threads anchored to lines** of your files
(see below), which need no network either. Matrix is planned (see
`spec/instant-message/index.md`).

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
| `/follow [N]` | In a Cowork session, follow participant `N`; with no number, stop |
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

## Threads on lines

A thread is a short conversation attached to a line, or a range of lines, of
a file: a review note, a question for whoever picks the code up next.

- **IM → Comment on Line…** asks for a comment. On a line that already has a
  thread it adds a reply; otherwise it starts a thread on the selected lines,
  or on the cursor's line. The file must be saved inside the workspace.
- A line with a thread shows `◆` in the gutter beside its line number, or
  `◇` once the thread is resolved.
- **IM → Threads…** lists every thread in the workspace — `path:line`, the
  number of messages and the first one — with the thread under the cursor
  highlighted and its messages shown below. **Enter** opens the file at the
  thread, **r** resolves or reopens it, **Esc** closes the list.
- **IM → Next Thread** and **Previous Thread** move through the threads in
  the current file, wrapping around, and show the thread's first message in
  the status line.
- **IM → Resolve or Reopen Thread** toggles the thread on the cursor's line.

A thread follows its text as you edit: add lines above it and its mark moves
down with it; change the line itself and the surrounding lines still place
it. If both the text and its surroundings are gone, the thread is
*outdated*: it has no mark, and the list says so.

Threads are stored one JSON file each in `.vix/im/threads/` at the
workspace root, written as soon as you comment, reply, resolve or reopen. The
line a thread sits on is updated when you save the file. Commit the directory
to share threads with your team through Git; threads that arrive that way
show up as gutter marks, and nothing pops up. Message text in a thread file is
cleaned like any received message, and a thread whose path leads outside the
workspace is ignored.

The gutter has one sign per line. When several apply, the debugger's
stopped line (`▶`) comes first, then a breakpoint (`●`), then a thread, then
the git change or coverage bar (`▎`).

## Session chat (Cowork)

When you host or join a [Cowork](../cowork/index.md) session, a **Session**
conversation appears in the list. Nothing opens: like every other
conversation it raises the unread count, and you open the panel when you
choose. It needs no account and no server — its lines travel inside the
session's own encrypted, authenticated connection — and it is dropped, with
its messages, when the session ends. It is never written to disk history.

- **Who said it.** Every line shows the sender's identity as the session
  verified it: the name the host confirmed when the person joined, and the
  number the session gave them (`ana #2`; the host is always `#1`). The host
  stamps it, so nobody can speak as someone else.
- **Who is where.** While the Session conversation is selected, the panel
  lists everyone above the messages, each with the shared file their cursor is
  in (or the session notes), and `(you)`, `(host)`, `(muted)` and
  `(following)` written out.
- **Code.** Share Selection and Share Location work here too. A guest who
  presses Enter on a reference opens the host's file if it is already shared
  in, or asks the host for it (the host must have shared its file tree).
- **Follow.** `/follow 2` follows participant 2 from buffer to buffer, as
  **Tools → Cowork → Follow…** does; `/follow` alone, or any key or click in
  your own view, stops.
- **Moderation (host).** **Tools → Cowork → Mute in Chat…** stops relaying a
  guest's lines (asking again unmutes); **Remove Guest…** ends their part in
  the session. Everyone sees a line in the conversation when either happens,
  and when someone joins or leaves.
- Anyone in the session can talk, including view-only guests.

**Session notes.** **Tools → Cowork → Session Notes** opens one Markdown
buffer shared by everyone in the session — Zed's channel notes, kept for the
length of a session. It never opens on its own. The host and guests with
edit rights write it; view-only guests read it. It is not a file: Save does
nothing (copy the text into a file to keep it), and when the session ends it
stays behind as an unsaved buffer.

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

## Connecting to Matrix

Matrix needs a Vix built with the opt-in `matrix` feature
(`cargo install vix-tui --features matrix`) and the separate `vix-matrix`
helper (`cargo install vix-matrix`), which does the Matrix work, including
end-to-end encryption, in its own process. Vix finds the helper beside the
`vix` program, then on `PATH`, or at `im.matrix_helper` when you set it. Add
an account:

```toml
[[im.accounts]]
name = "Home"
protocol = "matrix"
homeserver = "https://matrix.org"
user = "@ana:matrix.org"
```

**IM → Connect…** starts the helper and signs in. The first time, the
password comes from `VIX_IM_PASSWORD_<NAME>` (`VIX_IM_PASSWORD_HOME` here),
as for IRC. Vix hands it to the helper once, over a pipe, never on a command
line. The helper then saves the session in your system keyring, and later
connects resume it without a password. The encrypted message store lives
under the platform data directory (for example
`~/.local/share/vix/matrix/Home/`), never in the workspace.

Every room you have joined shows up as a conversation; direct-message rooms
show as direct conversations. People are shown by their Matrix user id
(`@bob:matrix.org`), not their display name, because display names are not
unique. A message from a device its owner has not cross-signed is marked
`(unverified)`, like an IRC nick the server didn't vouch for. A message this
device has no key for (for example one sent before it signed in) shows as a
system line saying it can't be decrypted. **Join Room…** takes a room alias
(`#vix:matrix.org`) or id, and **Leave Room** and `/me` work as for IRC.

Not yet: verifying devices (emoji or QR), restoring older messages from key
backup, starting a new direct conversation, `/nick`, `/topic`, threads,
reactions, edits, files and images, and single sign-on. The `vix-matrix`
helper is not yet in the release archives, Homebrew, npm or the Linux
packages. Set `VIX_MATRIX_LOG` to a file path to keep the helper's warnings;
it never logs messages, tokens or keys.

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
| `im.accounts` | `[]` | Accounts: `name` and `protocol` (`"irc"`, the default, or `"matrix"`); IRC: `server`, `port` (6697), `tls` (true), `channels`, `sasl_user`; Matrix: `homeserver`, `user`. |
| `im.matrix_helper` | `""` | The `vix-matrix` helper's path; empty looks beside `vix`, then on `PATH`. |

## Security

- Everything received is cleaned before it is stored or drawn: control
  characters, terminal escape sequences and the bidirectional override
  characters used in "Trojan Source" spoofing are removed. Text in any script,
  left-to-right or right-to-left, is otherwise shown as sent.
- Remote text never becomes a terminal hyperlink or any other escape
  sequence.
- Nothing is sent without you asking: no file names, no presence. In a
  Cowork session the other participants see the shared file your cursor is
  in, as Cowork itself already shows them.
- Blocked nicks are hidden locally.

## Building without it

`im` is a Cargo capability feature, on by default and part of the `net` and
`full` umbrellas. Without it Vix opens no chat socket: Connect, Disconnect,
Join Room, Leave Room and Direct Message are hidden, and everything else —
the panel, Notes, sharing into Notes, threads on lines, status,
notifications and history — still works. See [Cargo features](../../spec/cargo-features/index.md).

## See also

- The design, `spec/instant-message/index.md`, and the crate specs
  `crates/vix-im/spec/index.md`, `crates/vix-im-panel/spec/index.md` and
  `crates/vix-im-irc/spec/index.md`.
