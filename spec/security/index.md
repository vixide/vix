# Security

Vix keeps a formal security policy in the repo-root
[`SECURITY.md`](../../SECURITY.md), which GitHub, GitLab and Codeberg each
show as the project's security policy. This spec says what the policy must
contain and how it stays true.

## Requirements

`SECURITY.md` has these sections:

- **Scope.** What is in scope (the `vix` command, every crate including its
  crates.io releases, the GUIs, every release channel, the CI and release
  workflows, the site) and what is not (dependencies, the external programs
  Vix runs, anything needing a trusted workspace or an already-compromised
  account, and the accepted risks).
- **Supported versions.** Only the latest release.
- **Reporting.** Two private channels: GitHub's private vulnerability
  reporting, and email, which works from all three forges and needs no
  account. What a report should include. No public issue.
- **Response targets.** Acknowledge in 7 days, triage with a CVSS v3.1
  severity in 14 days, fix critical and high within 30 days of
  confirmation, fix medium and low within 90 days. They are targets for a
  one-maintainer project, stated as such.
- **Disclosure.** Coordinated, with a 90-day default embargo; a GitHub
  security advisory (with a CVE when warranted); a RustSec advisory for a
  published crate; a **Security** entry in `CHANGELOG.md`; credit unless the
  reporter declines it.
- **Safe harbor** for good-faith research.
- **Threat model.** What is trusted, what is untrusted until the workspace is
  trusted, what is always untrusted, what Vix runs and when, what it sends on
  the network, and how it holds secrets.
- **Accepted risks**, each with the reason it is accepted.
- **Hardening**, including what is *not* done yet (release signing), so
  the policy never claims more than the project does.

## Keeping it true

- Every claim in the threat model is backed by the owning crate's or topic's
  spec. A change that adds or moves a trust boundary (a new thing that runs
  project content, a new network peer, a new stored secret) updates
  `SECURITY.md` in the same change, the same way it updates its own spec.
- A reporting channel is listed only once it works. GitHub's private
  vulnerability reporting is enabled on `vixide/vix` (2026-11-17); check
  with `gh api repos/vixide/vix/private-vulnerability-reporting`. GitLab and
  Codeberg have no equivalent, so email stays the channel that works
  everywhere.
- `docs/contributing-security.md` includes `SECURITY.md` into the book, and
  [`docs/security-checklist.md`](../../docs/security-checklist.md) is the
  contributor checklist the policy points to.
- [`docs/threat-model.md`](../../docs/threat-model.md) is the detailed
  threat model: for each trust boundary (workspace trust, the authority slot
  and remote agent, language servers and debug adapters, Rhai scripts, MCP
  server mode, ACP agents, AI providers, database credentials, Cowork, IM,
  Jupyter kernels, crash reports, the HTTP client, the repository itself) its
  assets, entry points, existing mitigations with the files that enforce
  them, and open risks. It ends with the checklist for a new feature that the
  pull request templates (`.github/` and `.forgejo/`, kept identical by
  `tui/tests/code_lints.rs`) link. A change that moves a boundary updates it
  together with `SECURITY.md`.
- **No committed secrets.** `tui/tests/secret_scan.rs` reads every file
  `git ls-files` lists (working tree only: no history, no network, no
  external scanner) and fails on a high-confidence credential shape: a
  private key header, an AWS access key id, a GitHub classic or fine-grained
  token, a Slack token, an Anthropic key or an OpenAI-style key, each at its
  real length. Mentions and short fakes (`sk-ant-test`) do not match. A
  fixture that needs a full-length fake is allowlisted by path with a reason,
  and an allowlist entry that no longer matches fails. Binary files and
  files over 16 MiB are skipped; without `git` the test is skipped with a
  note. It runs in under a second.

## See also

- [`spec/ci`](../ci/index.md): `cargo deny`, the mutation-testing run.
- [`spec/test`](../test/index.md): fuzzing.
- [`spec/remote-editing`](../remote-editing/index.md): remote workspace trust.
- [`crates/vix-script/spec`](../../crates/vix-script/spec/index.md): the
  workspace trust prompt and the script sandbox.
