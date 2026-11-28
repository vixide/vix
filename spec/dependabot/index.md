# Dependabot

Enable GitHub Dependabot `dependabot_security_updates` at the repo level.

Enable GitHub Dependabot `.github/dependabot.yml` for scheduled update PRs.

Keep the number of update PRs low (T832): each Cargo workspace that has its
own lockfile and ships something, and the GitHub Actions, gets one weekly
grouped PR of minor and patch updates; each major update opens its own PR,
since it may need code changes; security updates are never held for the
weekly group.

## As implemented

- **Security updates** (`vulnerability_alerts` + `automated_security_fixes`):
  enabled on `vixide/vix` at the repo level via the GitHub API — there is no
  file for this; it is a repository setting under Settings → Code security.
- **Grouping (T832).** Every entry has one group,
  `update-types: [minor, patch]` with `applies-to: version-updates`:
  `cargo-minor-and-patch` (root), `gui-cargo-minor-and-patch`,
  `matrix-cargo-minor-and-patch` and `actions-minor-and-patch`. A major
  update matches no group, so it opens a PR of its own; security updates are
  not grouped (each is its own PR, opened when the advisory appears). At most
  one grouped PR per entry a week, plus the majors: the `gui/` and
  `matrix/` entries cap open PRs at 3 (`open-pull-requests-limit`), the
  others keep Dependabot's default of 5.
- **`gui/` and `matrix/` entries.** Each is a separate workspace with its
  own `Cargo.lock` and ships binaries, and their largest trees (the GUI
  toolkits, `matrix-sdk`) are in no other graph, so nothing watched them
  before. Both path-depend on root-workspace crates (`gui/` on `tui/`,
  `crates/vix-input`, `crates/vix-i18n`; `matrix/` on
  `crates/vix-matrix-protocol`), whose `{ workspace = true }` deps resolve
  in the root `Cargo.toml` — the same shape that made the `fuzz/` entry
  unsafe (below). So each repeats the root entry's `ignore` list (the three
  lists are kept identical, as the comment at the top of the file says), and
  a `/gui` or `/matrix` PR that edits the root `Cargo.toml` or
  `Cargo.lock` is closed unmerged: that change is the root entry's. Kept
  despite that because, unlike `fuzz/`, both are released, so their own
  pins matter.
- **Scheduled update PRs**: [`.github/dependabot.yml`](../../.github/dependabot.yml),
  weekly, one entry for the root `cargo` graph (with an `ignore` for
  `evalexpr` past 12.0.0 — AGPL-relicensed, incompatible with the license Vix
  offers, see the comment beside it in `Cargo.toml` — plus `vt100` past
  0.16, whose API break would touch `vix-terminal` and the Cowork terminal
  mirror; unlike `evalexpr` that isn't a permanent policy, just unmigrated —
  lift once someone updates those call sites. `getrandom` and `ureq` carried
  the same kind of ignore after the first grouped update PR (#6) broke
  `vix-editor-core` and `vix-http-client`; both have since been migrated
  (`ureq` 3, `getrandom` 0.4) and their ignores removed) plus
  `github-actions` for the workflow files. GitHub-only: Dependabot reads
  `.github/dependabot.yml`
  from the repository as GitHub sees it, so GitLab's and Codeberg's mirrors do
  not need an equivalent — `cargo deny` (`.github/workflows/security.yml`, the
  `deny` job of `.gitlab-ci.yml`, and `.forgejo/workflows/security.yml`)
  already covers advisory scanning on all three forges; Dependabot adds
  version-bump PRs on top of that, on GitHub.
- **No separate `fuzz/` entry** (still true after T832 added `gui/` and
  `matrix/`: `fuzz/` is dev-only and not built in CI, so a PR for its
  own pins would pass CI without being tested). `fuzz/Cargo.toml` declares its own
  `[workspace]` (see [`fuzz/README.md`](../../fuzz/README.md) — deliberately
  separate so `cargo build`/`cargo check --workspace` at the repo root never
  touches it), but it path-depends on several `crates/vix-*` members of *this*
  workspace, whose `{ workspace = true }` deps resolve against the root
  `Cargo.toml`. A grouped `/fuzz` entry therefore isn't actually isolated —
  the first one Dependabot opened edited the root `Cargo.toml` directly,
  bumping `evalexpr` straight past the AGPL line and `getrandom` across a
  breaking API change (`cargo build --workspace` failed on `main`'s own
  crates as a result). Closed unmerged; removed the entry rather than trying
  to fence it with more `ignore` rules.
- **`.github/workflows/release.yml` is generated** (`dist init`, per
  `spec/ci/index.md`), but the `github-actions` entry scans every file under
  `.github/workflows/`, `release.yml` included, and Dependabot has no
  per-file exclusion for that ecosystem. The first `actions/checkout`,
  `actions/upload-artifact`, and `actions/download-artifact` bumps it opened
  (#1–#3) each hand-edited `release.yml` too, which `dist`'s own `plan` job
  then correctly rejected as drifted from what `cargo-dist-version` in
  `dist-workspace.toml` generates. Handle these by hand rather than merging
  the raw diff: apply the bump to `ci.yml`/`security.yml` directly (those
  *are* hand-maintained), and separately bump `cargo-dist-version` and run
  `dist init -y` to regenerate `release.yml` — check whether the new `dist`
  release actually changed that action's pin before assuming the two will
  match up.
- **A grouped `cargo-dependencies` PR mixes safe and breaking bumps** — the
  `cargo-minor-and-patch` group in the root `cargo` entry above (it was
  `cargo-dependencies`, every update type, before T832) means one PR can
  bundle a dozen crates, and `cargo build` fails on the
  bundle as a whole even when most of them are fine. Handle by hand: apply
  the whole diff, build, and revert (with an `ignore` entry, see above) only
  the ones that actually broke something — of 11 proposed once (#6), 8
  applied clean and 3 didn't.
