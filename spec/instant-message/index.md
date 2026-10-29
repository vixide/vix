# IM — instant messaging for people editing together

**Status:** specified 2026-10-29; implementation tracked as T750–T759 in
[`tasks.md`](../../tasks.md). T750–T752, T754 and T755 are implemented: the
core crates, the IRC client, the IM menu, panel and App wiring (`im` Cargo
feature), status-bar unread and mention indicators with the notify policy,
and disk history. Not yet: anchored threads (T753; **Comment on Line…** and
**Threads…** say so), session chat over Cowork (T756), Matrix (T758), and
presence and session notes (T759). User guide: [`docs/im`](../../docs/im/index.md). Related: [Cowork](../cowork/index.md)
(real-time co-editing, whose v1 explicitly leaves chat out) and
[Worldwide](../worldwide/index.md).

## Goal

Let people who work on the same code talk about it without leaving Vix, and
make every message able to point at code: a file and line that opens on
Enter, a shared selection, a comment that stays attached to its line.

An **IM** top-level menu holds it. IM is a *code-aware* chat, not a general
chat client: it does the few things that are better inside an editor and
bridges to an open protocol for the rest.

## Prior art and what it teaches

| Editor / tool | Chat | Lesson for Vix |
| --- | --- | --- |
| Zed | Removed channel chat in Sept 2025 ([#37789](https://github.com/zed-industries/zed/pull/37789)) after guest chat kept popping open during a live demo; channel **notes** (a shared Markdown document) stayed | Free-floating chat inside an editor is noisy and under-used. Never open on its own; shared notes outlive chat. |
| Replit | Retired repl chat for [Inline Threads](https://docs.replit.com/replit-workspace/workspace-features/inline-threads): threads on a selection, with an inbox, next/previous and resolve | Threads anchored to code are the "chat" that survives. |
| JetBrains Code With Me | Session chat, private messages, calls, follow; [sunsetting](https://blog.jetbrains.com/platform/2026/03/sunsetting-code-with-me/), final shutdown Q1 2027 | Session-scoped chat during a share is the core case; calls are not worth building. |
| VS Code Live Share | Built to be used next to Slack/Teams/Discord; a light session chat was added on demand ([VS 2022](https://devblogs.microsoft.com/visualstudio/integrated-chat-in-live-share-for-visual-studio-2022)) | Long-lived team chat lives in the team's own tool. |
| Atom Teletype | No chat; [sunset](https://github.blog/2022-06-08-sunsetting-atom/) with Atom in 2022 | Co-editing without any back-channel pushes people to a separate app. |
| Emacs ERC/rcirc, [ement.el](https://elpa.gnu.org/devel/doc/ement.html); iamb (Rust, Matrix) | IRC and Matrix clients inside or beside the editor; ement.el leaves E2EE to the Pantalaimon proxy | An editor can be a capable client of an open protocol; Matrix E2EE is a large separate job. |

*Sources: a web survey on 2026-10-29 (links above). Unverified there: the
current state of Live Share's built-in chat, and CodeTogether, Tuple and
Floobits. T750 re-checks anything the code leans on.*

## Assessment

1. **Session chat is the core.** When people are editing together (Cowork),
   a chat bound to that session needs no account, no server and no new
   trust: it rides the Cowork link that is already encrypted and
   authenticated.
2. **Code-anchored comments are the most valuable kind of message.** A
   comment on a line that moves with the code and can be resolved is what
   reviews, pairing and handovers need. It also works with no network at
   all (stored in the workspace) and is shared through Git or Cowork.
3. **Never interrupt.** IM never opens, focuses or pops anything on its
   own. New messages raise an unread count in the status bar; a mention
   adds a marker. The user opens the panel.
4. **Shared session notes** — one Markdown scratch buffer per Cowork
   session, edited by everyone — cost almost nothing once the Cowork CRDT
   exists, and are what Zed kept when it dropped chat.
5. **Bridge, don't rebuild, for team chat.** Teams already have a chat
   home. Vix connects to an **open** protocol so nobody is locked to one
   vendor or country: IRC (IRCv3) first, because it is small, text-only and
   needs no SDK; Matrix later, because end-to-end encryption needs a large
   SDK and is its own project. Proprietary services (Slack, Teams, Discord)
   are out of scope; they can be reached through a bridge on their side.
6. **Skip:** voice and video, file transfer, a Vix-hosted server, read
   receipts, typing indicators over bridges.

## Concepts

- **Conversation.** A named stream of messages. Kinds: `Session` (the
  Cowork session), `Room` (an IRC channel or later a Matrix room),
  `Direct` (one other person), and `Notes` (local, only you; also the
  offline fallback so every menu item works with no connection).
- **Message.** Author, time (UTC, shown in the user's locale and time
  zone), body, and kind: `Text`, `Action` (`/me`), `Snippet` (code with a
  language, a path and a line range), `Location` (a path, line and
  column), `System` (joins, leaves, errors). Received text is
  **sanitized** before it is stored or drawn (see Security).
- **Code reference.** Any `path:line`, `path:line:col` or
  `path:line-line` inside a message body is a reference; when the path
  resolves inside the workspace, Enter on it opens the file there. A
  reference never opens a file outside the workspace root.
- **Anchored thread.** A thread of messages attached to a line range in a
  file, stored in `.vix/im/threads/*.json` with the anchored text and a
  few lines of context so it can re-find its line after edits (exact
  match, then nearest fuzzy match of the context, then "outdated").
  Threads can be resolved and reopened. The format is shared with T665's
  saved line comments; whichever lands first defines it.
- **Presence.** `Available`, `Away`, `Busy` (do not disturb), and, in a
  Cowork session, the file each peer is in (from Cowork presence).
- **Mention.** `@nick` in a message. Mentions of you raise a notification
  unless you are Busy.
- **Unread.** Per-conversation count and a first-unread marker; the status
  bar shows the total and a mention marker.

## The IM menu

Top level, between **Git** and **Org**. Every item has an action id, a
`t!` label in every locale, and a palette entry.

| Item | Action id | Notes |
| --- | --- | --- |
| Chat Panel | `im.panel` | Toggle the IM dock panel: conversation list, messages, input line. |
| Connect… | `im.connect` | Pick a configured account (IRC server) and connect. |
| Disconnect | `im.disconnect` | Close the active account. |
| Join Room… | `im.join` | Prompt for a room (`#channel`). |
| Leave Room | `im.leave` | Leave the selected room. |
| Direct Message… | `im.direct` | Prompt for a nick; opens a `Direct` conversation. |
| Next Unread | `im.next_unread` | Jump to the next conversation with unread messages. |
| Share Selection | `im.share_selection` | Send the selection as a `Snippet` to the active conversation (asks first; shows what will be sent). |
| Share Location | `im.share_location` | Send `path:line:col` of the cursor. |
| Comment on Line… | `im.comment_line` | Start or reply to the anchored thread at the cursor. |
| Threads… | `im.threads` | List anchored threads in the workspace; Enter jumps, `r` resolves. |
| Status ▸ Available / Away / Busy | `im.status_available`, `im.status_away`, `im.status_busy` | Busy silences notifications. |
| Notifications ▸ All / Mentions / Off | `im.notify_all`, `im.notify_mentions`, `im.notify_off` | Default: Mentions. |
| Clear History | `im.clear_history` | Delete stored history of the selected conversation after a confirm. |

**Panel keys:** `Enter` sends (or, on a message with a code reference,
opens it); `Tab`/`Shift+Tab` cycle conversations; `Up` in an empty input
edits nothing — it moves into the message list; `Esc` returns focus to the
editor. Slash commands in the input: `/me`, `/join`, `/part`, `/msg`,
`/nick`, `/topic`, `/status`, `/clear`. Unknown slash commands are an
error, never sent as text.

## Settings

```toml
[im]
nick = "ana"                 # default: the OS user name
notify = "mentions"          # "all" | "mentions" | "off"
history = "session"          # "session" (memory only) | "disk"
history_days = 30            # retention when history = "disk"
blocked = ["spammer"]        # nicks whose messages are hidden

[[im.accounts]]
name = "Libera"
server = "irc.libera.chat"
port = 6697                  # TLS only; plaintext needs tls = false
tls = true
channels = ["#vix"]
sasl_user = "ana"            # password from the env var VIX_IM_PASSWORD_<NAME>, never from the file
```

History is in memory only by default. `history = "disk"` writes JSONL per
conversation under the platform data directory (not the workspace), pruned
to `history_days`. Passwords are never read from or written to a file
Vix owns.

## Architecture

- **`vix-im`** — pure core, no I/O: `Conversation`, `Message`,
  `MessageKind`, `Presence`, the code-reference parser, mention detection,
  unread tracking, slash-command parsing, the sanitizer, the anchored-thread
  model and re-anchoring, and history JSONL encode/decode. Property tests
  for the sanitizer and the reference parser.
- **`vix-im-panel`** — panel state (selected conversation, scroll, input
  line, unread markers) over `vix-im`, rendering-free like the other
  `*-panel` crates; drawing lives in `tui/src/ui/`.
- **`vix-im-irc`** — IRCv3 client: line codec (RFC 1459/2812 framing,
  IRCv3 message tags, `server-time`, `CAP LS 302`, SASL PLAIN, `PING`,
  CTCP `ACTION`), splitting long messages on UTF-8 boundaries within the
  512-byte limit, and a connection on a background thread over TLS
  (`rustls` with `webpki-roots`) that exchanges events with the App over
  channels. It depends only on its own event types, not on `vix-im`; the
  App converts.
- **Session transport** — once Cowork's link (`vix-cowork-net`) is on
  `main`, session chat is a separate message type inside the same
  encrypted frames, so a Cowork session has chat with nothing to set up.
- **App shell** — `tui/src/app/im.rs` (every build) owns the inbox, the
  actions, the panel's keys, sharing, notifications and history;
  `tui/src/app/im_net.rs` (feature `im`) owns the IRC connections and the
  event pump that converts `vix-im-irc` events into `vix-im` messages, with
  `im_off.rs` stand-ins; `tui/src/ui/im.rs` draws the dock panel and the IM
  dialogs (account picker, share and clear confirmations).
- **Cargo feature** `im`, part of `net` and `full` (see
  [cargo-features](../cargo-features/index.md)): it gates `vix-im-irc` and
  every socket. Without it, the IM menu still offers Notes and anchored
  threads, which need no network.

## Security

- **Sanitize everything received.** Strip C0 and C1 control characters
  (except tab, shown as spaces), `ESC` and every escape sequence, and the
  bidirectional override and embedding controls (U+202A–U+202E,
  U+2066–U+2069) that enable "Trojan Source" spoofing. Text is otherwise
  drawn as-is, in any script, left-to-right or right-to-left. The zero-width
  joiner and non-joiner (U+200D, U+200C) are kept: Persian, the Indic
  scripts and emoji sequences need them. Remote text is never turned into
  an OSC 8 hyperlink or any other escape sequence (compare
  [CVE-2026-47090](https://osv.dev/vulnerability/CVE-2026-47090), where
  untrusted values in OSC 8 links allowed clipboard writes).
- **No auto-open.** A code reference opens only on an explicit Enter,
  only inside the workspace root, and never runs anything. URLs are shown,
  not fetched.
- **Sharing is explicit.** Share Selection shows exactly what will be
  sent and to whom before sending. Nothing is sent automatically —
  no file names, no presence outside a Cowork session.
- **TLS by default** for IRC, certificate verified against
  `webpki-roots`; plaintext requires `tls = false` and shows a warning in
  the conversation.
- **Identity.** IRC nicks are not authenticated; the panel marks a nick
  as `(unverified)` unless the server confirms a SASL account via
  `account-tag`. Session peers show the identity the Cowork handshake
  verified, not a name they chose; the host can mute or remove a peer.
- **Blocking** hides a nick's messages and mentions locally.
- **No logging** of message bodies or credentials in Vix's own logs.

## Worldwide

IRC and Matrix are open, vendor-neutral protocols available in every
country. Message text is Unicode in any script; times are stored in UTC
and shown with the user's locale and time zone; nicks are compared with
IRC case-mapping as the server declares it (`CASEMAPPING`), not ASCII
lowercasing alone. Every label and message goes through `t!` in all 15
locales.

## Accessibility

Each message renders as one logical line with author, time and text in
reading order, so a screen reader reads it naturally; unread and mention
state is text (`3 unread`, `@`), not color alone.

## Out of scope

Voice and video, file transfer, a Vix-run chat server, Slack/Teams/Discord
clients, read receipts, typing indicators, end-to-end encryption on IRC
(Matrix E2EE is T758).

## Tasks

T750–T759 in [`tasks.md`](../../tasks.md): T750 research check and
`vix-im` core; T751 `vix-im-irc`; T752 the IM menu, panel and App wiring;
T753 anchored threads; T754 notifications and status bar; T755 disk
history; T756 session chat over Cowork (after Cowork's link lands on
`main`); T757 docs and site; T758 Matrix bridge (spike first); T759
follow mode, presence and shared session notes (after Cowork presence).
