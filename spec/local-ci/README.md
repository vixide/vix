# Local-first CI and CD

Vix's quality bar is [`scripts/check`](../../scripts/check). Today the forges
re-run that bar on their own machines ([`spec/ci`](../ci/index.md)). This spec
describes how to make the **local machine the primary authority** and the
forges a thin, cheap second opinion, because hosted CI has proved unreliable
and costs money. It is the design for tasks T760–T768 in `tasks.md`.

Status: **proposed**, except tiers (T760), the attestation (T761), option 3
(trim hosted CI, T762), option 6 (local Linux and Windows coverage, T765) and option 8 (one entrypoint, T767), which are implemented: see "Slim hosted CI" in [`spec/ci`](../ci/index.md). Nothing here changes the gate itself; it changes where
the gate runs and who has to trust whom.

## Why

Measured or observed, not assumed:

- **GitLab CI is unusable.** Every pipeline for `vixide/vix` has failed with
  `ci_quota_exceeded` since 2026-08-29. The GitLab file still exists but
  verifies nothing.
- **GitHub is the only forge that runs the full matrix**, including macOS and
  Windows (`ci.yml` test matrix, `gui.yml`, `packaging.yml`, `release.yml`).
  Hosted macOS and Windows minutes are the expensive ones for a private
  repository; whether Vix pays depends on repository visibility and plan.
  Hosted runner prices were cut in January 2026, but a platform charge for
  self-hosted runners was announced for 2026-03-01 and then postponed by
  GitHub. Sources disagree on whether it is in force now, so treat
  self-hosted GitHub runners as *free today, not guaranteed*
  ([GitHub's pricing post](https://resources.github.com/actions/2026-pricing-changes-for-github-actions/),
  [changelog](https://github.blog/changelog/2025-12-16-coming-soon-simpler-pricing-and-a-better-experience-for-github-actions/)).
- **The gate already runs locally and is the real bar.** Every change in this
  repository lands only after a full local `scripts/check`. CI has only ever
  confirmed it, and has twice found things the piecemeal local checks missed
  (see the auto-memory note on running the whole gate before a push).
- **The gate is slow, not wrong.** A full run is dominated by
  `cargo hack check --each-feature` (39 builds), the semver step (two builds
  of every crate) and the test suite. Under heavy machine load the
  each-feature step alone took about an hour. `nextest` measured about twice
  as slow as `cargo test` on macOS (process-spawn scanning), so it is opt-in.
- **Four hand-kept copies of the gate** (GitHub, GitLab, Forgejo, `Makefile`)
  drift. `spec/ci` calls this deliberate; it is also a standing maintenance
  cost.

## Gate tiers (T760, implemented)

`scripts/check --tier fast|push|full` runs a prefix-nested subset of the one
gate. The default is `full`, exactly the pre-tier behavior, so nobody who does
not opt in sees a change. An unknown tier prints usage and exits 2.

| Tier | Runs | For |
| ---- | ---- | --- |
| `fast` | `cargo fmt --check`, clippy (pedantic, `-D warnings`) and `cargo test` on only the packages with files changed since the merge base with `main` (committed, staged, unstaged or untracked, mapped to the nearest `Cargo.toml`). If a root `Cargo.toml`, `Cargo.lock`, toolchain, lint config, `.cargo/` file or `scripts/check` changed, or there is no `main`, it uses the whole workspace; if no crate changed it skips clippy and tests. | the edit loop |
| `push` | `fast`'s checks on the whole workspace, plus `cargo build`, the full test suite, `cargo doc`, the generated reference docs and man page, and `scripts/check-docs`. | before a push to a branch |
| `full` | `push` plus the feature combinations (`--no-default-features`, `syntax-all`, `cowork`, `jupyter`, each-feature), the headless examples and chaos smoke, semver, packaging templates and the per-binary license check. | the landing bar |

Every step prints its wall-clock time and the run ends with a summary table
(also printed when a step fails), so slow steps are measured rather than
guessed. The final line is distinct per tier: `full` ends with exactly
`OK: build + clippy + tests + doc + docs all passed`; `push` and `fast` end in
`OK (tier push): …` and `OK (tier fast): …`, which say they are not a full pass
so they cannot be mistaken for one. A signed attestation (T761) must accept
only `full`.


## Principles

1. **One gate, one definition.** A tier of `scripts/check`, never a second
   hand-copied list of commands.
2. **Local is authoritative; remote is a verifier.** A forge job should check
   that the gate already passed for exactly this tree, and independently run
   only what a developer machine cannot (other operating systems).
3. **Trust is signed.** Commits are already SSH-signed. A gate result is
   recorded the same way, so a forge can tell "the maintainer's machine ran
   this" from "someone pushed a claim".
4. **Never trade away independence completely.** A self-reported pass is not
   proof. Keep one cheap, independent Linux job (fmt, clippy, test) on a forge
   that does not bill Vix (Codeberg), so a bad local environment cannot pass
   itself.
5. **Own-machine runners only for trusted code.** A self-hosted runner executes
   whatever a workflow says. It must not run jobs for fork pull requests or
   unreviewed branches.

## Options assessed

| # | Idea | Saves | Risk | Verdict |
| - | ---- | ----- | ---- | ------- |
| 1 | **Gate tiers** (`fast`, `push`, `full`) so most pushes do not pay for each-feature, semver and mutation work | local time, remote minutes | low | **Do first.** |
| 2 | **Signed gate attestation + `pre-push` hook**; forge CI verifies it instead of re-running | almost all remote minutes on trusted pushes | medium: self-reported, so needs principle 4 | **Do second.** |
| 3 | **Trim hosted CI**: macOS/Windows and `cargo hack`, mutants and chaos only on schedule or tags; path filters; cancel superseded runs | most of the paid minutes | low | **Done (T762).** |
| 4 | **Self-hosted runner** on the maintainer's Mac: a [Forgejo runner](https://forgejo.org/docs/latest/admin/actions/) (host-mode label) for Codeberg, a GitHub or GitLab runner for the others | all hosted minutes, native macOS | medium: runs code on a personal machine | **Spike** (needs the maintainer's call on hardware and always-on). |
| 5 | **Build caching** (shared `CARGO_TARGET_DIR`, `sccache`) | local time | low, but the `kache` wrappers are known to cause bogus linker and rustdoc failures in the gate | **Decide**: fix or retire `kache` for the gate; document the clean-room environment. |
| 6 | **Local Linux and Windows coverage** (container or VM script) | the only reasons hosted OS jobs exist | medium | **Done (T765).** A Linux container script and a documented Windows VM recipe; hosted Windows and macOS stay on tags and the weekly schedule. |
| 7 | **Local release path**: build the three targets, checksum, SSH-sign and upload from the maintainer's machine; `release.yml` remains the manual fallback | release minutes, and release depends on no hosted queue | medium: cutting a tag stays the user's decision | **Do** after 1 and 2. |
| 8 | **One entrypoint** (`scripts/ci <job>`) that every forge file calls | drift | low | **Done (T767).** `spec/ci` revised. |
| 9 | **Evaluate** `act` (run GitHub workflows locally), Dagger, the existing `flake.nix` (`nix flake check`) and a Woodpecker instance | unknown | n/a | **Spike only.** Not researched deeply: `act` cannot run macOS or Windows jobs; Woodpecker is a separate server to operate. |

Woodpecker versus Forgejo Actions: a Forgejo runner reuses the existing
`.forgejo/workflows/` files and is lighter to deploy than a Woodpecker server
([discussion](https://45.148.62.229.sslip.io/Codeberg/Documentation/issues/743)).
Garnix and Buildbot were not verified and are not recommended on that basis.

## Local Linux and Windows coverage (option 6, T765, implemented)

**Linux.** `scripts/check-linux [--tier fast|push|full] [--shell]` runs
`scripts/check --tier <tier>` (default `push`) inside a Linux container, for
what a Mac misses: case-sensitive paths, glibc, anything that only builds with
Linux tools.

- *CLI.* It uses the `docker` command (`VIX_CONTAINER_CLI` overrides it).
  Docker Desktop, Colima and OrbStack all provide it. With no CLI, or no
  reachable daemon, it exits 1 and prints how to install or start Colima,
  OrbStack or Docker. It installs nothing.
- *Image.* Built locally from a generated Dockerfile on stdin (no build
  context, so nothing of the repository enters the image): the official
  `rust:<version>-bookworm`, where `<version>` is `rust-version` in the root
  `Cargo.toml` (the MSRV; `VIX_LINUX_RUST` or `VIX_LINUX_IMAGE` override it),
  plus clippy, rustfmt, python3, git and a C++ compiler. Hosted CI tracks
  stable, so a lint added after the MSRV may differ; use `VIX_LINUX_RUST` to
  match stable.
- *Mounts.* The repository is bind-mounted read-only at `/src` and copied into
  the named volume `vix-check-linux-work`, where a throwaway git repository
  (no branch called `main`, so `fast` checks everything) lets the gate's
  `git diff` checks on generated docs run. `target/` and the cargo home are the
  named volumes `vix-check-linux-target` and `vix-check-linux-cargo`, so reruns
  are incremental. There is exactly one host bind mount.
- *Isolation.* Runs as the caller's uid and gid, never root, with all
  capabilities dropped and `no-new-privileges`. It never mounts `~/.ssh`, git
  or other credentials, or the container socket, and passes no secret
  environment. The only run-time network use is cargo fetching crates; the
  one-time image build also reaches apt and rustup.
- *Dry run.* `VIX_CI_DRY_RUN=1` prints the Dockerfile and the `docker run`
  command.
- *Not an attestation.* A pass is a Linux confidence check. `scripts/attest`
  still requires a full run on the maintainer's machine, and an `aarch64`
  container on Apple Silicon is not the `x86_64` that hosted CI runs.

`scripts/test-check-linux` (part of the `push` tier and above) needs no
Docker: it checks argument handling and the generated command (the MSRV image,
non-root user, named volumes, a single read-only bind mount, no credential
paths, `VIX_*` overrides) in dry-run mode, and the two failure messages with
a stub CLI. The container itself is exercised only when someone runs the
script with a real daemon.

**Windows.** There is no Windows script, by design: a Windows VM cannot be
driven from `sh` portably. [`docs/local-ci`](../../docs/local-ci/index.md)
gives the recipe: a Windows 11 VM (UTM, Parallels or VMware Fusion on Apple
Silicon, which tests the `aarch64` target; or a cloud `x86_64` VM, which
matches the hosted job), install Git, Python, the MSVC build tools and rustup
with `winget`, set `core.autocrlf false`, and run `scripts/ci build`,
`scripts/ci test` and `scripts/ci examples` in Git Bash, the same three steps
as the hosted Windows job. **The recipe has not been run by its author on real
Windows hardware** and says so.

**What stays hosted.** Hosted Windows and macOS keep running, but only on
version tags, the weekly schedule and manual dispatch (T762,
[`spec/ci`](../ci/index.md), "Slim hosted CI"), never on every push. These two
local routes are how an OS-specific failure is found before a tag does.

## The attestation (option 2, T761, implemented)

`scripts/attest` runs `scripts/check --tier full` and, only if it passes,
records a signed note bound to the **tree** of `HEAD`, not the commit, so a
signed merge, rebase or fast-forward of identical content stays valid:

```text
vix-gate-attestation v1
tree=<git tree hash>
tier=full
gate-version=<git blob hash of scripts/check>
toolchain=<rustc -V>
os=<uname -sm>
result=pass
duration-secs=<seconds>
finished=<UTC timestamp>
-----BEGIN SSH SIGNATURE-----
```

It is stored as a git note under `refs/notes/vix-gate`, keyed by the tree
hash. It refuses to run on a dirty working tree and refuses to record if the
tree changed while the gate ran. The tier is always `full`; `fast` and `push`
runs never produce a record, and a record whose `tier` is not `full` or whose
`result` is not `pass` never verifies.

**Signing.** The note is signed with `ssh-keygen -Y sign -n vix-gate` over the
exact payload text, using `$VIX_ATTEST_KEY` or, by default, the key named by
`git config user.signingkey` (the key that already signs commits; a key file,
or a public key when an SSH agent holds the private half). `git notes` writes
its own commits and cannot sign them, and a signed notes commit would vouch
for the whole notes history rather than one payload, so the payload is signed
directly. The separate namespace `vix-gate` means the signature cannot be
replayed as a commit signature (or the reverse). Commit signing itself is
never changed or bypassed.

**Verifying.** `scripts/attest --verify [rev]` (default `HEAD`) is read-only,
needs only git and `ssh-keygen`, and makes no network call. It exits 0 only if
all of these hold, and otherwise exits 1 with a one-line `VERIFY FAILED: ...`
reason:

1. a note exists for the tree of `rev`;
2. its signature is valid and from a key in the allowed-signers file
   (`$VIX_ALLOWED_SIGNERS`, else `gpg.ssh.allowedSignersFile`, else
   `.allowed_signers` in the repository root);
3. the `tree` field equals the tree of `rev`;
4. `tier=full` and `result=pass`;
5. `gate-version` equals the hash of `scripts/check` in `rev`. (The tree hash
   already covers the script, so this is a second, readable check that the
   record was made by the gate the tree contains.)

**The pre-push hook.** `scripts/hooks/pre-push` is installed by `make hooks`,
which only runs `git config core.hooksPath scripts/hooks`. For each branch or
tag being pushed it runs `scripts/attest --verify <pushed sha>`. A push of
`refs/heads/main` or of any tag without a valid attestation is refused, and no
setting overrides that. Another branch is also refused, unless the explicit,
documented escape `VIX_ALLOW_UNATTESTED=1` is set for that push (it prints a
warning). Deleting a ref and pushing other refs (such as
`refs/notes/vix-gate`) are not gated. `git push --no-verify` skips every hook,
which is why the forge job below also exists.

**Sharing the notes.** Notes are not pushed or fetched by default. Push them
beside `main` with `git push origin refs/notes/vix-gate`, and fetch them with
`git fetch origin refs/notes/vix-gate:refs/notes/vix-gate`. To fetch them on
every `git fetch`, add `fetch = +refs/notes/vix-gate:refs/notes/vix-gate` under
the remote in `.git/config`. `scripts/attest` itself never pushes.

**The forge `verify` job.** After fetching `refs/notes/vix-gate`, a forge job
runs `VIX_ALLOWED_SIGNERS=<file> scripts/attest --verify "$SHA"`. It is
read-only, takes seconds and needs no toolchain, only git and OpenSSH. The
allowed-signers file must come from a trusted place, not from the pushed
content under test. T767 wired the job in as `scripts/ci verify`: it takes the
allowed signers from a repository secret or variable (`VIX_ALLOWED_SIGNERS_DATA`)
and never reads a committed `.allowed_signers`. A committed file was rejected as
the source because on a push to `main` the file under test is the pushed tip
itself; trusting it only from the protected default branch would need the
pre-push state, which a forge job does not reliably have. The job is advisory
until the variable `VIX_VERIFY_REQUIRED=1`. See "One entrypoint" below.

What the attestation does **not** prove: that the machine was clean, or that a
different operating system passes. That is what the independent Codeberg job
and the scheduled cross-OS jobs are for. It is also self-reported: whoever
holds the signing key can attest without running the gate, so the key is the
trust root, and it is the same key that signs commits.

`scripts/test-attest` tests all of this against a throwaway repository with a
generated ed25519 key and a stub gate (it never touches real keys or the real
repository), and runs in the `push` tier and above.

## One entrypoint (option 8, T767, implemented)

`scripts/ci <job>` is the only place a CI job's commands are written; the GitHub,
Codeberg and GitLab files call it and keep only forge-specific glue (triggers,
path filters, concurrency, runners, tool installation, caching). T762's slim
triggers and path filters are unchanged. The job table, the `verify` trust model
(secret, advisory then required) and what is and is not tested are in
[`spec/ci`](../ci/index.md), "One CI entrypoint". `scripts/test-ci` covers the
job table, the forge-file no-drift check and `verify`; the workflow files
themselves cannot be run locally.

## The local release path (option 7, T766, implemented)

`scripts/release-local VERSION` builds the three Makefile targets (it calls `make
build-macos build-windows build-linux`, nothing is duplicated), packs each as the
asset name the packaging templates use, writes a `.sha256` per asset and one
`SHA256SUMS`, and SSH-signs it (`ssh-keygen -Y sign -n vix-release`, the key and
allowed-signers handling of `scripts/attest`) into `SHA256SUMS.sig`;
`--verify-dir DIR` verifies a downloaded release (signature, then every hash).

It refuses unless: the tree is clean and on `main`; `scripts/attest --verify`
passes for HEAD; VERSION is the workspace version with a `## [VERSION]`
changelog section; and every build tool and cross linker exists. The default and
`--dry-run` do all of that and print the exact `gh`/`tea` commands, but upload and
tag nothing. Uploading needs `--publish` and a typed line, `publish VERSION to
github codeberg`, and requires the tag to exist already on both forges (`gh
release create --verify-tag`), so an upload can never create one. A tag is made
only by the separate `--tag` flag: a signed local tag, never pushed. `release.yml`
stays the manual fallback; pushing a tag starts it, so the maintainer decides
which path is the release. `scripts/packaging render VERSION --sums SHA256SUMS`
consumes the checksums (nothing is published); the templates name the `dist`
targets, which the Makefile does not build, so the handoff is partial. How-to:
[`docs/release`](../../docs/release/index.md).

`scripts/test-release-local` tests the refusal paths, the dry run, signing and
`--verify-dir`, the confirmation, and that no tag appears without `--tag`, in a
throwaway repository with a generated key and stub `make`, `gh` and `tea` (no
network); it runs in the `push` tier and above. Honest limits: never run for real,
and the `tea` flags are from its documentation, not exercised against Codeberg.

## Non-goals

- Do not weaken the gate, skip steps to save time, or disable commit signing.
- Do not run untrusted pull-request code on a self-hosted runner.
- Do not cut a release tag automatically; that stays the maintainer's call
  (see `AI_STATEMENT.md`).

## Tasks

T760 gate tiers · T761 attestation and hook · T762 slim hosted CI · T763
self-hosted runner spike · T764 caching decision · T765 local Linux and
Windows coverage · T766 local release path · T767 single CI entrypoint · T768
tool evaluation spike. Each is detailed in `tasks.md`.
