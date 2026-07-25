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
```

Nothing merges that does not pass all five. Run `scripts/check` (or
`make check`) locally first; CI should only ever confirm what the local gate
already said.

## Files

| Forge    | CI                          | CD                              |
| -------- | --------------------------- | ------------------------------- |
| GitHub   | `.github/workflows/ci.yml`  | `.github/workflows/release.yml` |
| GitLab   | `.gitlab-ci.yml`            | `release` stage of the same file |
| Codeberg | `.forgejo/workflows/ci.yml` | `.forgejo/workflows/release.yml` |

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

Stages `check` → `test` → `release`, on the pinned `rust:1.96` image. The
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

## Not yet wired

Tracked in [`tasks.md`](../../tasks.md):

- **T002** — markdown link checking (`lychee`) alongside the `cargo doc` job.
- **T003** — `deny.toml` and a `cargo deny check` job (licenses, advisories,
  duplicate versions).

Spell checking (CSpell, `cspell.json`) is not a CI job yet: the tree still has
outstanding findings, so it would fail on day one.
