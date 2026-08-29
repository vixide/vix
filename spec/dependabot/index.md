# Dependabot

Enable GitHub Dependabot `dependabot_security_updates` at the repo level.

Enable GitHub Dependabot `.github/dependabot.yml` for scheduled update PRs.

## As implemented

- **Security updates** (`vulnerability_alerts` + `automated_security_fixes`):
  enabled on `vixide/vix` at the repo level via the GitHub API — there is no
  file for this; it is a repository setting under Settings → Code security.
- **Scheduled update PRs**: [`.github/dependabot.yml`](../../.github/dependabot.yml),
  weekly, one entry per `Cargo.toml`-rooted dependency graph (the main
  workspace at `/` and the separate `fuzz/` workspace — see
  [`fuzz/README.md`](../../fuzz/README.md) for why `fuzz/` is separate) plus
  `github-actions` for the workflow files. GitHub-only: Dependabot reads
  `.github/dependabot.yml` from the repository as GitHub sees it, so GitLab's
  and Codeberg's mirrors do not need an equivalent — `cargo deny`
  (`.github/workflows/security.yml`, the `deny` job of `.gitlab-ci.yml`, and
  `.forgejo/workflows/security.yml`) already covers advisory scanning on all
  three forges; Dependabot adds version-bump PRs on top of that, on GitHub.
