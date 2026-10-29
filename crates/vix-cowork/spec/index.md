# Cowork core

Pure real-time collaboration core: a CRDT text document, peers, presence,
permissions and the wire messages. No I/O: every rule here is testable without
a socket or a terminal. The transport is `vix-cowork-net`; the design and the
decision record for the CRDT (Loro) are in
[`spec/cowork`](../../../spec/cowork/index.md).

## `SharedDoc`

One CRDT replica of one buffer (a Loro text container). The editor's rope
remains the source of truth for rendering; the document mirrors it.

- **Units.** Every position is a *character* (Unicode scalar value), the unit
  of `ropey` and `vix-editor-core`. No conversion at the bridge.
- `SharedDoc::host(me, text, cap)` starts a session document; it refuses text
  over `cap` bytes ([`DEFAULT_CAP_BYTES`] is 5 MB) and text containing NUL
  (binary). `from_snapshot` joins from the host's `snapshot()`.
- `edit(offset, delete, insert)` records a local edit the editor made and
  returns the update bytes to broadcast (or `None` for a no-op). It validates
  the range, the cap and NUL, and never echoes the edit back as a remote one.
- `apply_host_update(bytes)` (guest side) and `apply_guest_update(peer, bytes)`
  (host side) import an update and return the `BufferEdit`s — `(offset,
  delete, insert)` splices, valid in order — to replay on the buffer.
- `apply_guest_update` is the security door on the host: the peer must have
  `Action::Edit`; the blob must be an *incremental update* (a snapshot is
  refused, so a guest cannot replace the document); and every operation in it
  must be authored by that peer's own id (a forged host edit is refused).
  Bytes that do not decode are an error, never a panic. After the import the
  size cap is rechecked; on breach the call errors and the caller drops the
  peer.
- `undo()` / `redo()` are **per peer**: they reverse only this peer's edit
  groups, never a collaborator's text, and return the update to broadcast plus
  the buffer edits. The initial file load is not an undo step.
- `replace_all(text)` records an external reload (host-side) as a minimal
  diff rather than a delete-and-reinsert.
- `anchor_at(offset)` / `resolve(anchor)` give a position that follows its
  character through edits (a CRDT cursor), used for remote cursors. An
  `Anchor` is an opaque byte string; `Anchor::from_bytes` refuses bytes that do
  not decode.
- `transform_offset(pos, edits)` keeps the **local** cursor on its character
  across remote edits. An insertion exactly at the cursor leaves the cursor
  before it, so a collaborator typing at your caret does not drag it along.

## Peers and permissions

`PeerId` is the session identity and the CRDT peer id (host = 1, guests 2 and
up, never reused). `Permission` is `View` (default) < `Edit` < `EditTerminal`;
`Permission::allows(Action)` is the single decision point (`Read`, `Edit`,
`Save`, `Terminal`). Display names pass `sanitize_name`: control characters
(terminal escape sequences) are dropped and the length is capped.

## Paths

`SharedPath::parse` is the one door for a guest-named path. It is lexical:
empty, absolute (`/`, `\`, `C:`), `..`, control-character and over-long paths
are refused; `.`, repeated and trailing separators are normalized away. A
property test pins that an accepted path joined under a root never leaves it.
Symlink checks need I/O and live in the App's workspace sharing (T696).

`SharedPath::is_private` is the one "never shared" list (secrets, keys,
credentials, `.git`, `.vix` settings; the full list is in `spec/cowork`). It
is checked on the host's own `share`, on file lists and on open requests.

## Wire messages

`wire::Message` is the application payload inside one authenticated, encrypted
frame: `Hello`, `Welcome`, `Open`, `Close`, `Update`, `Presence`,
`SaveRequest`, `Saved`, `PermissionChanged`, `PeerJoined`, `PeerLeft`, `Leave`,
`Error`, and for workspace sharing (T696) `ListFiles`, `FileList` (at most
`MAX_LISTED_FILES` paths; a lying count allocates nothing) and `OpenRequest`.
`Message::channel` names each message's family (`Channel::Control`,
`Document`, `Presence`, `Workspace`); the module docs carry the tag table.
Tags 48–63 (`RESERVED_CHAT_TAGS`) are kept for session chat (T756): they
decode to `WireProblem::Reserved`, which `CoworkError::is_reserved_message`
recognizes so a receiver skips the message instead of dropping the peer. `Message::encode` and `wire::decode` are hand-written, length-checked
on every field, capped at `MAX_MESSAGE_BYTES`, and total: a property test
feeds arbitrary and mutated bytes and requires an error, not a panic. Paths in
messages go through `SharedPath::parse` on decode. The host stamps the real
peer id on every relayed `Presence`; the sender's value is ignored.

## Sessions

`HostSession` is the authority and `GuestSession` the mirror; both return the
messages to send as values. Beyond sharing, admitting, permissions, edits,
presence and saves:

- **Undo (T692).** `HostSession::undo`/`redo` (host's own edits; the update
  goes to every guest) and `GuestSession::undo`/`redo` (needs edit) revert
  only the caller's edit groups.
- **Workspace sharing (T696).** Off until `set_workspace_shared(true)`.
  While on, a guest with read access may send `ListFiles` (reported in
  `Handled::list_files`; answer with `file_list`, which drops private paths)
  and `OpenRequest` (reported in `Handled::open` unless private; the host
  confines it on disk, then `share`s it or `reopen_for`s an already shared
  one). While off, both are refused with `ErrorCode::PermissionDenied` and the
  guest stays connected. `refuse_to` builds a refusal for a request the host
  could not carry out. Guests see `GuestEvent::Files`.

## Presence

`PresenceBoard` holds participants' names and each peer's latest cursor and
resolves them against a `SharedDoc` into `RemoteCursor`s (caret, optional
selection, color). Reports from peers that never joined are dropped.
`peer_color(peer, background)` picks a stable palette color per peer and shifts
it lighter or darker until it keeps a WCAG contrast ratio of at least 3.0
against the theme background (tested on dark, light, black, white and mid-gray
backgrounds). `Follow` pins the viewport to a peer and unpins on any local
scroll or when that peer leaves; `PresenceBoard::location` gives the followed
peer's latest buffer and caret.

## Tests

Unit tests per module; property tests for the codec, paths, and an N-replica
convergence run (`crates/vix-cowork/tests/converge.rs`): a host and three guests edit and undo
concurrently over delayed per-link queues, and every replica's CRDT text and
its replay-only plain buffer must be identical at the end.
