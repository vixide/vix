# Local-first CI and CD

Vix's quality bar is [`scripts/check`](../../scripts/check). Today the forges
re-run that bar on their own machines ([`spec/ci`](../ci/index.md)). This spec
describes how to make the **local machine the primary authority** and the
forges a thin, cheap second opinion, because hosted CI has proved unreliable
and costs money. It is the design for tasks T760–T768 in `tasks.md`.

Status: **proposed**. Nothing here changes the gate itself; it changes where
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
| 3 | **Trim hosted CI**: macOS/Windows and `cargo hack`, mutants and chaos only on schedule or tags; path filters; cancel superseded runs | most of the paid minutes | low | **Do alongside 1.** |
| 4 | **Self-hosted runner** on the maintainer's Mac: a [Forgejo runner](https://forgejo.org/docs/latest/admin/actions/) (host-mode label) for Codeberg, a GitHub or GitLab runner for the others | all hosted minutes, native macOS | medium: runs code on a personal machine | **Spike** (needs the maintainer's call on hardware and always-on). |
| 5 | **Build caching** (shared `CARGO_TARGET_DIR`, `sccache`) | local time | low, but the `kache` wrappers are known to cause bogus linker and rustdoc failures in the gate | **Decide**: fix or retire `kache` for the gate; document the clean-room environment. |
| 6 | **Local Linux and Windows coverage** (container or VM script) | the only reasons hosted OS jobs exist | medium | **Do** a Linux container script; Windows stays hosted but on tags and nightly only. |
| 7 | **Local release path**: build the three targets, checksum, SSH-sign and upload from the maintainer's machine; `release.yml` remains the manual fallback | release minutes, and release depends on no hosted queue | medium: cutting a tag stays the user's decision | **Do** after 1 and 2. |
| 8 | **One entrypoint** (`scripts/ci <job>`) that every forge file calls | drift | low | **Do**, and revise `spec/ci` ("deliberately duplicated"). |
| 9 | **Evaluate** `act` (run GitHub workflows locally), Dagger, the existing `flake.nix` (`nix flake check`) and a Woodpecker instance | unknown | n/a | **Spike only.** Not researched deeply: `act` cannot run macOS or Windows jobs; Woodpecker is a separate server to operate. |

Woodpecker versus Forgejo Actions: a Forgejo runner reuses the existing
`.forgejo/workflows/` files and is lighter to deploy than a Woodpecker server
([discussion](https://45.148.62.229.sslip.io/Codeberg/Documentation/issues/743)).
Garnix and Buildbot were not verified and are not recommended on that basis.

## The attestation (option 2)

`scripts/attest` runs the chosen tier and, on success, writes a record bound
to the **tree**, not the commit, so a rebase or `--no-ff` merge of identical
content stays valid:

```text
tree=<git tree hash>  tier=full  toolchain=<rustc -V>  os=<uname -sm>
result=pass  finished=<UTC>  gate-version=<hash of scripts/check>
```

It is stored as a git note under `refs/notes/vix-gate` and signed with the same
SSH key that signs commits. A `pre-push` hook (installed by `make hooks`, which
sets `core.hooksPath`) refuses a push to `main` whose tip tree has no passing
`full` record, unless `VIX_SKIP_ATTEST=1` is set deliberately. The forge `verify`
job fetches the note, checks the signature against the allowed-signers file
already in the repository, checks the tree hash, and passes in seconds.

What the attestation does **not** prove: that the machine was clean, or that a
different operating system passes. That is what the independent Codeberg job
and the scheduled cross-OS jobs are for.

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
