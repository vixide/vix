# Security Policy

This is the security policy for Vix: what it covers, how to report a
vulnerability, what happens after you do, and the threat model a report is
judged against. Its requirements are specified in
[`spec/security/index.md`](spec/security/index.md).

## Scope

In scope:

- The `vix` command (`tui/`) and every `vix-*` crate in this repository,
  including the versions published to crates.io.
- The desktop GUIs in `gui/`.
- The release artifacts this repository builds: the GitHub Release archives,
  the shell and PowerShell installers, the Homebrew formula, the npm package,
  and the `.deb`, `.rpm` and Arch packages.
- This repository's CI and release workflows (`.github/workflows/`,
  `.gitlab-ci.yml`, `.forgejo/`), where a flaw could tamper with what is
  built or published.
- The site at <https://vixide.github.io> (`vixide.github.io/`).

Out of scope:

- **Vulnerabilities in a dependency.** Report them to that project. Once an
  advisory is published, `cargo deny` flags it here (see
  [Hardening](#hardening)) and the dependency is updated or the advisory is
  ignored with a dated reason in [`deny.toml`](deny.toml).
- **The external programs Vix runs on your behalf:** language servers,
  debug adapters, formatters, Jupyter kernels, AI agents and CLI clients,
  database clients, `git`, `ssh`, `docker` and the like. Vix is in scope for
  *how* it runs them (argument quoting, which binary, under what trust
  gate), not for what they do once running.
- **Anything that needs a trusted workspace.** Trusting a workspace means
  letting it run code (see [Threat model](#threat-model)). A workspace you
  trusted running code is the feature, not a bypass.
- **Anything that needs the attacker to already control your user account,**
  your configuration directory (`~/.config/vix/`), or the machine.
- The two [accepted risks](#accepted-risks) below, unless a report shows a
  way past the constraints described there.

## Supported versions

| Version | Supported |
| ------- | --------- |
| Latest 2.x release | Yes |
| Anything older | No |

Only the latest published release gets fixes. There are no long-term-support
branches and no backports. Check a report against the latest release (or
`main`) before sending it; a fix lands in the next release.

## Reporting a vulnerability

**Do not open a public issue, discussion or pull request.** Report privately,
either way:

- **On GitHub:** open a private report from the
  [Security tab](https://github.com/vixide/vix/security/advisories/new). Only
  the maintainer sees it, and the fix and the advisory are worked on in the
  same private thread.
- **By email** to the maintainer at
  [joel@joelparkerhenderson.com](mailto:joel@joelparkerhenderson.com). Email
  works from any of the three forges (GitHub, GitLab, Codeberg), and needs no
  GitHub account.

Include:

- What you found and why it is a vulnerability: the impact, not only the
  behavior.
- Steps, a minimal reproduction, or a proof of concept.
- The version (`vix --version`) or commit you tested, and your OS.
- Whether, and how, you would like to be credited.

## What happens next

Vix has one maintainer and no funded security team. These are targets, not
contractual guarantees, and a confirmed vulnerability takes priority over
other work.

| Step | Target |
| ---- | ------ |
| Acknowledge the report | Within 7 days |
| Confirm or decline it, with a severity | Within 14 days |
| Release a fix for a critical or high severity issue | Within 30 days of confirming it |
| Release a fix for a medium or low severity issue | In a release within 90 days |

1. **Triage.** The maintainer reproduces the report and rates it with a CVSS
   v3.1 base score: critical (9.0–10.0), high (7.0–8.9), medium (4.0–6.9) or
   low (0.1–3.9). A declined report gets the reason, usually a pointer to
   [Scope](#scope) or the [Threat model](#threat-model).
2. **Fix.** The fix is developed privately and goes through the same
   `scripts/check` gate and CI as every other change. You are offered the
   chance to check it before release.
3. **Release.** A patch release ships the fix to every channel listed under
   [Scope](#scope).
4. **Disclose.** A GitHub security advisory is published for the release,
   with a CVE requested through GitHub when the issue warrants one. A
   vulnerability in a crate published to crates.io is also reported to the
   [RustSec advisory database](https://rustsec.org), so `cargo audit` and
   `cargo deny` warn downstream users. `CHANGELOG.md` records it under
   **Security**.

## Coordinated disclosure

Please keep a report private until a fix is released, or until 90 days after
you reported it, whichever comes first. If a fix needs longer, the maintainer
will say so before the 90 days end and agree a new date with you. If the issue
is being actively exploited, the fix and the advisory go out as soon as they
are ready.

You are credited by name (or handle) in the advisory and `CHANGELOG.md`
unless you would rather stay anonymous.

## Safe harbor

Research done in good faith under this policy is welcome. Vix will not pursue
or support legal action against you for it. Good faith means you:

- test only against your own installation, files and accounts;
- do not access, change or destroy other people's data;
- do not degrade a service for other people (the site, the forges,
  crates.io, the package registries);
- report what you find privately, and give the maintainer the time described
  under [Coordinated disclosure](#coordinated-disclosure).

## Threat model

Vix is a local editor. It runs as you, with your permissions, and it can
already read and write every file you can. Its security job is to keep content
you did not write from acting as you without your say-so. A report is judged
against these boundaries.

### Trusted

- **You, at the keyboard,** and every action you take explicitly.
- **Your own configuration** in `~/.config/vix/`: settings, keymaps,
  themes, global scripts, and the commands it names (`ai_command`,
  `test_command`, `lsp_servers`, formatters, `password_command`,
  `ai_api_key_command`).
- **The programs those settings name.** Vix runs them; it does not sandbox
  them.

### Untrusted until you trust the workspace

A folder you open is untrusted until you answer **y** to "Trust this
workspace and run them?" (keys pressed in the first 250 ms are ignored, so a
held key cannot answer for you). The answer is remembered per workspace root.
Until then Vix does not:

- load project scripts (`<project>/.vix/scripts/*.rhai`), or a plugin whose
  SHA-256 no longer matches the one you trusted;
- read `<project>/.vix/settings.toml`, which can set commands;
- run project environment detectors (direnv, mise, `.vix/env.toml`);
- use a project's own Prettier from `node_modules/.bin`, or its Jupyter
  kernelspecs;
- start ACP agents;
- start the well-known language servers it finds on your `PATH` (it asks,
  naming the server, the first time you open a file one would serve).

A remote workspace (SSH, container, Kubernetes, WSL) starts untrusted too.
It is remembered per host and path, and until you trust it, its language
servers, debug adapters and tasks stay off. Its `.vix/settings.toml` is
never read, trusted or not. See
[`spec/remote-editing`](spec/remote-editing/index.md).

### Untrusted always

- **The contents of any file you open.** Opening and viewing a file must not
  run code, whatever the file says. Reads refuse FIFOs, sockets and devices,
  oversized files and binary files rather than hang or exhaust memory.
- **Anything a network peer sends:** AI provider responses, language server
  and agent messages, IRC and Matrix traffic, Cowork guests, HTTP responses,
  database results, and a remote agent's frames.
- **An imported settings bundle.** Command fields that differ from your own
  settings are reset, not imported. Theme names must be a bare file name.

### What Vix runs, and when

- **Tasks** (`.vix/project.toml`, discovered tasks) run only when you choose
  one, through a prompt that shows the resolved command.
- **Language servers** start when you first open a matching file, from the
  `lsp_servers` in your settings (or a trusted workspace's). The well-known
  servers Vix starts on its own when they are on your `PATH` (rust-analyzer,
  gopls, pyright, clangd, …) wait for workspace trust, since they can run the
  project's build scripts and macros: the trust prompt names the server first.
- **Debug adapters** start only on **Run → Start Debugging**.
- **Org source blocks** sent to a Jupyter kernel ask first, once per block,
  in an untrusted workspace or for a file outside the workspace.
- **ACP agents** get no permission by default: every request they make
  defaults to deny, and every file they write becomes a diff you accept or
  reject.
- **User scripts** (Rhai) have no file, network or process access at all.
  They can only read and edit the active buffer and register commands and
  keys. See [`crates/vix-script/spec`](crates/vix-script/spec/index.md).

### Network

Vix sends nothing anywhere on its own. It has no telemetry, does not check
for updates, and does not connect to anything until you use a feature that
does:

- **Crash reports** stay on your machine. Vix writes one when it crashes,
  scrubbed of your home folder, user name and quoted text, and only opens a
  prefilled GitHub issue if you choose **Help → Report a Crash…** and then
  submit it yourself. See [crash reports](docs/crash-reports/index.md).
- **AI providers** use HTTPS by default. A local model endpoint defaults to
  plain HTTP on `localhost`. The endpoint is yours to set.
- **`vix --mcp`** speaks MCP over stdio only, with no listening socket. It is
  read-only, keeps paths inside the workspace root (symlinks resolved),
  refuses files with secret-looking names, and caps reads at 2 MiB.
- **Cowork** sessions are always encrypted (Noise `NKpsk0`), invites are
  single-use, and the host approves each guest.
- **SSH** leaves host-key checking to `ssh` and your `known_hosts`, and never
  puts a password on a command line. The remote agent must be the same Vix
  version and has its uploads checked by SHA-256.
- **IRC** uses TLS by default and warns before a plaintext connection.
  **PostgreSQL** and **MySQL** connections use rustls.

### Secrets

- AI keys and database passwords come from a command you configure, then the
  OS keyring, then (for databases) a prompt. A database password is saved
  only to the keyring, and only if you ask.
- Database errors are redacted. IRC and Matrix passwords print as
  `<redacted>` and are zeroed after use.
- `db_history.toml` keeps the text of the queries you ran (never
  bind-parameter values) and is made owner-only on Unix. Do not put literal
  secrets in query text you would not want kept there.

## Accepted risks

These are known and accepted, not oversights:

- **The HTTP client (`vix-http-client`, the `.http`-buffer tool) has no
  loopback or private-IP guard against SSRF.** It is a local developer tool
  for sending requests to whatever host you type, including `localhost` and
  private addresses during API development. Blocking those would break its
  main use. It does allow only the `http` and `https` schemes. Do not point it
  at untrusted, attacker-influenced URLs and expect it to protect you.
- **`vix-db`'s streaming query session could, in principle, attribute a
  result to the wrong in-flight request** if a UI-level guard were ever
  bypassed. Today the UI serializes query dispatch, so this cannot happen in
  practice. A blanket guard at the session layer would break the legitimate
  internal catalog and commit calls that run during a user query's stream,
  so it has not been added. It stays a tracked, accepted risk until a real
  per-call-site fix lands.

A report about either gets a reply pointing here, not a fix, unless it shows a
way to real impact despite these constraints. In that case, please do report
it.

## Hardening

- **No unsafe code.** `unsafe_code = "forbid"` applies to every crate
  through the workspace lints.
- **Supply chain.** Every push and pull request, and a weekly schedule, runs
  `cargo deny check` against [`deny.toml`](deny.toml): RustSec advisories, a
  license allow-list, bans on wildcard and duplicate dependencies, and
  allowed registry sources. See [`spec/ci`](spec/ci/index.md).
- **Fuzzing.** `fuzz/fuzz_targets/` fuzzes the parsers and text transforms
  that take input Vix did not write, asserting invariants rather than only
  the absence of a panic. Every target runs for two minutes each week in CI
  (`.github/workflows/fuzz.yml`), and for as long as you like locally
  (`cargo +nightly fuzz run <target>`).
- **Mutation testing.** A weekly `cargo mutants` run checks that the small
  pure crates' tests catch every behavior change.
- **Release integrity.** Each release publishes a SHA-256 checksum for every
  artifact (and a combined `sha256.sum`) and a CycloneDX SBOM. Release
  artifacts are not yet signed or attested.

## Checklist for contributors

The [Security Checklist](docs/security-checklist.md) covers what to think
about when a change adds input handling, process spawning, network access or
stored secrets. A change that adds or moves a trust boundary updates the
[Threat model](#threat-model) above in the same change.
