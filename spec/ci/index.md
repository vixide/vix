# Continuous integration and delivery

Vix lives on three forges — GitHub, GitLab, and Codeberg — and `git push`
pushes to all three (`git remote -v` shows the three push URLs). Each forge
therefore carries its own CI configuration, and all of them enforce the same
gate: **the bar in [`scripts/check`](../../scripts/check)**.

> **Proposed direction:** make the local gate the authority and the forges a
> verifier — see [`spec/local-ci`](../local-ci/index.md).

## The gate

Every forge runs, in this order:

```sh
cargo fmt --all --check                                  # spec/rust-cargo-fmt
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings    # spec/rust-clippy-pedantic
cargo test --workspace
# T505: the headless examples must keep running, not just compiling
cargo run --example list_commands && cargo run --example headless_edit \
  && cargo run --example render_frame -- <file> && cargo run --example textops_pipeline -- <file> \
  && cargo run --example query_search -- <dir> <pattern>
cargo doc --workspace --no-deps                          # RUSTDOCFLAGS=-D warnings
cargo run --example list_commands -- --write && git diff --exit-code -- docs/reference/  # T305: generated reference current?
cargo run --example generate_man && git diff --exit-code -- tui/man/          # T307: man page current?
python3 scripts/check-docs                               # documentation integrity
```

`scripts/check` also takes `--tier fast|push|full` (T760, see
[`spec/local-ci`](../local-ci/index.md)); the forges and a landing always mean
the default `full` tier, which is the list above plus the feature, semver,
packaging and license steps. `fast` and `push` print `OK (tier …)` lines and are
never a substitute.

`scripts/attest` (T761) runs the `full` tier and records a signed git note
under `refs/notes/vix-gate`, bound to the tree hash, that a `pre-push` hook
(`make hooks`) and a read-only `scripts/attest --verify` check; only `full`
counts. A forge job may use `--verify` to confirm that the gate already passed
for exactly this tree instead of re-running it; the forge workflows do not do
so yet. See [`spec/local-ci`](../local-ci/index.md), "The attestation".

Nothing merges that does not pass all nine. Run `scripts/check` (or
`make check`) locally first; CI should only ever confirm what the local gate
already said.

GitHub additionally runs an **MSRV** job: `cargo check --workspace --all-targets`
on the toolchain floor, which is the current stable release minus two (see
[`spec/rust-msrv-n-minus-2/index.md`](../rust-msrv-n-minus-2/index.md)). GitLab
gets the same signal for free by pinning its image to the floor.

Two things are deliberately **not** in the gate:

- **Fuzzing** needs nightly and a time budget, and it is a tool for the person
  changing a parser rather than a pass/fail bar. A crash it finds becomes a unit
  test, which *is* in the gate.
- **Benchmarks** need a quiet machine to mean anything; a shared CI runner would
  produce noise and false alarms. Run `cargo bench` locally, before and after,
  when touching a hot path.

See [`spec/test/index.md`](../test/index.md) for how the layers fit together.

## Files

| Forge    | CI                          | CD                              | Supply chain                       |
| -------- | --------------------------- | ------------------------------- | ---------------------------------- |
| GitHub   | `.github/workflows/ci.yml`  | `.github/workflows/release.yml` | `.github/workflows/security.yml`   |
| GitLab   | `.gitlab-ci.yml`            | `release` stage of the same file | `deny` job of the same file       |
| Codeberg | `.forgejo/workflows/ci.yml`, `docs.yml` | `.forgejo/workflows/release.yml` | `.forgejo/workflows/security.yml` |

Since T767 the forge files no longer carry their own command lists: they all
call the one entrypoint `scripts/ci` (next section). When the gate changes,
change `scripts/ci` and `scripts/check` together; the forge files change only
for forge-specific reasons (triggers, runners, tool installation, caching).

## One CI entrypoint (T767)

`scripts/ci <job>` is the single definition of what each CI job runs, and
`scripts/ci list` prints the job names. GitHub, Codeberg and GitLab are thin
wrappers: check out, install a toolchain or a pinned tool, cache, then call a
job. This replaces the earlier arrangement in which the same commands were
deliberately copied into each file, which is a standing risk of drift.

| Job | What it runs |
| --- | --- |
| `fmt`, `clippy`, `build`, `test` | `cargo fmt --check`; pedantic clippy `-D warnings`; build all targets; the tests (nextest plus doctests when `cargo-nextest` is installed, else `cargo test`) |
| `examples`, `doc`, `reference-docs`, `man-page`, `licenses` | the headless examples (T505); rustdoc with warnings denied; the generated references and man page are current (T305, T307); the per-binary license check (T648) |
| `check` | all of the above in order: the one Linux job on every push (T762) |
| `feature-combos` | every feature combination (T587), run on the schedule and on tags |
| `docs-check`, `docs-links`, `docs-links-external`, `docs` | `scripts/check-docs`; lychee offline (blocking); lychee http(s) (advisory, marked so by the workflow); the first two together |
| `verify` | the signed gate attestation for the pushed tip (below) |

What deliberately stays in a workflow file: triggers and path filters,
`concurrency`, runner images, installing Rust, CSpell, lychee, cargo-hack or
nextest, caching, and the jobs that are not part of the gate (`security`,
`release`, packaging, GUI, mutants, chaos, container, binary size, the mdBook
build and deploy, semver, offline tests, coverage, MSRV). Those are either
forge-specific or already a single named tool invocation. Moving them behind
`scripts/ci` is possible later but was not done in T767.

**`verify`.** It fetches `refs/notes/vix-gate` from `origin` and runs
`scripts/attest --verify` against the pushed tip (`VIX_CI_SHA`, else `HEAD`).
The allowed-signers file is the trust root and is supplied from outside the
pushed tree: the text of a repository secret or variable
(`VIX_ALLOWED_SIGNERS_DATA`, exposed from `secrets.VIX_ALLOWED_SIGNERS` on
GitHub and Codeberg and from the CI/CD variable of the same name on GitLab), or
a file path in `VIX_ALLOWED_SIGNERS`. A committed `.allowed_signers` and
`gpg.ssh.allowedSignersFile` are never consulted, because a push could replace
them. The job is **advisory first**: unless the variable `VIX_VERIFY_REQUIRED`
is `1`, a missing secret, a missing notes ref or a failed verification prints
`WARNING` and exits 0; with it set, each of those fails the job. Switch it to
required once a few attested pushes have verified cleanly. It runs only for
pushes (GitHub and Codeberg: `github.event_name == 'push'`; GitLab: the default
branch), because a pull request's merge commit has a tree nobody attested. The
verifier code (`scripts/ci`, `scripts/attest`) comes from the pushed tree, so
the real guard is review of changes to those files and branch protection on the
default branch; that is a limit of any in-repository check.

**What was and was not verified.** `scripts/test-ci` (in the `push` tier) tests
the job table with `VIX_CI_DRY_RUN=1`, checks that every `scripts/ci <job>` named
in the four forge files exists and that the old inline command lists have not
reappeared, and runs `verify` against a throwaway repository with a local bare
remote (valid, wrong signer, unattested tree, missing notes, signers from data
and from a file, committed `.allowed_signers` ignored, advisory versus required).
The forge workflows themselves **cannot be run locally**: the YAML was parsed
and the jobs they call were run, but the first real run on each forge is the
first test of the glue (checkout credentials for the notes fetch, secret
exposure, the GitLab job's `ssh-keygen`).

## Slim hosted CI (T762)

Hosted CI is deliberately thin, because the local `scripts/check` is the
authoritative gate ([`spec/local-ci`](../local-ci/index.md)) and hosted minutes
cost money or, on GitLab, are exhausted. Two tiers:

- **Every push to `main` and every pull request:** one Linux job that runs the
  fast gate (fmt, clippy, build, test, headless examples, rustdoc, generated
  references, binary licenses), plus the documentation checks (`check-docs`,
  lychee, mdBook). A **docs-only** change (only `*.md` and files under
  `docs/`, `spec/` or `agents/`, and not the generated `docs/reference/` or
  `tui/man/`) skips the Rust job but still runs the docs checks.
- **Weekly schedule, version tags and `workflow_dispatch`:** everything else.
  The macOS and Windows test matrix, `cargo hack --each-feature` and the other
  feature combinations, semver, offline tests, coverage, MSRV, binary size,
  the GUI matrix (`gui.yml`), mutation testing (`mutants.yml`) and the chaos
  monkey (`chaos.yml`, nightly) all run here. **No check was removed, only
  moved**: a regression they would catch surfaces on the next scheduled run or
  at the latest on the release tag.

Every workflow uses `concurrency` with `cancel-in-progress: true`, so a newer
push to the same ref cancels the superseded run; the scheduled run has its own
group so it cannot cancel a push to `main`. GitLab and Codeberg follow the
same split (Codeberg: `ci.yml` skips docs-only changes via `paths-ignore` and
runs the feature-combination step only on schedule, tags and dispatch;
`docs.yml` always runs the docs checks). GitLab is unchanged apart from a note
at the top of `.gitlab-ci.yml`: its pipeline is paused by CI-minute quota
(`ci_quota_exceeded` since 2026-08-29), so a red run there means no quota.

## GitHub

Jobs in `ci.yml`. `plan` decides what this run needs (`full`: schedule, tag or
dispatch; `rust`: not a docs-only change).

- **check** — the one per-push Linux job: `fmt`, `clippy`, `build`,
  `nextest`, headless examples, `doc`, generated references and the binary
  license check. Needs `rust`.
- **docs-check**, **docs-links**, **docs-build** (+ **docs-deploy** on `main`) —
  the documentation checks, on every push and pull request.
- **test** — *full runs only.* `build` + `test` on `macos-latest` and
  `windows-latest` (T547, `tasks.md`; Linux is `check`). Windows joined the matrix because the
  release workflow's cross-builds only compile for distribution (via `dist`,
  on version tags) — they never run `cargo test`, so they can't catch an
  OS-specific runtime bug (e.g. shelling out via a hardcoded POSIX `sh`,
  T547's own finding). The job's default shell is `bash`
  (Git-Bash, preinstalled on the `windows-latest` runner image) rather than
  the platform-default PowerShell, so the same POSIX-syntax steps work
  unmodified on every OS in the matrix. GitLab and Codeberg stay Linux-only.
- **offline-test**, **feature-combos**, **semver**, **coverage**, **msrv**,
  **binary-size** — *full runs only.* **msrv** is `cargo check` on the toolchain floor declared by
  `workspace.package.rust-version` in `Cargo.toml`; bump the matrix entry when
  that floor moves. **binary-size** records its baseline from the scheduled run
  on `main`; its pull-request comment step no longer fires.

Caching is `Swatinem/rust-cache`.

Releases are produced by [`dist`](https://opensource.axo.dev/cargo-dist/)
(config in `dist-workspace.toml`): pushing a version tag builds the seven
target triples (`aarch64`/`x86_64-apple-darwin`, `aarch64`/`x86_64-unknown-linux-gnu`,
`x86_64-unknown-linux-musl`, `aarch64`/`x86_64-pc-windows-msvc`), the shell/PowerShell/npm/Homebrew/MSI installers, and the
GitHub Release, then pushes the formula to `vixide/homebrew-tap` — that last
step is why the `HOMEBREW_TAP_TOKEN` secret exists (see
`spec/homebrew-tap-token`). Windows on ARM is the terminal app only (each GUI lists its own
targets) and builds on the native `windows-11-arm` runner
(`github-custom-runners`), since dist's default Ubuntu cross-build can't
make the MSI (T658). `release.yml` is **generated**: edit
`dist-workspace.toml` and re-run `dist init` (or the narrower `dist
generate --mode ci`), never the workflow by hand — `dist generate --mode
ci --check` fails if the committed file has drifted from the config, the
same check-then-write shape `docs/reference/*.md`/`tui/man/vix.1`'s own
regenerate-and-diff gate uses, though nothing currently runs it in CI
(only ever exercised by hand before a release-affecting config change).
`dist-workspace.toml`'s `cargo-cyclonedx = true` (T554, tasks.md) makes
each release also generate and upload a CycloneDX SBOM (`*.cdx.xml`, one
per target) via `cargo-cyclonedx` — native `dist` support, not a
hand-rolled job — for downstream consumers doing their own supply-chain
compliance.

Who's authorized to do what here is governed by
[`AI_STATEMENT.md`](../../AI_STATEMENT.md), not by this file: an AI agent may
publish a crate to crates.io, and may judge on its own that a specific
release is ready to do that for, without asking first. Pushing the version
tag that triggers the pipeline above — and so the GitHub Release, the
installers, and the Homebrew tap push — is not covered by either grant and
still gets confirmed first.

### Offline tests (T573)

GitHub only: an `offline-test` job builds the tests with the network, then
runs `cargo test --workspace --offline` inside `unshare -n` with only
loopback up (the AI/HTTP smoke tests use a local listener), as the
unprivileged user. A test that reaches the real network fails there instead
of in a downstream packager's sandbox. Test git helpers set
`GIT_TERMINAL_PROMPT=0`. Audit at introduction: no test used a real remote.

### Minimal release variant (T594)

`.github/workflows/release-minimal.yml` (GitHub only, hand-written — unlike the
dist-generated `release.yml`, which it must not be merged into) builds `vix` with
`--no-default-features` for five targets on the same version-tag trigger,
packages each with the README, LICENSE, CHANGELOG and man page plus a SHA-256, and
attaches them to the GitHub Release once `release.yml` has created it (it polls for
up to 90 minutes; `GITHUB_TOKEN`-created releases cannot trigger a second workflow).
No installers: dist builds one app per package and every installer assumes one
binary per target. Not exercised on a runner until the next tag.

### Container image (T743)

`.github/workflows/container.yml` (GitHub only, hand-written) builds
`docker/Dockerfile` natively on `ubuntu-24.04` (amd64) and `ubuntu-24.04-arm`
(arm64) on the same version-tag trigger, pushes each by digest to
`ghcr.io/vixide/vix`, and joins them into `:<tag>` plus `:latest` (not for a
prerelease tag). It never runs on a branch push; a manual `workflow_dispatch`
run builds both architectures and pushes nothing. It has not run yet. See
[`packaging/index.md`](../../packaging/index.md).

### Mutation testing (T606)

`.github/workflows/mutants.yml` (GitHub only) runs `cargo mutants` weekly and on demand over
the small pure crates (list-state, civil-date, byte-size, textops, capped-stack, greedy-wrap,
hex-rgb) and uploads `mutants.out/`. It is advisory (`continue-on-error`): a missed mutant
points at an unasserted behavior. `.cargo/mutants.toml` lists only mutants proven equivalent.

### Desktop GUIs (T641, T642, T647)

`.github/workflows/gui.yml` (GitHub only) builds the desktop GUIs — `gui/`, a separate Cargo
workspace the main gate never builds: `vix-gpui` (GPUI), `vix-egui`
(egui), `vix-iced` (iced), `vix-gtk4` (GTK 4) and `vix-slint` (Slint) — on macOS, Windows and Linux: `cargo fmt --check`, pedantic clippy with warnings
denied over all features, and the unit tests, all via `--manifest-path gui/Cargo.toml
--locked`. On macOS it also runs each front end's real render (the `screenshot` feature:
open the window, type into the command palette, save a PNG) and uploads the images. Linux
installs the windowing/font system libraries, GTK 4 and OpenGL/EGL first; macOS installs
Homebrew's `gtk4`; Windows runners have no GTK, so `vix-gtk4` is excluded there. It is advisory (`continue-on-error`) and runs weekly, on version tags and on demand (T762), no longer on every push or PR. The matrix also includes
`ubuntu-22.04` and `ubuntu-22.04-arm`, the runners `release.yml` builds the GUIs on: dist
releases `vix-gpui`, `vix-egui`, `vix-iced` and `vix-slint` (not `vix-gtk4`) from the same tag as `vix-tui` (T645; `dist-workspace.toml`
members `cargo:.` and `cargo:gui`), so a GUI that does not build there fails the whole release.
See `gui/spec/index.md`.

### Packaging channels (T742)

`.github/workflows/packaging.yml` (GitHub only) checks the install-channel
templates under `packaging/` and the Nix flake, and, after the `Release`
workflow succeeds on a version tag, renders them for that release and
verifies them (`nix build`, `winget validate`, `makepkg`). Its publish jobs
(winget-pkgs, a Scoop bucket, the AUR) run only when the repository variable
`PACKAGING_PUBLISH` is `true` and the placeholder secret exists. See
[`spec/packaging`](../packaging/index.md). `scripts/packaging check` is also a
step of `scripts/check`.

### Chaos monkey (T575)

`.github/workflows/chaos.yml` (GitHub only) runs `tui/examples/chaos_monkey` nightly and on
demand: seeded random key, mouse, paste and resize streams into the real `App` on a
`TestBackend`, each iteration under a watchdog, inside an empty network namespace
(`unshare -n`, as in the offline test job). The example re-executes itself in a
sandbox (empty `PATH`, throwaway `HOME`) so a random command line cannot start
anything. A second pass runs the same streams with `--rtl 1` (Arabic, bidi reordering and the
mirrored layout forced on). It also asserts, after every event, that the cursor, selections and extra
carets lie inside the text. A failure prints the seed, Vix's own stack frames and the
last events; `--seed N --iterations 1` replays it. It is not in the per-push gate;
`scripts/check` runs only a three-iteration fixed-seed smoke so the driver cannot rot.

### SSH authority against a real sshd (T719)

`.github/workflows/ssh.yml` (GitHub only) runs [`scripts/test-ssh-local`](../../scripts/test-ssh-local)
on `ubuntu-latest` after installing `openssh-server`. The script starts a throwaway sshd **as
the runner user** (not root) on `127.0.0.1:22022` with a generated ed25519 host key and client
key, public-key login only, and its own `authorized_keys`; the client side gets its own
`ssh_config` (a `vix-test-sshd` host alias, a `known_hosts` holding exactly that host key,
`StrictHostKeyChecking yes`, no agent), reached through an `ssh` wrapper first on `PATH` that
adds `-F`. Nothing touches `~/.ssh` or the system sshd. It then runs only
`a_real_host_conforms` (`crates/vix-authority/tests/ssh.rs`) with
`VIX_TEST_SSH_HOST=vix-test-sshd`. Opt-in: path-filtered to `crates/vix-authority/**`, the
script and the workflow, plus `workflow_dispatch`, so it never slows an unrelated push or PR.
The per-push gate keeps testing the authority against a fake `ssh`. Developers run the same
script locally (Linux or macOS). Not yet exercised on a runner (added 2026-10-30).

### Feature combinations (T587)

All three forges (a `feature-combos` job on GitHub and GitLab, a step on
Codeberg) run, for the `vix-tui` package: `cargo clippy --no-default-features
--all-targets -- -D warnings`, `cargo test --no-default-features`, and
`cargo hack check --each-feature --no-dev-deps`, so every capability feature
(`spec/cargo-features/index.md`) keeps compiling alone and all-off. `scripts/check`
runs the same three (the `cargo hack` step is skipped with a note when
`cargo-hack` isn't installed). The opt-in `cowork` feature is in no default or
`full` build, so the same job (and `scripts/check`) also runs `cargo clippy
--features cowork --all-targets -- -D warnings` and `cargo test --features cowork
--lib cowork` (its unit and two-peer loopback integration tests). The opt-in
`jupyter` feature is handled the same way: `cargo clippy --features jupyter
--all-targets -- -D warnings`, `cargo test --features jupyter --lib jupyter`
and `cargo test --features jupyter --test jupyter` (the App against an
in-process fake kernel; no Python). The same job (and `scripts/check`) runs
`cargo test -p vix-editor-core --features syntax-all --lib textobject` (T616):
it compiles every bundled `textobjects.scm` against its grammar and runs the
per-language fixtures, which the default grammar set would skip. The checks that read source text or render the
whole product — `tui/tests/action_catalog.rs` (via `dispatch_scan`), the menu-bar
insta snapshots, `tui/tests/integration/menu.rs`'s Alt-letter test, and the
`list_commands --write` / `generate_man` diffs — are **default-feature only**: they
run in the normal job, and the snapshot and menu tests are gated on
`#[cfg(feature = "full")]`. Tests that need one capability gate on its feature
(`#[cfg(feature = "ai")]`, …), the same pattern as `lang-*` grammar tests.

### Semver (T602)

The GitHub `semver` job runs `cargo semver-checks --workspace` (pinned
`0.51.0`) against the version of each crate last published on crates.io. The
release type is inferred from the version in `Cargo.toml`, and an unchanged
version counts as a patch release, so a breaking public-API change fails until
the affected crates are bumped to a new major version. A crate never yet published (no
baseline) is excluded by name until its first release, then the `--exclude`
is dropped. `vix-encoding` (new in T661) is excluded now; `vix-input` and
`vix-tui` were, until their first publish in 1.8.0 (T644). `scripts/check` runs the
same command and skips it with a note when `cargo-semver-checks` (>= 0.51) isn't
installed. GitHub only for now: it needs network access to crates.io and
nothing about it is forge-specific, so GitLab and Codeberg can add it later.

## GitLab

Stages `check` → `test` → `release`, on the pinned `rust:1.97` image (the
MSRV — see [`spec/rust-msrv-n-minus-2/index.md`](../rust-msrv-n-minus-2/index.md)). The
`workflow.rules` block runs pipelines for merge requests, the default branch,
and tags, but never a duplicate branch pipeline beside an open merge request.
Cargo's registry and `target/` are cached per ref.

Its jobs call `scripts/ci` (see "One CI entrypoint"); the `verify` job is
advisory and default-branch only.

The `test` job's shared runner has, more than once, exhausted disk mid-link
building the workspace's test binary (`ld terminated with signal 7 [Bus
error]` / "No space left on device"; T010, `tasks.md`) — confirmed
independent of the `binary-size` job (it has its own, isolated cache; `test`
failed with `binary-size` untouched too). The link is dominated by *debug
info* pulled in from a handful of heavy dependencies (tokio/sqlx/tree-sitter/
image), not code size, so root `Cargo.toml`'s `[profile.test]` sets
`debug = 1` (line tables only) workspace-wide — panic backtraces still
resolve file/line under `RUST_BACKTRACE=1`, just without full variable/type
info. If this alone isn't enough, the next cheapest steps are `cargo test
--no-run` followed by per-crate test runs (so no single link holds the whole
workspace) and then a bigger runner tier.

The release stage runs only for tag pipelines and needs no secrets — GitLab's
`CI_JOB_TOKEN` covers both steps:

1. **release:build** — a static `x86_64-unknown-linux-musl` binary, tarred with
   `README.md`, `CHANGELOG.md`, `LICENSE`, plus a SHA-256 checksum file.
2. **release:upload** — pushes both files to the project's generic package
   registry, giving the assets a permanent URL.
3. **release:publish** — creates the GitLab Release linking those URLs.

## Codeberg

Codeberg runs **Forgejo Actions**, which is broadly GitHub-Actions compatible
with three differences that shape these files:

- Workflows live in `.forgejo/workflows/`, not `.github/workflows/`.
- `uses:` must be a **full URL**
  (`https://code.forgejo.org/actions/checkout@v4`); a bare `owner/action`
  resolves against the instance's default actions host rather than github.com.
- `runs-on: docker` selects Codeberg's shared runner, whose default image
  (`ghcr.io/catthehacker/ubuntu:act-latest`) carries Node — needed by
  JavaScript actions — but no Rust, so the workflows install the pinned
  toolchain with `rustup` and the C toolchain with `apt-get` (the Tree-sitter
  grammars, mimalloc, and image codecs are vendored C).

Codeberg's runners are a donated, shared resource, so CI is a **single job**
that runs the whole gate rather than several jobs that each recompile the
workspace. Since T762 the docs checks (`check-docs`, lychee) live in their own
`docs.yml` so a docs-only change, which `ci.yml` skips, still gets them, and the
slow feature-combination step runs only on schedule, tags and dispatch.

Releases mirror GitLab's: a static musl binary plus checksum, published with
`https://code.forgejo.org/actions/forgejo-release@v2`. It authenticates with
the automatic `FORGEJO_TOKEN`; if that token is read-only on the instance, add
a repository secret `RELEASE_TOKEN` (a personal access token with
`write:repository`) and it takes precedence.

### Enabling Codeberg CI

Actions are opt-in per repository: **Settings → Units → Actions** on
codeberg.org/vixide/vix. Without it, the workflow files are inert.

## Supply chain

See [`SECURITY.md`](../../SECURITY.md) for how to report a vulnerability
privately, supported-version policy, and what's deliberately out of
scope. This section covers only the automated supply-chain scan.

`cargo deny --workspace --all-features check` enforces [`deny.toml`](../../deny.toml)
on every push and pull request, and weekly on all three forges: RUSTSEC
advisories, an allow-list of licenses, wildcard/duplicate bans, and registry
sources.

It sits **outside** the gate above, on purpose. `scripts/check` is reproducible
offline from the lockfile; this is not — the advisory database moves under a
lockfile that has not changed, so a green run yesterday says nothing about
today. That is what the weekly schedule is for. It also means a red `deny` run
is not necessarily a red commit: it is often the world changing, not the tree.

Three details worth knowing before editing those files:

- **`--all-features`** puts the optional Tree-sitter grammar features in the
  graph. Without it, `syntax-all` grammars are invisible to the scan.
- **cargo-deny is pinned and checksummed**, not `cargo install`ed: a
  supply-chain gate should not itself install an unverified binary, and
  nothing in the job needs compiling. Bump the version and the SHA-256
  together, on all three forges.
- **GitLab has no in-YAML cron**, so its weekly run needs a schedule created
  in the UI (**Build → Pipeline schedules**). The `schedule` entry in
  `workflow.rules` is what lets such a schedule run at all.

The license allow-list is permissive-only, plus MPL-2.0 (per-file copyleft,
which does not reach the linking crate). Vix offers itself under
"Apache-2.0 OR BSD-3-Clause OR MIT OR GPL-2.0-only OR GPL-3.0-only", so a
dependency with stronger terms would quietly take that choice away from anyone
who ships a build — this check is what makes that visible. Two consequences
are already recorded in the tree: `evalexpr` is pinned to 11.x because 12.0.0
relicensed to AGPL-3.0-only, and `RUSTSEC-2024-0436` (`paste`, unmaintained)
is ignored with a dated note because it arrives through
`ratatui-image → icy_sixel → quantette → image/avif → rav1e`.

## Generated reference (T305)

`docs/reference/actions.md`, `docs/reference/settings.md`,
`docs/reference/keybindings-shared.md`, and one
`docs/reference/keybindings-<keymap>.md` per keymap are generated, not
hand-maintained: `cargo run --example list_commands -- --write`
(`tui/examples/list_commands.rs`, grown from a plain command lister into this
generator) reads the real data — `vix_menu::menus()`, `vix_palette::COMMANDS`,
`vix_action_catalog::CATALOG` (and its
`dispatch_scan::every_dispatchable_action_id`, shared with
`tui/tests/action_catalog.rs` so both tools' notion of "every action id" can
never drift apart), `vix_settings::Settings::default()` plus a parse of its
own struct's doc comments, and `vix_keybindings::TABLES`/`SHARED` — and
writes the pages fresh every time. All three forges regenerate and
`git diff --exit-code -- docs/reference/`: a stale page (someone edited the
generated file directly, or changed the underlying data without
regenerating) fails the same way a broken link does. The generator fixes its
own locale (`vix_i18n::set_locale("en")`) before writing, so the output is
byte-identical run to run regardless of the environment's locale — required
for the diff check to be meaningful. Plain `cargo run --example
list_commands` (no `--write`) keeps its original, side-effect-free behavior:
print the command-palette list to stdout.

## Man page (T307)

`tui/man/vix.1` is generated too: `cargo run --example generate_man`
(`tui/examples/generate_man.rs`) builds it with `clap_mangen` from
`vix::cli::Cli` — the same `Cli` `tui/src/main.rs` parses, moved into the
library crate (`tui/src/cli.rs`) specifically so the two can't drift apart.
All three forges (and `scripts/check`) regenerate and
`git diff --exit-code -- tui/man/`, same pattern as `docs/reference/`
above. `dist-workspace.toml`'s `include = ["tui/man/vix.1"]` bundles the
committed (and therefore always-current, by the gate above) file into
every release archive and installer, alongside the auto-included
README/CHANGELOG/LICENSE — confirmed with `dist generate --mode ci
--check` that adding `include` doesn't itself change the generated
`release.yml`.

## Binary licenses (T648)

`scripts/check-binary-licenses` runs in `scripts/check` and on all three
forges (GitHub and Codeberg in the lint job beside `check-docs`, GitLab as
the `binary-licenses` job). It reads `cargo metadata --locked` for the root
workspace and for `gui/` (no build), walks each binary package's linked
dependency graph, and fails when a package's declared `license` is not
exactly the subset of Vix's five licenses that graph permits. The rule and
the current results are in `spec/license/index.md` ("Binary licenses").

## Docs links

`lychee` link-checks every `*.md` on every push and pull request, on all three
forges, as two passes:

- **Offline** — local file links only (`--offline`, no network calls), so it
  is deterministic and part of the gate: a broken relative link fails the
  build. `CHANGELOG.md` is excluded, matching `check-docs`'s exemption above —
  its entries pin paths that were current when written and may not exist
  today.
- **External** (`--scheme https --scheme http`) — the `http(s)` links, which
  go stale independently of any change in this tree, so it reports without
  failing the build (`continue-on-error` on GitHub/Codeberg, `allow_failure`
  on GitLab). `CHANGELOG.md` is included here: a URL it names should still
  resolve even though a local path it names may not.

Like `cargo-deny`, `lychee` is pinned and checksummed rather than installed
from a marketplace action or `cargo install`ed, so a docs gate does not itself
pull an unverified binary. Bump `LYCHEE_VERSION`/`LYCHEE_SHA256` together on
all three forges when upgrading. It sits outside `scripts/check` — the offline
pass is reproducible offline, but requires a tool `scripts/check` cannot
assume every contributor has installed; run
`lychee --offline --exclude-path target --exclude-path CHANGELOG.md '**/*.md'`
locally for the same signal `cargo doc` and `check-docs` don't already give you
(lychee also validates in-page anchors and the external check).

GitHub's `docs (lychee)` job once failed downloading the pinned tarball from
GitHub Releases (`curl: (35) Recv failure: Connection reset by peer`,
2026-09-03; T010, `tasks.md`) — a transient network blip, not a code or repo
problem, but avoidable: the job now caches the verified binary keyed on
`LYCHEE_VERSION`+`LYCHEE_SHA256` (`actions/cache@v4`), so only the *first* run
after a version/checksum bump ever hits the network at all; the download
step itself also retries (`curl --retry 3 --retry-all-errors`) for that
first-run case. GitLab's and Codeberg's equivalent downloads have not (yet)
shown this failure, so they are unchanged — revisit if one does.

## Docs site (mdBook)

T301 (`tasks.md`): `book.toml` (`tui/src = "docs"`) plus `docs/SUMMARY.md`
organize the existing `docs/*/index.md` pages — and a handful of
repo-root files reached via `../` links (`AGENTS.md`, `agents/*`,
`AI_STATEMENT.md`, `SECURITY.md`, `CHANGELOG.md`) — into an mdBook site:
Getting Started / Guides / Features / Reference / Contributing.
Deliberately no file moves: `SUMMARY.md` links into the layout as it
already exists, per this task's own instruction.

GitHub-only — GitLab Pages and Codeberg Pages are separate products this
repo does not publish through, so there is no GitLab/Codeberg equivalent
(unlike the gate above, which all three forges mirror):

- **`docs-build`** — installs `mdbook` (pinned + checksummed, same
  pattern as `cargo-deny`/`lychee`), runs `mdbook build` on every push and
  pull request (a broken `SUMMARY.md` should fail the same way a broken
  link does), and uploads the built `book/` as a Pages artifact when the
  push is to `main`.
- **`docs-deploy`** — `needs: docs-build`, gated to `main`; deploys the
  artifact with `actions/deploy-pages@v4` under the `github-pages`
  environment (`pages: write`, `id-token: write` — the only job in this
  file with either permission).

Publishing requires **Settings → Pages → Source: GitHub Actions** enabled
once on `github.com/vixide/vix` — a repository setting, not something a
workflow file can turn on. Until that is set, both jobs still run (so
`mdbook build` stays part of the gate), but `docs-deploy` fails at the
deploy step rather than silently no-op-ing.

`book/` (the build output) is gitignored, same as `target/`.

## Cross-toolchain note

The musl release builds on GitLab and Codeberg override three variables:

```
CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc
CC_x86_64_unknown_linux_musl=musl-gcc
AR_x86_64_unknown_linux_musl=ar
```

`.cargo/config.toml` points that target at the macOS cross-toolchain
(`x86_64-linux-musl-gcc`, installed via Homebrew) because the Makefile
cross-compiles from an Apple Silicon host; on a Debian CI image the toolchain
is Debian's `musl-tools`, which installs `musl-gcc`. Cargo lets the real
environment win over its config `[env]` table — those entries are not
`force = true` — and `CARGO_TARGET_*_LINKER` overrides the config's `linker`,
so the three variables are enough. See `spec/rust-cargo-config-toml-musl`.

## Binary size

T008 (`tasks.md`): a `cargo build --release` (default features — what a
user's build actually gets) that measures the stripped binary
(`target/release/vix`; `[profile.release]` sets `strip = true`) and makes
growth visible, so a regression is a number that moved rather than something
discovered later. Deliberately asymmetric across forges, in proportion to
what each one makes easy:

- **GitHub** — the full version: a `binary-size` job saves every `main`
  build's size under a `binary-size-main-<sha>`-prefixed cache entry (a cache
  key can't be overwritten, so `restore-keys` prefix matching fetches the
  most recent one — the same trick `Swatinem/rust-cache` uses internally). A
  PR only *reads* that prefix, never writes under it, so a PR's speculative
  size can never become the next baseline. A sticky comment (found and
  edited by an HTML marker, not reposted every push) reports the size and the
  delta against that baseline. Best-effort: a fork PR gets a read-only
  `GITHUB_TOKEN` regardless of this job's declared permissions — a hard
  GitHub restriction — so the comment step no-ops rather than failing the
  job; `pull_request_target` would avoid that, but runs workflow code with
  base-repo permissions against the PR's own untrusted content, not a trade
  worth making for a size comment.
- **GitLab** — a lighter `binary-size` job: same measurement, reported to the
  job log rather than a merge-request note (posting one needs an API token
  this pipeline is not given). Its `binary-size-main` cache entry always
  saves — GitLab overwrites a cache key freely, so unlike GitHub's job this
  needs no prefix trick, but it also means a merge-request pipeline can
  transiently overwrite the baseline with its own speculative size until the
  next `main` pipeline corrects it. Accepted deliberately for the
  simplicity; the job log always shows the true delta at build time
  regardless. Its dependency cache is its own key
  (`binary-size-registry-$CI_COMMIT_REF_SLUG`), and it does not cache
  `target/` at all — the first version shared the default
  `$CI_COMMIT_REF_SLUG`/`target/` cache with every other job, so this job's
  `--release` (LTO-heavy) artifacts were folding into the same blob
  `fmt`/`clippy`/`test` restore, inflating what an already
  disk-constrained shared runner has to pull for jobs that have nothing to
  do with a release build.
- **Codeberg — not added.** Its runners are a donated, shared resource
  (see above), already reduced to one job specifically to avoid recompiling
  the workspace more than once; a second full `--release` build (LTO +
  strip, the slowest profile in the tree) for an informational-only metric
  GitHub already reports is not a cost this forge's CI should carry.

## Not yet wired

Nothing outstanding. Spell checking (CSpell, `cspell.json`) used to be listed
here; since 2026-10-25 it is part of `scripts/check-docs`. GitHub and Codeberg
install the pinned CSpell release and set `VIX_REQUIRE_CSPELL=1`, so an
unknown word fails the job. GitLab's `rust` image has no Node, so there the
step is skipped with a notice. Locally it runs whenever CSpell is installed.
