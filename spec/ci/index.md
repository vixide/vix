# Continuous integration and delivery

Vix lives on three forges — GitHub, GitLab, and Codeberg — and `git push`
pushes to all three (`git remote -v` shows the three push URLs). Each forge
therefore carries its own CI configuration, and all of them enforce the same
gate: **the bar in [`scripts/check`](../../scripts/check)**.

## The gate

Every forge runs, in this order:

```sh
cargo fmt --all --check                                  # spec/rust-cargo-fmt
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings    # spec/rust-clippy-pedantic
cargo test --workspace
cargo doc --workspace --no-deps                          # RUSTDOCFLAGS=-D warnings
python3 scripts/check-docs                               # documentation integrity
```

Nothing merges that does not pass all six. Run `scripts/check` (or
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
| Codeberg | `.forgejo/workflows/ci.yml` | `.forgejo/workflows/release.yml` | `.forgejo/workflows/security.yml` |

When the gate changes, change all four files plus `scripts/check` together.
They are deliberately duplicated rather than abstracted: each forge's syntax is
different enough that a shared script would hide more than it saves.

## GitHub

Three jobs, so a formatting failure is visible without waiting for the tests:

- **lint** — `fmt`, `clippy`, `doc` on `ubuntu-latest`. These findings are
  platform-independent, so they run once rather than per matrix entry.
- **test** — `build` + `test` on `ubuntu-latest` and `macos-latest`. Windows is
  covered by the release workflow's cross-builds; adding it here would roughly
  double wall time for little extra signal, because the editing logic is
  terminal-independent and tested without a TTY.
- **msrv** — `cargo check` on the toolchain floor declared by
  `workspace.package.rust-version` in `Cargo.toml`. Bump the matrix entry when
  that floor moves.

Caching is `Swatinem/rust-cache`; runs are cancelled when superseded on the
same ref.

Releases are produced by [`dist`](https://opensource.axo.dev/cargo-dist/)
(config in `dist-workspace.toml`): pushing a version tag builds the seven
target triples, the shell/PowerShell/npm/Homebrew/MSI installers, and the
GitHub Release, then pushes the formula to `vixide/homebrew-tap` — that last
step is why the `HOMEBREW_TAP_TOKEN` secret exists (see
`spec/homebrew-tap-token`). `release.yml` is **generated**: edit
`dist-workspace.toml` and re-run `dist init`, never the workflow by hand.

## GitLab

Stages `check` → `test` → `release`, on the pinned `rust:1.96` image (the
MSRV — see [`spec/rust-msrv-n-minus-2/index.md`](../rust-msrv-n-minus-2/index.md)). The
`workflow.rules` block runs pipelines for merge requests, the default branch,
and tags, but never a duplicate branch pipeline beside an open merge request.
Cargo's registry and `target/` are cached per ref.

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
workspace.

Releases mirror GitLab's: a static musl binary plus checksum, published with
`https://code.forgejo.org/actions/forgejo-release@v2`. It authenticates with
the automatic `FORGEJO_TOKEN`; if that token is read-only on the instance, add
a repository secret `RELEASE_TOKEN` (a personal access token with
`write:repository`) and it takes precedence.

### Enabling Codeberg CI

Actions are opt-in per repository: **Settings → Units → Actions** on
codeberg.org/vixide/vix. Without it, the workflow files are inert.

## Supply chain

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
  regardless.
- **Codeberg — not added.** Its runners are a donated, shared resource
  (see above), already reduced to one job specifically to avoid recompiling
  the workspace more than once; a second full `--release` build (LTO +
  strip, the slowest profile in the tree) for an informational-only metric
  GitHub already reports is not a cost this forge's CI should carry.

## Not yet wired

Spell checking (CSpell, `cspell.json`) is not a CI job yet: the tree still has
outstanding findings, so it would fail on day one. Tracked in
[`tasks.md`](../../tasks.md).
