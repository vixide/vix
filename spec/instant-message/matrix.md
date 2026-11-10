# IM — the Matrix bridge decision (T758 spike)

**Status:** spike done 2026-11-10; the bridge itself is T793 in
[`tasks.md`](../../tasks.md). Parent spec: [IM](index.md).

**Decision.** Matrix runs **out of process**, in a small Vix-owned helper
binary, `vix-matrix`, built on `matrix-sdk` with its SQLite store and real
Olm/Megolm end-to-end encryption (vodozemac). The helper lives in its own
Cargo workspace (like [`gui/`](../../gui/Cargo.toml)), so it has its own
lockfile, and talks to Vix over JSON lines on stdin/stdout. Inside Vix the
bridge is a new crate, `vix-im-matrix`, behind a new Cargo feature `matrix`.
Linking `matrix-sdk` into `vix` itself is rejected for now: the two cannot
share a lockfile today (see "The SQLite `links` conflict"). Pantalaimon and
iamb are rejected too.

## What a Matrix client needs

To read a team's rooms, a client needs E2EE. Element creates private rooms
and direct messages encrypted by default, so a client that only handles
plaintext rooms sees little of a real team. And E2EE needs **persistent
crypto state**: the device's Olm identity, its Megolm inbound sessions and
the device lists all have to survive a restart. Without them every launch is
a new device that cannot decrypt anything sent before it logged in, and
every other member sees another unverified device. So "E2EE with an
in-memory store" doesn't count as a working option.

## Options

| Option | E2EE | New crates in Vix's lockfile | Verdict |
| --- | --- | --- | --- |
| A. `matrix-sdk` in process, SQLite store | Yes (vodozemac) | 95 new names, 16 second versions | **Blocked**: `libsqlite3-sys` 0.38 vs 0.37, see below |
| A′. `matrix-sdk` in process, memory store | Not usable: keys lost on exit | 87 new names, 14 second versions | Rejected: a new device every launch |
| B. Pantalaimon (local E2EE proxy) + plain HTTP | Yes, via libolm | 0 | Rejected: archived, deprecated crypto, Python |
| C. iamb out of process | — | 0 | Rejected: no interface to drive |
| D. Plain client-server API over `ureq` | No | 0 | Possible fallback for plaintext rooms only |
| **E. `vix-matrix` helper process (`matrix-sdk` + SQLite) over stdio** | **Yes (vodozemac)** | **0 in Vix**; the helper has its own lockfile | **Chosen** |

### A. `matrix-sdk` in process

Measured with `matrix-sdk` **0.19.1** (the latest release, 2026-09-18) in a
probe crate outside the repo. The probe was a compile-only "log in with a
password, sync once, list joined rooms" binary that was never run against a
server. It used `default-features = false` with `e2e-encryption`, with and
without `sqlite`, and Vix's own `rustls` line (`ring`, `std`, `tls12`),
installed as the process default provider. `matrix-sdk` enables
`reqwest/rustls-no-provider` itself and only its default
`rustls-aws-lc-rs` feature pulls in aws-lc, so with default features off
the tree has `ring` 0.17.14, no aws-lc.

| Measure (aarch64-apple-darwin, Apple M4 Max, 16 cores, rustc 1.99.1) | Memory store | SQLite store |
| --- | --- | --- |
| Packages in the normal and build dependency tree | 267 | 280 |
| Already in Vix's `Cargo.lock` at the same version | 166 | 169 |
| New package versions | 101 | 111 |
| New crate names | 87 | 95 |
| Second versions of crates Vix already has | 14 | 16 |
| Clean release build with Vix's profile (`opt-level = "z"`, thin LTO, strip) | 54 s | 59 s |
| Clean release build with the default profile (plus strip) | 79 s | 84 s |
| Stripped binary, Vix's profile | 19.9 MB | 25.6 MB |
| Same, minus a tokio + rustls baseline (1.0 MB) | **+18.9 MB** | **+24.6 MB** |

The second versions are mostly the RustCrypto generation split:
`matrix-sdk-crypto`/vodozemac use `aead` 0.6, `aes` 0.9, `cipher` 0.5,
`chacha20poly1305` 0.11, `curve25519-dalek` 5, `poly1305` 0.9 and
`universal-hash` 0.6, while Vix has the previous release of each. The rest
are `itertools` 0.15, `rand_core` 0.9, `rand_xoshiro` 0.7, `wide` 0.7,
`safe_arch` 0.7, `inout` 0.2 and, with SQLite, `hashlink` 0.12 and
`libsqlite3-sys` 0.38. The new names include `ruma` (and its 5
sub-crates), the 6 `matrix-sdk-*` crates, `vodozemac`, `reqwest`, `hyper`,
`h2`, `tower`/`tower-http`, `oauth2`, `prost`, `imbl` and `eyeball`. The
macOS SQLite build links the system `libsqlite3.dylib`, so
`bundled-sqlite`, which a portable release would want, adds more
(*unmeasured*).

- **MSRV.** Every `matrix-sdk-*` crate declares `rust-version = 1.96`. No
  crate in the tree needs more, so Vix's floor (1.97) is fine.
- **`cargo deny check licenses advisories bans`** with a copy of Vix's
  `deny.toml`: **licenses ok**, **bans ok**, **advisories failed** on one
  new finding. `anymap2` 0.13.0, a direct dependency of `matrix-sdk`, is
  unmaintained (RUSTSEC-2026-0319) and no fixed version exists, so Vix would
  need a new `ignore` entry. With SQLite, the existing RUSTSEC-2026-0459
  ignore (SQLite < 3.54 via `libsqlite3-sys` < 0.39) matches
  `libsqlite3-sys` 0.38.2 as well.
- **Stability.** `matrix-sdk` is pre-1.0. It released 0.9 through 0.19 between
  2024-12-18 and 2026-09-16, about one breaking minor every two to three
  months.

#### The SQLite `links` conflict

`matrix-sdk-sqlite` 0.19.1 depends on `rusqlite` 0.40.2, which requires
`libsqlite3-sys` ^0.38.2. Vix's DB workbench uses `sqlx` 0.9.0, whose
`sqlx-sqlite` requires `libsqlite3-sys` `>=0.30.1, <0.38.0`; the lockfile
has 0.37.0. Both declare `links = "sqlite3"`, and Cargo allows only one
package per `links` value in a dependency graph. A probe that depends on
both, with Vix's exact `sqlx` features, fails to resolve:

```text
package `libsqlite3-sys` links to the native library `sqlite3`, but it
conflicts with a previous package which links to `sqlite3` as well:
package `libsqlite3-sys v0.38.2`
    ... required by `rusqlite v0.40.2` ... `matrix-sdk-sqlite v0.19.1`
```

It fails the same way when `matrix-sdk` is an **optional** dependency.
`Cargo.lock` resolves every optional dependency, so putting the SDK behind a
feature does not help: one optional line anywhere in the workspace breaks
the whole build. The two would agree only if `sqlx` and `rusqlite` happened
to require the same `libsqlite3-sys` minor, and they move in lockstep with
nobody coordinating them. The memory store (A′) avoids the conflict but is
not a working E2EE client (see above). Writing a Vix store on `sqlx`
instead would mean implementing `matrix-sdk`'s store traits:
`matrix-sdk-crypto`'s `CryptoStore` trait alone has 46 `async fn`s, before
the state and event-cache stores, and `matrix-sdk-sqlite` is about 11,500
lines. That is a project in itself, and
it would break with every SDK minor.

### B. Pantalaimon

Pantalaimon is a reverse proxy on `localhost:8009` that speaks the ordinary
Matrix client-server API and encrypts and decrypts transparently, so the
client needs no E2EE code. Devices are verified with its `panctl` tool or its
D-Bus UI. Read on 2026-11-10:

- The GitHub repository `matrix-org/pantalaimon` was **archived on
  2026-04-08**, and the last PyPI release is 0.10.5, from **2022-09-28**.
- It runs on `matrix-nio[e2e]` (`>=0.20,<0.21`), which runs on the C
  **libolm**. Matrix.org deprecated libolm in August 2024 in favor of
  vodozemac, after CVE-2024-45191, -45192 and -45193 (timing side channels
  and malleable signatures).
- It is a Python daemon the user would have to install and keep running.

Building on an archived proxy over deprecated crypto is not acceptable. A
plain client-server client (D) would still work through any Pantalaimon a
user runs on their own, at their own risk. Vix will neither document that
as the E2EE story nor test it.

### C. iamb

`iamb` 0.13.1 (Apache-2.0, `rust-version = 1.96`) is a Vim-style Matrix TUI
built on `matrix-sdk` (its `bundled` feature turns on
`matrix-sdk/bundled-sqlite`). Its README and feature list document no API,
socket, RPC, scripting or headless mode, so there is nothing to drive. It
does show that a Rust TUI on `matrix-sdk` plus SQLite works.

### D. Plain client-server API over `ureq`

Vix already has `ureq` 3.5.1 and `serde_json`, and `/login`, `/sync`,
`/joined_rooms` and `PUT /rooms/{id}/send` are plain JSON over HTTPS. This
adds no crates, but it covers **unencrypted rooms only** (see "What a Matrix
client needs"). It is not the bridge, but the helper protocol below leaves
room to add it later as a no-helper mode for public rooms.

### E. The `vix-matrix` helper (chosen)

A separate program avoids every blocker above:

- **No lockfile conflict.** The helper's workspace resolves `matrix-sdk-sqlite`
  on its own, and `vix`'s graph never sees it.
- **Real E2EE.** vodozemac, which Element X uses, with a persistent SQLite
  crypto store encrypted by `matrix-sdk-store-encryption` under a
  passphrase.
- **`vix` stays the same size.** The ~25 MB lands in a second binary that
  only Matrix users need (the helper's real size with `bundled-sqlite` and
  the release profile is *unmeasured*).
- **One familiar shape.** `vix-im-matrix` gives the App the same pattern as
  `vix-im-irc`: a `connect(options) -> Handle`, a `Command` enum in and an
  `Event` enum out over `std::sync::mpsc`, read on a background thread. The
  App converts events into `vix-im` messages exactly as `im_net.rs` does for
  IRC.

What it costs: a second binary to build, test and ship. It needs its own CI
job (clippy, tests and `cargo deny` with the RUSTSEC-2026-0319 and
RUSTSEC-2026-0459 ignores), its own entry in the per-binary license table
(`matrix-sdk` and vodozemac are Apache-2.0, so it gets the same license as
`vix`), and, for release channels, packaging. The first slice ships it as
a source build and on crates.io only, as `vix-zgui` did. Starting it is a
new process spawn, so `tui/tests/spawn_sites.rs` gets a row gated on
`matrix`.

## The Cargo feature

| Feature | Gates | Pulls into `vix` |
| --- | --- | --- |
| `matrix` | `vix-im-matrix/net`: finding and starting the `vix-matrix` helper and the JSON-lines pump thread; with it, the Matrix rows of `im.connect` and the account picker | Nothing new: `vix-im-matrix` uses `serde`/`serde_json`, already in the lockfile. The protocol types and codec stay compiled in every build, as `vix-im-irc`'s codec does |

`matrix` is **not** in `full`, `net` or `default` until the helper ships
through the release channels, the same rule `jupyter` follows. A build
without it shows Matrix accounts as *"Matrix is not included in this build"*
(`msg.capability_missing`). The helper is found as `vix-matrix` beside the
`vix` executable, then on `PATH`, or at `im.matrix_helper` when set.

## Protocol sketch (for T793 to fix)

One JSON object per line, UTF-8, and nothing else on stdout. The helper's
own diagnostics go to stderr, and Vix drops them unless debugging is on.
Every message carries `"v": 1`.

- **Vix → helper:** `login` (homeserver, user id; the password follows on
  the next line and is never put in argv or the environment), `restore`
  (resume from the store), `send` (room id, body, optional `m.notice` or
  `m.emote`), `join`, `leave`, `rooms`, `logout`, `quit`.
- **Helper → Vix:** `ready` (the helper and protocol versions), `logged_in`
  (user id, device id), `room` (id, display name, `encrypted`, `direct`),
  `message` (room, event id, sender, origin time in UTC, body, `encrypted`,
  and `verified`: whether the sender's device is cross-signed), `undecryptable`
  (room, event id, sender: shown as a System line), `error` (an i18n key and
  arguments, never a pre-formatted English string), `closed`.

Received bodies go through `vix_im::sanitize` before they are stored or drawn,
like IRC. Formatted (`org.matrix.custom.html`) bodies are dropped in favor
of the plain `body`.

## E2EE story

- **Crypto:** vodozemac via `matrix-sdk-crypto`. No libolm anywhere.
- **Store:** `matrix-sdk`'s SQLite state and crypto store under the platform
  data directory (`…/vix/matrix/<account>/`), never in the workspace. It is
  encrypted with a random passphrase that the helper keeps in the OS keyring
  (`keyring-core` 4, as `sql` and `ai` already do), together with the access
  token. The account password is used once at login and never stored.
- **Trust:** messages from devices that aren't cross-signed are marked
  `(unverified)`, the rule IRC nicks already follow. Interactive verification
  (SAS emoji, QR) is **not** in the first slice. Neither is restoring history
  from server-side key backup with a recovery key, so messages sent before
  this device logged in stay undecryptable until that later slice lands.
- **Logging:** the helper never logs message bodies, tokens or keys. The
  `tracing` output of `matrix-sdk` is capped at `warn` and goes only to
  stderr.

## First slice (T793)

1. A `matrix/` workspace (excluded from the root one, like `gui/`) with
   the `vix-matrix` binary: `matrix-sdk` 0.19 with `default-features =
   false` and `e2e-encryption`, `sqlite` and `bundled-sqlite`, plus `ring` as
   the `rustls` provider. It implements password login, restore, sync,
   joined rooms, send, join and leave, and the protocol above, and has its
   own lockfile, `deny.toml` and CI job.
2. `crates/vix-im-matrix`: the protocol types, the JSON-lines codec with
   property tests (always compiled), and `net`, which spawns and pumps the
   helper.
3. `[[im.accounts]]` gains `protocol = "matrix"` (IRC stays the default)
   with `homeserver` and `user`. The password comes from
   `VIX_IM_PASSWORD_<NAME>`, as for IRC.
4. App wiring in `tui/src/app/im_net.rs`: Matrix rooms become `Room`
   conversations and direct rooms become `Direct` ones. Mentions, unread
   counts, history and sharing need no new rules.
5. Tests with a **fake helper** (a test binary that speaks the protocol), so
   no test ever contacts a Matrix server or creates an account.
6. Docs: [`docs/im`](../../docs/im/index.md) and this page.

Not in the first slice: device verification, key backup and recovery,
threads, reactions, edits and redactions (shown as System lines), media,
SSO or OAuth login, the D fallback, and shipping the helper in dist, Homebrew,
npm or the Linux packages.

## Unmeasured

- The size and build-time delta **inside `vix`**. Option A cannot be built
  in the workspace at all, and option E adds nothing to `vix`.
- The helper's size with `bundled-sqlite`, and builds on Linux and Windows
  (everything above was measured on macOS arm64).
- Runtime: memory, first-sync time on a large account, and battery cost of
  the long-poll sync.
- Whether `sqlx` and `matrix-sdk-sqlite` will ever agree on one
  `libsqlite3-sys` minor, which would reopen option A.

## How it was measured

The probe crates lived outside the repo, in a scratch directory that was
deleted afterwards:

1. The probe: `matrix-sdk = { version = "0.19.1", default-features = false,
   features = ["e2e-encryption"] }`, plus an optional `sqlite` feature, and
   `rustls`/`tokio` on Vix's versions.
2. A tokio + rustls baseline binary.
3. A `links` probe with the probe's SQLite configuration and Vix's exact
   `sqlx` line.

Package sets come from `cargo tree --prefix none -e normal,build` for the
host target, compared by name and version with a copy of Vix's
`Cargo.lock`. Builds were clean, with no `RUSTC_WRAPPER` and with sources
prefetched, and times are wall clock. `cargo deny` 0.20.2 ran with a copy of
Vix's `deny.toml`. Pantalaimon and iamb were checked from their PyPI,
crates.io and GitHub pages, and the libolm deprecation from the Matrix.org
blog (2024-08-27). No Matrix server was contacted.
