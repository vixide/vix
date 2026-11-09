# Running the gate on Linux and Windows from a Mac

Vix's quality bar is `scripts/check`, run on the maintainer's machine. A Mac
cannot show you what breaks on Linux or Windows, and the hosted macOS and
Windows runners are the expensive ones, so they run only on version tags and
the weekly schedule. This page shows how to cover both operating systems
yourself, before a tag finds the problem. The design is in
[`spec/local-ci`](../../spec/local-ci/index.md).

## Linux, in a container

```sh
scripts/check-linux                  # the push tier, in a Linux container
scripts/check-linux --tier fast      # fmt, clippy, tests of changed crates
scripts/check-linux --shell          # a shell in the same container, for debugging
```

You need the `docker` command talking to a running daemon. Any of these works
(the script installs nothing and tells you which one is missing):

- Colima: `brew install colima docker && colima start`
- OrbStack: `brew install --cask orbstack`, then open it once
- Docker Desktop
- Podman: `podman machine start`, then either the `docker` command on
  Podman's Docker-compatible socket or `VIX_CONTAINER_CLI=podman`

What the script does:

- Builds a small image, once per toolchain, on the official
  `rust:<version>-bookworm` image, where `<version>` is your own
  `rustc --version`, so the container lints with the same clippy as
  `scripts/check` on your Mac (with no `rustc` installed it falls back to the
  MSRV, `rust-version` in `Cargo.toml`). It adds clippy, rustfmt and the few
  system tools the gate needs. Set `VIX_LINUX_RUST=<version>` to pick another
  toolchain. Do not pick the MSRV for the gate: clippy's lints change between
  releases, so `-D warnings` at the MSRV fails on code stable accepts.
- Limits cargo to one job per 3 GiB of the engine's memory (at most one per
  CPU), because the VM a Mac engine runs has no swap and linking one test
  binary on `aarch64` took 2.5 GiB; one job per CPU got the linker killed.
  `VIX_LINUX_JOBS=<n>` overrides it. Giving the VM more memory makes it faster.
- Mounts your repository **read-only** and copies it into a Docker volume, so
  your working tree is never written. A throwaway git repository is made there
  because the gate runs `git diff` on the generated reference docs.
- Keeps `target/` and the cargo registry in named volumes
  (`vix-check-linux-target`, `vix-check-linux-cargo`), so the second run is
  incremental. `docker volume rm vix-check-linux-work vix-check-linux-target
  vix-check-linux-cargo` starts clean. The target volume grows to about 45 GB
  for the push tier, so the engine's VM disk needs that much free (an old
  toolchain's build stays in it until you empty it).
- Runs as your user id, not root, with all capabilities dropped. It mounts no
  credentials: not `~/.ssh`, not your git config, not the Docker socket. At
  run time the only network use is cargo fetching crates (building the image
  also reaches apt and rustup).

`VIX_CI_DRY_RUN=1 scripts/check-linux` prints the Dockerfile and the exact
`docker run` command instead of running it.

A pass here means "the push tier passes on Linux". It is not the signed
attestation (`scripts/attest` still wants a full run), and a Linux container on
Apple Silicon is `aarch64`, not the `x86_64` that hosted CI uses.

Tested on 2026-11-09 with Podman on an Apple Silicon Mac, both through the
`docker` command and with `VIX_CONTAINER_CLI=podman`: the push tier passed,
in about 12 minutes from a cold cache with a 12 GiB VM and about 80 seconds
on a rerun. Docker Desktop, Colima and OrbStack have not been tried, nor the
`fast` and `full` tiers or `--shell`.

## Windows, in a virtual machine

> **Untested by the author on real Windows hardware.** These steps follow the
> hosted `windows-latest` job in `.github/workflows/ci.yml` and the vendors'
> documentation, but nobody has yet run them end to end. Treat the first run
> as the test and report what differs.

Pick a Windows 11 machine:

- On Apple Silicon: UTM (free), Parallels Desktop or VMware Fusion running
  **Windows 11 ARM64**. Microsoft publishes an ARM64 Windows 11 disk image and
  ISO. This tests `aarch64-pc-windows-msvc`, one of the shipped targets, but
  not the `x86_64` target the hosted job tests.
- Any machine: a cloud Windows VM (Azure, AWS or similar) on `x86_64`, which
  matches the hosted job. Stop or delete it when done; it bills while it runs.

Give it at least 4 CPU cores, 8 GB of memory and 60 GB of disk. Then, in an
administrator PowerShell:

```powershell
winget install --id Git.Git -e
winget install --id Python.Python.3.12 -e
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
winget install --id Rustlang.Rustup -e
```

Open a new terminal (so `PATH` is refreshed), then:

```powershell
rustup default stable
rustup component add clippy rustfmt
git config --global core.autocrlf false
git clone https://github.com/vixide/vix.git
cd vix
```

Line endings matter: with `core.autocrlf` on, the generated-docs check and
some tests see `\r\n` and fail for reasons that are not real bugs.

Run the jobs in **Git Bash** (installed with Git), because `scripts/ci` is a
POSIX shell script and the hosted job also uses bash:

```sh
scripts/ci build      # cargo build --workspace --all-targets
scripts/ci test       # the test suite
scripts/ci examples   # the headless examples
```

Those three are what the hosted Windows job runs. `scripts/ci clippy` and
`scripts/ci fmt` are also fine. Do not expect `scripts/check` as a whole to
work: it assumes Linux or macOS tools in places, and the hosted job does not
run it either. If a test fails on Windows, run just that package with
`cargo test -p <crate>` and compare against the hosted weekly run.

The GTK 4 front end (`vix-gtk4`) cannot build on Windows; the hosted GUI job
excludes it too.

## What stays hosted

Hosted Windows and macOS run on version tags and the weekly schedule, not on
every push (see [`spec/ci`](../../spec/ci/index.md), "Slim hosted CI"). The
local routes above are how you find an OS-specific failure before a tag does.
