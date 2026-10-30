# Cowork: edit together in real time

Cowork lets several people edit the same files at the same time, each in their
own terminal running Vix, and see each other's cursors. One person **hosts**:
their Vix owns the files and is the only one that writes to disk. Everyone
else **joins** with an invite.

Cowork is for pairing and mobbing. It is not a hosted service: there is no
server, account or cloud in between, and nothing is shared unless the host
chooses to share it.

## Get a build with Cowork

Cowork is off by default, so the standard build has no collaboration or
listening code. Build Vix with the `cowork` feature:

```sh
cargo install vix-tui --features cowork
# or, from a checkout:
cargo run --features cowork
```

Everyone in the session needs a build with Cowork. All the actions below live
under **Tools → Cowork** and in the command palette (`>cowork`).

## Host a session

1. Open the file you want to work on.
2. Choose **Tools → Cowork → Host Session…**. The prompt suggests
   `127.0.0.1:0`: this computer only, on any free port. To let people on your
   network join, type an address they can reach, for example `0.0.0.0:7070`.
   If they reach you at a different address (a VPN or a forwarded port), type
   `bind=advertise`, for example `0.0.0.0:7070=203.0.113.5:7070`.
3. Vix shows an **invite**, a line starting `vix-cowork:1:`. Send it to one
   person over a channel you trust. Each invite works **once**, for **10
   minutes**. **Tools → Cowork → New Invite** makes another.
4. Choose **Tools → Cowork → Share This Buffer** for each file to share.
   Nothing is shared until you do this. **Stop Sharing This Buffer** takes it
   back.

When someone uses an invite, Vix asks you before they get anything:

```text
Ada (192.0.2.10:52814) wants to join. Allow as: view, edit, terminal, or no
```

Press **Enter** to let them in **view-only** (the default), type `edit` to let
them type, `terminal` to let them type and also (once you allow it) type in a
terminal you share, or type `no` (or press **Esc**) to refuse.

## Join a session

1. Choose **Tools → Cowork → Join Session…** and paste the invite.
2. Wait for the host to let you in.
3. Each file the host shares opens in a tab named `cowork:` plus its path.
   These tabs belong to the host's session: **Save** asks the host to write
   the file, and nothing is written on your computer.

A view-only guest's shared tabs are read-only until the host grants edit.

## While you work together

- **Cursors.** Everyone's caret and selection show in the shared buffer, each
  person in their own color (chosen to stay readable on your theme). The
  status bar lists who else is in the file you are looking at.
- **Undo** (`Ctrl+Z`) and **Redo** undo only *your own* typing, never a
  collaborator's.
- **Follow…** pins your view to a participant: you jump to whatever file and
  line they are on. Any key or click of your own stops following. Choose
  **Follow…** again with an empty number to stop.
- **Peers** lists everyone, with their number and permission.
- **Permissions…** (host) changes a guest's level: type the guest's number and
  `view`, `edit` or `terminal`, for example `2 edit`.
- **Leave Session** ends your part. When the host leaves, the session ends for
  everyone. A guest who leaves keeps each shared file as an unsaved, untitled
  copy.

## Share the file tree

By default a guest sees only the files the host shares one by one. To let
guests browse and pick files themselves:

1. The host chooses **Tools → Cowork → Share File Tree** (choose it again to
   stop).
2. A guest chooses **Tools → Cowork → Shared Files…**. The host's files appear
   in the file finder; type to filter and press **Enter**. The file opens for
   you, in a background tab for the other guests, and in a background tab on
   the host, so the host always sees what is shared. At most 32 files are
   shared at once.

The list follows the project's `.gitignore` and leaves out hidden files.
Some files are **never** listed or shared, whatever anyone asks: `.env`
files, `.git/`, `.ssh/`, `.vix/` (project settings and the trusted
environment), cloud credentials, SSH keys, and key files such as `*.pem` and
`*.key`. A guest can only reach files inside the host's project folder; a
symbolic link that points outside it, or at a private file, is refused.

## Share your terminal

The host can show its integrated terminal (**Tools → Terminal**) to everyone
in the session, for example to watch a build or a test run together. The
terminal and its shell stay on the host's computer; guests see what it
shows.

1. Open the terminal, then choose **Tools → Cowork → Share Terminal**. Vix
   asks first, and warns that guests will see **everything** the terminal
   shows from then on, including any password, token or key printed in it.
   Press `y` to share, `n` or **Esc** to keep it private.
2. Guests are told that a terminal is shared. A guest chooses **Tools →
   Cowork → Shared Terminal** to show it, and `Ctrl+]` to hide it again.
3. A shared terminal is **read-only**. To let guests type, choose **Tools →
   Cowork → Allow Terminal Input** and press `y`. Only guests you gave the
   `terminal` level can then type, and anything they type runs in your
   shell, as you. Choose **Allow Terminal Input** again to make it read-only.
4. Choose **Share Terminal** again, or close the terminal, to stop sharing.
   A new share always starts read-only.

The guest sees the host's terminal at the host's size; a smaller window shows
its top-left part.

## Security

- **Encrypted, always.** Every byte after a short greeting is encrypted and
  authenticated with the Noise protocol. There is no unencrypted mode.
- **The invite is the key.** It holds a one-time secret and the host's public
  key, so only its holder can join, and only the real host can answer. Treat
  it like a password; it expires after 10 minutes and works once.
- **The host decides.** The host approves each person by name, gives
  view-only access unless they say otherwise, and is the only one who writes
  to disk. A guest cannot reach a path outside the project.
- **Limits.** A shared file can be at most 5 MB, and binary files are never
  shared.
- **The terminal is opt-in twice.** Sharing it and letting guests type in it
  are separate choices, each confirmed, and typing also needs the `terminal`
  level. Clear anything secret off the screen before you share it.
- Cowork has no relay or NAT traversal: the guest must be able to reach the
  host's address directly (the same network, a VPN, or a forwarded port).

## Not yet

- A dropped connection does not reconnect: the host sends a new invite.
- Searching the *contents* of the host's files and session chat are not
  available yet. Only the integrated terminal can be shared (one at a time),
  without its scrollback. Chat is planned to travel inside the same
  encrypted connection (see "Chat seam" in `spec/cowork/index.md`).

See the design and decision record in `spec/cowork/index.md` and the crate
specifications in `crates/vix-cowork/spec/index.md` and
`crates/vix-cowork-net/spec/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
