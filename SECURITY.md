# Security Policy

## Supported versions

Only the latest published release is supported. Vix does not maintain
long-term-support branches or backport fixes to older versions — upgrade
to the latest release before reporting an issue, and a fix lands in the
next one.

## Reporting a vulnerability

**Do not open a public issue.** Report privately to the maintainer at
[joel@joelparkerhenderson.com](mailto:joel@joelparkerhenderson.com) (the
same contact `AI_STATEMENT.md`'s "Questions" section names). Include:

- What you found and why it's a vulnerability (impact, not just a
  description of the behavior).
- Steps, a minimal reproduction, or a proof-of-concept if you have one.
- The version (or commit) you tested against.

You'll get an acknowledgment within a few days. Vix is a side project
with one maintainer, not a funded security team, so there's no formal
SLA on a fix — but a confirmed vulnerability is prioritized over other
work, and you'll be credited (unless you'd rather stay anonymous) once a
fix ships and is disclosed.

GitHub also surfaces this file under its Security tab, including
[private vulnerability reporting](https://github.com/vixide/vix/security/advisories/new)
as an alternative to email if you'd prefer that flow; GitLab and
Codeberg mirror this file as-is but don't offer an equivalent guided
flow there — email works from any of the three.

## Supply-chain scanning

Every push and pull request (and a weekly schedule) runs
`cargo deny check` against [`deny.toml`](deny.toml): RUSTSEC advisories,
an allow-list of licenses, wildcard/duplicate dependency bans, and
registry sources. See [`spec/ci/index.md`](spec/ci/index.md)'s "Supply
chain" section for how that's wired across all three forges, and its
recorded exceptions (a version pin and two dated, justified advisory
ignores).

## Out of scope, by design

Two things a security report might reasonably flag are known and
accepted, not oversights:

- **The HTTP client (`vix-http-client`, the `.http`-buffer tool) has no
  loopback or private-IP guard against SSRF.** It's a local developer
  tool for hand-driving REST calls against whatever host you type,
  including `localhost`/private IPs during API development — blocking
  those would break its main use. It does enforce an `http`/`https`
  scheme allowlist (no `file:`, no other schemes). Don't run it against
  untrusted, attacker-influenced URLs and expect it to protect you.
- **`vix-db`'s streaming query session can, in principle, misattribute
  a result to the wrong in-flight request** if a UI-level guard were
  ever bypassed — today the UI serializes query dispatch, so this isn't
  reachable in practice, but a blind guard at the session layer would
  break legitimate internal catalog/commit calls that run *during* a
  user query's stream, so it hasn't been added. Tracked as a known,
  accepted risk pending a real per-call-site fix, not a silent gap.

A vulnerability report about either of these will get a reply pointing
back to this section, not a fix — unless it demonstrates a way to reach
real impact despite the constraints just described, in which case
please do report it.

## Scripting sandbox

`vix-script` (Rhai user scripts, `.rhai` files under
`~/.config/vix/scripts/` and `<project>/.vix/scripts/`) runs with no
file, network, or process access by default — Rhai's standard library
simply doesn't expose any, so there's nothing to disable. A script can
only read and write the active buffer/selection and register commands
and key bindings. See
[`crates/vix-script/spec/index.md`](crates/vix-script/spec/index.md).
