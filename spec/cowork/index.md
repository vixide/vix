# Cowork — real-time collaborative editing

**Status:** proposed (2026-10-28). Planning only; no code. Tasks T690–T697 in
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
- Chat is out of scope for v1 (use the call you are already on).
- Menu: **Cowork → Host Session…, Join Session…, Leave, Peers, Follow,
  Permissions**. All text goes through `t!`.

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
