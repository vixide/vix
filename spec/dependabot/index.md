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
  offers, see the comment beside it in `Cargo.toml`) plus `github-actions` for
  the workflow files. GitHub-only: Dependabot reads `.github/dependabot.yml`
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
