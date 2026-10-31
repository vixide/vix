# Cowork — real-time collaborative editing

**Status:** implemented behind the opt-in `cowork` Cargo feature (2026-10-29):
T690–T696 are done; T697 (shared terminal, read-only by default) landed
2026-10-30, and so did IM's session chat, roster and session notes over the
same link (T756, T759; see "Session chat" below). The task list
records what each task left out. User guide: `docs/cowork/index.md`. Tasks
T690–T697 in
[`tasks.md`](../../tasks.md). Supersedes the "Collaborative editing" entry in
the Ideas backlog.

*Sources: the author's knowledge of the tools below; no live web research was
done for this draft. Verify crate versions and APIs before the T690 spike.*

## Goal

Several people edit the same Vix buffers at the same time, each in their own
terminal, and see each other's cursors. Cowork is for pairing and mobbing. It
is not a hosted service and it does not replace git.

## Prior art

| Tool | Model | Lesson for Vix |
| ---- | ----- | -------------- |
| VS Code Live Share | Host owns the workspace; guests join by link; follow mode; shared terminals and servers | Host-authoritative workspace is simple and safe. Follow mode and read-only guests matter. |
| JetBrains Code With Me | Host IDE, guests use a thin client; permission levels | Permission tiers (read-only, edit, full) and an explicit consent prompt per guest. |
| Zed | CRDT buffers; channels; follow; shared projects | Per-user undo; presence is a first-class stream. |
| Emacs `crdt.el` / `collab-mode` | CRDT over a small server; text-mode friendly | Proof a terminal editor can do this with cursors rendered as overlays. |
| Neovim `instant.nvim`, Atom Teletype | CRDT (WebRTC in Teletype) | Peer-to-peer is nice, but NAT traversal is the cost. |

## Design decisions to settle first (T690)

1. **Sync algorithm.** CRDT, not OT: no central transform server, and offline
   edits merge. Candidates, all Rust: `loro`, `automerge`, `diamond-types`,
   `yrs`. The spike scores them on text-only performance, memory on a 1 MB
   file, maturity, license (Vix is GPL-3.0-only compatible) and
   `forbid(unsafe_code)` in our own code (dependencies may use unsafe).
2. **Topology.** Start **host-relayed**: the host listens (or uses a tiny
   relay), guests connect. Peer-to-peer is a later option. Cowork never
   requires a public server we operate.
3. **Scope of sharing.** Phase 1: shared *open buffers* by path relative to
   the host root. Phase 2: shared file tree and search. Phase 3: shared
   terminal (read-only by default).
4. **Authority for the disk.** The host is the only writer to disk. A guest's
   Save asks the host to save. This removes write conflicts and fits the
   existing external-change guard.
5. **Undo.** Per-user undo stack over the CRDT, so Ctrl-Z never reverts a
   collaborator's text. Persistent undo (`vix-undo-store`) stays host-local.

## Security model

- Joining needs an invite token. The host confirms each guest by name.
- Transport is encrypted and authenticated (Noise via `snow`, or TLS with a
  pinned key). Tokens are short-lived and single-use.
- Guests get a permission level: **view**, **edit**, or **edit+terminal**.
  Default is view.
- Path confinement: every guest-supplied path is resolved against the host
  root and rejected if it escapes (the same checks as `vix-fileops`).
- Never share: the trust-gated project environment (T667), secrets, DB
  passwords, the AI provider keys, or settings files.
- Everything is off by default and behind a Cargo feature (`cowork`), so the
  default build gains no network code.

## Architecture

- `vix-cowork` — pure: document model wrapper, peer ids, presence state, op
  encoding, permission checks. No I/O. Unit and property tested with a
  loopback of N replicas.
- `vix-cowork-net` — transport: framing, handshake, reconnect. Behind the
  `cowork` feature.
- App shell — a Cowork menu, a peers panel, remote cursor and selection
  overlays in `ui.rs`, and an `Editor` hook that turns local edits into ops
  and applies remote ops without moving the local cursor.

## Presence and UX

- Each peer gets a stable color from the theme palette (accessible contrast,
  checked against T203's themes). Remote cursors render as a colored cell;
  remote selections as a tinted range; the name appears in the status line and
  a peers panel.
- **Follow mode:** pin the viewport to a peer; any local scroll unpins.
- Chat was out of scope for v1; it landed with IM (T756) as a message family
  in the same encrypted frames (see "Session chat").
- Menu: **Tools → Cowork** holds Host Session…, New Invite, Share This Buffer,
  Stop Sharing This Buffer, Share File Tree, Share Terminal, Allow Terminal
  Input, Join Session…, Shared Files…, Shared Terminal, Peers, Session Notes,
  Mute in Chat…, Remove Guest…, Follow…, Permissions… and Leave Session (a
  submenu rather than a new
  top-level menu, so the menu bar's Alt mnemonics stay put). Each is also a
  `Cowork: …` palette command. All text goes through `t!` in every locale.
  The actions are `cowork.*` and are hidden, like the menu, in a build without
  the `cowork` feature.

## App behavior (T692, T693, T695, T696)

The App shell (`tui/src/app/cowork.rs`, with `cowork_off.rs` stand-ins) is
glue over `vix-cowork`, `vix-cowork-net` and `vix_editor_core::remote`:

- **Hosting.** Host Session asks for a bind address (default `127.0.0.1:0`,
  this machine only; `bind=advertise` when guests reach the host through
  another address) and shows a single-use invite valid for 10 minutes. Each
  authenticated guest raises a consent prompt naming it and its address;
  Enter admits it **view-only**, `edit` or `terminal` raise the level, and
  anything else or Esc declines. Nothing is sent to a guest before consent.
- **Sharing is opt-in per file.** Share This Buffer shares the active saved
  project file; a private path (below) is refused with a status message.
- **Guest buffers** open as tabs whose path is the synthetic
  `cowork:<path>` (`vix_editor::REMOTE_PATH_PREFIX`). `vix-editor` refuses to
  save such a tab, and Save asks the host instead (`SaveRequest`), which
  writes with its normal save path and answers `Saved`. A view-only guest's
  tabs are read-only and become editable when the host grants edit. Leaving
  turns them into untitled, unsaved local copies.
- **The pump.** Once per event-loop tick: finish a join, offer a waiting
  guest for consent, feed local edits (the editor's edit log) to the CRDT,
  then apply remote edits, then publish and draw presence, then follow.
  Local edits go first because the log's offsets refer to the buffer as it
  was before any remote edit.
- **Undo (T692).** While a buffer is shared (its edit log is on) the editor's
  own Undo and Redo do nothing, because remote edits make that history's
  offsets stale. `edit.undo`/`edit.redo` on a shared buffer go to the
  session's per-participant stack (`HostSession::undo`/`redo`,
  `GuestSession::undo`/`redo`) and the result is replayed on the buffer, so
  Ctrl+Z never reverts a collaborator's text.
- **Presence (T693).** Remote carets and selections are drawn by
  `vix-editor-core` in contrast-checked peer colors; the status bar lists the
  other participants in the active shared buffer. **Follow…** pins the view to
  a participant (their buffer and caret); any local key, click or scroll
  unpins it, and so does that participant leaving.
- **Failure is closed.** If a shared buffer and its document disagree, the
  host stops sharing that buffer; a guest that cannot apply the host's update
  leaves the session.

## Workspace sharing (T696)

Off by default. **Share File Tree** (host, a toggle) lets guests list the
project and ask for files; until then a guest sees only what the host shares
one by one.

- **List.** `ListFiles` is answered with the host's file-finder index (it
  honors `.gitignore`, skips hidden files and `target`/`node_modules`),
  limited to files under the root and to `MAX_LISTED_FILES`, with every
  private path removed. A guest picks from it in the palette's file finder
  (**Shared Files…**).
- **Open.** `OpenRequest` names a `SharedPath`. The session refuses it when
  the tree is not shared or the path is private; the App then resolves it on
  disk and refuses it unless its canonical path (symlinks followed) is a
  regular file inside the canonical root and is still not private, is not an
  image, and opens as an unlocked text buffer. An accepted file opens in a
  background tab on the host (the host always sees what is shared) and is
  shared with everyone. At most 32 buffers are shared at once (further open
  requests are refused), and the host reuses its file list for 5 seconds, so
  a guest cannot make the host open every file or walk the project in a
  loop.
- **Disk writes stay with the host.** Guests still only ask to save.
- **Not done:** searching file *contents* across the shared tree.

## Shared terminal (T697)

The host may show one of its integrated terminals (`tools.terminal`, a
`vix-terminal` PTY) to the session: with several open (T631), the tab that
was active when the host confirmed sharing, pinned by its id. Switching tabs
on the host never changes what guests see, guests' keystrokes go only to that
tab, closing another tab leaves the share alone, and closing the shared tab
ends it; the host's tab strip marks it. Nothing about it is shared until the
host acts, and guests may type only after a second, separate act.

- **Screen, not PTY.** The shell and its PTY stay on the host; the PTY is
  local-only, as the remote-editing authority design (T710) keeps it, and
  nothing here routes through that authority. Guests get the terminal's *rendered screen*: the
  host sends `vt100`'s escape-sequence rendering of the whole screen
  (`TerminalScreen`), then once per tick the difference from the screen the
  guests last saw (`TerminalOutput`, nothing when idle), and the whole screen
  again after a resize and to a guest that joins later. A guest replays them
  into its own `vt100` parser and draws it like the integrated terminal, at
  the host's size (clipped by a smaller view). Scrollback is not shared.
- **Opt-in, with a warning.** **Share Terminal** (`cowork.share_terminal`, a
  host toggle; it needs the terminal open) asks first. The confirmation says
  how many guests will see it and that everything the terminal shows from
  then on, including any password, token or key printed in it, is visible to
  them; `y` shares, `n`/Esc does not. Choosing it again, or closing the
  terminal, stops sharing (`TerminalClose`).
- **Read-only by default.** Every new share starts read-only. **Allow
  Terminal Input** (`cowork.terminal_input`, a host toggle) asks again (the
  confirmation says a guest can then run any command as the host) and tells
  guests (`TerminalInputAllowed`). Typing then also needs the guest's
  `terminal` permission level (`Permission::EditTerminal`, admitted with
  `terminal` or set with Permissions…). The host session checks all three
  on every `TerminalKeys` message; a refused one is answered with
  `PermissionDenied` and the guest stays connected. Accepted keystrokes go to
  the PTY as they are (`Terminal::send_bytes`).
- **Guest view.** A guest is told when a terminal is shared but the view is
  never opened over its work: **Shared Terminal** (`cowork.terminal`) shows
  it and `Ctrl+]` hides it. While shown it takes every key (and a paste,
  replayed as keys); keys are encoded as the integrated terminal encodes
  them and sent only when this guest may type, otherwise the status says the
  terminal is read-only. The title says which.
- **Limits.** A screen or difference is at most `MAX_TERMINAL_BYTES` (4 MB;
  a larger one stops the share), a keystroke burst at most
  `MAX_TERMINAL_KEYS` (4 KB), and a shared terminal at most
  `MAX_TERMINAL_ROWS` x `MAX_TERMINAL_COLS` (500 x 1000); a guest refuses a
  larger size on decode, so a hostile host cannot make it allocate a huge
  screen.
- **Live screen only.** The host's scrollback (T631) is not shared: what is
  sent is always the live screen, so the host scrolling back never reaches
  guests.
- **Not done:** sharing more than one terminal at once, the guests' own
  scrollback, mouse reporting, and a per-guest input grant (input is allowed
  for every guest with the `terminal` level at once).

## Never shared (private paths)

`SharedPath::is_private` is the single list, applied to the host's own
shares, to file lists and to open requests: the directories `.git`, `.ssh`,
`.gnupg`, `.aws`, `.azure`, `.kube`, `.docker`, `.vix`; any name starting
`.env`; `.netrc`, `.npmrc`, `.pypirc`, `.pgpass`, `credentials`,
`credentials.json`; SSH keys (`id_rsa`, `id_dsa`, `id_ecdsa`, `id_ed25519`,
and their `.pub`); and the extensions `.pem`, `.key`, `.p12`, `.pfx`, `.jks`,
`.keystore`, `.kdbx`, `.gpg`. Matching ignores ASCII case.

## Session chat (T756) and session notes (T759)

Session chat (`spec/instant-message`) is a separate message family inside the
same encrypted, authenticated frames, so a Cowork session has chat with
nothing more to set up or trust.

- **Wire.** `Channel::Chat`: `Chat(ChatLine)` (tag 48: sender, UTC time,
  body, and a kind of text, action, location or snippet, with paths as
  `SharedPath`s and bodies and code capped at `MAX_CHAT_BYTES`, 16 KB) and
  `ChatMuted` (tag 49). Tags 50–63 stay reserved
  (`wire::RESERVED_CHAT_TAGS`): such a message decodes to
  `WireProblem::Reserved` and the transport's reader skips it instead of
  closing the link, so a later chat message can share a session with this
  build. A build from before T756 skips tags 48 and 49 the same way.
- **Routing.** The host relays a guest's line to every other guest (never
  back to the sender), stamping the connection's verified peer id over
  whatever the sender claimed, as it does for presence. Any permission level
  may chat; view-only guests too.
- **Identity.** A line's author is shown as the name the host confirmed at
  consent plus the peer number (`ana #2`, the host `#1`). The number keeps a
  guest from passing as someone with the same name, or as you.
- **Host moderation.** **Mute in Chat…** (`cowork.mute`, host, by number;
  again to unmute) makes the host refuse and not relay a guest's lines and
  tells the guest (`ChatMuted`); muting does not change what it may edit.
  **Remove Guest…** (`cowork.remove`, host, by number) sends
  `Error { Removed }`, tells the others the guest left, and closes its
  connection; it needs a new invite to come back.
- **The conversation.** The App adds a `Session` conversation to the IM inbox
  when a session starts (host or guest) and drops it when the session ends.
  It is never written to disk history and never opens or focuses the panel;
  a line raises the unread count. Joins, leaves and mutes are system lines.
  `/follow N` in the panel follows participant `N` (as **Follow…**).
- **Presence in the panel (T759).** While the session conversation is
  selected the panel lists every participant with the shared file their
  cursor is in (or the notes) and `you`, `host`, `muted`, `following` as
  words.
- **Session notes (T759).** One Markdown document per session, created empty
  with the session (`SharedPath::session_notes`, `.vix/cowork/notes.md`). It
  syncs through the same CRDT path as a shared buffer (edits, per-user undo,
  presence, follow). **Session Notes** (`cowork.notes`) opens it in a
  `cowork:` tab on the host or a guest; it never opens on its own. The host
  and edit guests write it, view guests read it. It is not a file: the path is
  private, so no file can be shared, listed or opened in its place; Save does
  nothing (a guest's save request for it is refused); closing its tab does not
  end it; and when the session ends it is left as an unsaved buffer.
- **References.** For a guest, Enter on a code reference in the session
  conversation opens the host's shared-in buffer at that line, or asks the
  host to share the file (T696 rules: the file tree must be shared).

## Risks

- A CRDT doubles memory per character; cap shared buffers (default 5 MB) and
  refuse binary files.
- Rope/CRDT impedance: Vix's `ropey` buffer must stay the source of truth for
  rendering. The CRDT needs a bridge, and the spike must prove cursor
  stability under remote inserts.
- Reconnect after sleep/network loss must resync without duplicating text.
- Encoding (T661) and CRLF handling must agree on the host.

## Out of scope for v1

Hosted relay service, voice/video, audio, cloud accounts, collaborative
debugging, guest-side LSP. For chat: history beyond the session, private
messages between two participants, and sharing anchored threads (T753) over
the session (they are shared through Git).

## Decision record (T690, 2026-10-29)

**Decision: `loro` (1.17.x, MIT).** Spike: a scratch crate outside the
workspace, release build, Apple-silicon macOS, rustc 1.99. A 1 MB source file
(about 16,000 lines) is loaded as one insert, then 20,000 random single-char
inserts and deletes are applied (one transaction each), the document is
snapshotted, and a second replica imports the snapshot. The `ropey` row is the
buffer-only baseline.

| Candidate | License | Load | 20k edits | Snapshot | Peak RSS | Verdict |
| --------- | ------- | ---- | --------- | -------- | -------- | ------- |
| `ropey` (baseline) | MIT | n/a | 3 ms | n/a | 6 MB | the buffer; stays the source of truth |
| `loro` 1.17 | MIT | 3 ms | 29 ms | 353 KB | 26 MB | **chosen** |
| `diamond-types` 1.0 | ISC | 0.5 ms | 6 ms | 79 KB | 25 MB | fastest, but see below |
| `yrs` 0.28 | MIT | 0.4 ms | 3,156 ms | 1.7 MB | 32 MB | the 1 MB block splits are slow |
| `automerge` 0.12 | MIT | 290 ms | 243 ms | 184 KB | 364 MB | memory far above the 5 MB cap's budget |

All four converged (the importing replica matched byte for byte) and all four
are permissively licensed, so none is excluded by `deny.toml`.

Why Loro over the faster `diamond-types`: the bridge, not raw speed, is the
cost that matters. Loro ships exactly the three things Cowork needs and
`diamond-types` ships none of them:

1. **Stable cursors** (`LoroText::get_cursor` / `LoroDoc::get_cursor_pos`).
   The throwaway test put a cursor before "world" in `hello world`, had a
   second replica insert `>>> ` at the start and ` <<<` at the end, imported
   that, and the cursor moved from 6 to 10 (it followed its character). This
   is both the local-cursor-stability and the remote-presence primitive.
2. **Change events as text deltas** (`LoroDoc::subscribe`, `Diff::Text` with
   retain, insert and delete runs, tagged `Import` or `Local`). A remote
   import therefore turns into ordinary `(offset, delete, insert)` buffer
   edits with no diffing of whole strings.
3. **Per-peer undo** (`UndoManager`). With peer A's `A:` and peer B's `B:`
   both inserted, A's undo removed only `A:`.

Loro also offers `LoroText::update`, which diffs a whole new string into the
document, the right tool for an external disk reload on the host.

Rejected: `automerge` (an order of magnitude more memory and a slow load), `yrs` (3 s for 20k edits in a
large block; relative positions exist but the slow path is the common one),
`diamond-types` (best numbers, but a text-only library with no public stable-cursor,
per-peer undo or change-event API that the spike found: we would write all
three).

Build impact: `loro` adds about 90 transitive crates, all MIT or Apache-2.0
(checked with `cargo deny`), and compiles in about 30 seconds in release.
It is a dependency of `vix-cowork` only, which is not in the default build
(see "Build gating").

### Settled design points

- **Position unit.** Loro text positions are Unicode scalar values, the same
  unit as `ropey` char indices and `vix-editor-core` offsets. No conversion.
- **Source of truth.** The `ropey` buffer renders; the CRDT mirrors it. Local
  edits are pushed into the CRDT as they happen; remote edits arrive as
  deltas and are applied to the buffer.
- **Peer ids.** Each session peer gets a random 64-bit CRDT peer id assigned
  by the host at join (never reused), so two guests cannot collide.
- **Size cap.** 5 MB per shared buffer by default, refused at share time.
- **Wire format.** Loro's binary updates, carried inside the encrypted frames
  of `vix-cowork-net`; the application protocol adds a version byte and a
  message tag.
