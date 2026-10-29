# Cowork — real-time collaborative editing

**Status:** implemented behind the opt-in `cowork` Cargo feature (2026-10-29):
T690–T696 are done, T697 (shared terminal) is not started. The task list
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
- Chat is out of scope for v1 (use the call you are already on); the wire
  format keeps room for it (see "Chat seam").
- Menu: **Tools → Cowork** holds Host Session…, New Invite, Share This Buffer,
  Stop Sharing This Buffer, Share File Tree, Join Session…, Shared Files…,
  Peers, Follow…, Permissions… and Leave Session (a submenu rather than a new
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

## Never shared (private paths)

`SharedPath::is_private` is the single list, applied to the host's own
shares, to file lists and to open requests: the directories `.git`, `.ssh`,
`.gnupg`, `.aws`, `.azure`, `.kube`, `.docker`, `.vix`; any name starting
`.env`; `.netrc`, `.npmrc`, `.pypirc`, `.pgpass`, `credentials`,
`credentials.json`; SSH keys (`id_rsa`, `id_dsa`, `id_ecdsa`, `id_ed25519`,
and their `.pub`); and the extensions `.pem`, `.key`, `.p12`, `.pfx`, `.jks`,
`.keystore`, `.kdbx`, `.gpg`. Matching ignores ASCII case.

## Chat seam

Session chat (IM, task T756 in the instant-messaging plan) will be a separate
message family inside the same encrypted, authenticated frames, so a Cowork
session gets chat with nothing more to set up or trust. The wire format
already leaves room for it:

- Every `wire::Message` has a `Channel` (`Control`, `Document`, `Presence`,
  `Workspace`); T756 adds `Channel::Chat` and its messages.
- Tags 48–63 are reserved for chat (`wire::RESERVED_CHAT_TAGS`). Today such a
  message decodes to `WireProblem::Reserved`, and the transport's reader
  skips it instead of closing the link (`CoworkError::is_reserved_message`),
  so a peer that speaks chat can share a session with one that does not.
  `Link::send_encoded` carries an already encoded payload, which is how the
  test proves the skip.
- Chat routing belongs in the session layer: the host will relay chat by
  channel, stamping the sender's verified peer id as it does for presence.

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
debugging, guest-side LSP.

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
