# Dependabot

Enable GitHub Dependabot `dependabot_security_updates` at the repo level.

Enable GitHub Dependabot `.github/dependabot.yml` for scheduled update PRs.

## As implemented

- **Security updates** (`vulnerability_alerts` + `automated_security_fixes`):
  enabled on `vixide/vix` at the repo level via the GitHub API — there is no
  file for this; it is a repository setting under Settings → Code security.
- **Scheduled update PRs**: [`.github/dependabot.yml`](../../.github/dependabot.yml),
  weekly, one entry for the root `cargo` graph (with an `ignore` for
  `evalexpr` past 12.0.0 — AGPL-relicensed, incompatible with the license Vix
  offers, see the comment beside it in `Cargo.toml` — plus `getrandom`,
  `ureq`, and `vt100` past the versions that broke `vix-editor-core`,
  `vix-http-client`, and `vix-terminal` respectively when the first grouped
  update PR (#6) proposed them; unlike `evalexpr` these aren't a permanent
  policy, just unmigrated — lift once someone updates those call sites) plus
  `github-actions` for the workflow files. GitHub-only: Dependabot reads
  `.github/dependabot.yml`
  from the repository as GitHub sees it, so GitLab's and Codeberg's mirrors do
  not need an equivalent — `cargo deny` (`.github/workflows/security.yml`, the
  `deny` job of `.gitlab-ci.yml`, and `.forgejo/workflows/security.yml`)
  already covers advisory scanning on all three forges; Dependabot adds
  version-bump PRs on top of that, on GitHub.
- **No separate `fuzz/` entry.** `fuzz/Cargo.toml` declares its own
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
  `groups: cargo-dependencies: patterns: ["*"]` in the `cargo` entry above
  means one PR can bundle a dozen crates, and `cargo build` fails on the
  bundle as a whole even when most of them are fine. Handle by hand: apply
  the whole diff, build, and revert (with an `ignore` entry, see above) only
  the ones that actually broke something — of 11 proposed once (#6), 8
  applied clean and 3 didn't.
