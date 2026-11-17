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
- **Reporting.** One private channel that works from all three forges
  (email), and what a report should include. No public issue.
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
- **Hardening**, including what is *not* done yet (release signing,
  fuzzing in CI), so the policy never claims more than the project does.

## Keeping it true

- Every claim in the threat model is backed by the owning crate's or topic's
  spec. A change that adds or moves a trust boundary (a new thing that runs
  project content, a new network peer, a new stored secret) updates
  `SECURITY.md` in the same change, the same way it updates its own spec.
- A reporting channel is listed only once it works. GitHub's private
  vulnerability reporting is not enabled on `vixide/vix`, so it is not
  listed; enabling it is the maintainer's call, and the policy names it once
  it is on.
- `docs/contributing-security.md` includes `SECURITY.md` into the book, and
  [`docs/security-checklist.md`](../../docs/security-checklist.md) is the
  contributor checklist the policy points to.

## See also

- [`spec/ci`](../ci/index.md): `cargo deny`, the mutation-testing run.
- [`spec/test`](../test/index.md): fuzzing.
- [`spec/remote-editing`](../remote-editing/index.md): remote workspace trust.
- [`crates/vix-script/spec`](../../crates/vix-script/spec/index.md): the
  workspace trust prompt and the script sandbox.
