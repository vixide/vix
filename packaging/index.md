# Packaging

Templates for install channels that Vix does **not publish yet** (T742,
T743 in [`tasks.md`](../tasks.md)). Nothing here has been submitted
anywhere, except that the container image is built and pushed to ghcr.io on
every version tag, where the package is not public yet. A channel moves into the main install list of
[`docs/getting-started`](../docs/getting-started/index.md) only after a
maintainer has verified it on a real machine or CI runner and published it.

The channels that do ship today (Homebrew, the shell and PowerShell
installers, npm, the MSI, `.deb`/`.rpm`/Arch packages and the release
archives) are all built by `dist` and `.github/workflows/linux-packages.yml`;
see [`dist-workspace.toml`](../dist-workspace.toml).

## Tooling and CI (spec: [`spec/packaging`](../spec/packaging/index.md))

- `scripts/packaging check` (offline; part of `scripts/check`) keeps the
  templates consistent: one version, matching URLs, well-formed hashes, dist
  targets, binstall metadata, the flake, `bash -n` on the PKGBUILD.
- `scripts/packaging render <version>` rewrites the winget, Scoop and AUR
  templates for a release from its `.sha256` files (or `--sums FILE`); an
  arm64 Windows entry appears when that asset exists (2.0.0 and later).
- [`.github/workflows/packaging.yml`](../.github/workflows/packaging.yml)
  renders and verifies them after a release (`nix build`, `winget validate`,
  `makepkg`) and has publish jobs for winget-pkgs, a Scoop bucket and the
  AUR. The publish jobs do nothing unless the repository variable
  `PACKAGING_PUBLISH` is `true` **and** the secret exists:
  `WINGET_PKGS_TOKEN`, `SCOOP_BUCKET_TOKEN`, `AUR_SSH_PRIVATE_KEY`. None of
  these exists. The workflow has never run.

## What stays manual

1. Verify each channel on a real machine (the "Verify first" steps below).
2. Commit `flake.lock` from the first real `nix build`.
3. Create `vixide/scoop-bucket`, the AUR account and `vix-tui-bin` package,
   and the three tokens above; set `PACKAGING_PUBLISH=true`.
4. Submit the first winget-pkgs pull request by hand and wait for Microsoft's
   review; only then can `wingetcreate update` take over.
5. Test `cargo binstall vix-tui` (the metadata is in the published 2.0.2
   crate, untested), make the ghcr.io package public, and optionally submit to nixpkgs and mise's registry.
6. Move each verified channel into the main install list of
   [`docs/getting-started`](../docs/getting-started/index.md), the README and
   the site's install page.

## What the release really ships

Every template below points at the real assets of a GitHub Release, checked
against the 1.8.0 release:

- Tags have no `v` prefix: `releases/download/1.8.0/…`.
- Archives are `vix-tui-<target>.tar.xz` (macOS, Linux) with the files under
  a `vix-tui-<target>/` directory: `vix`, `vix.1`, `README.md`,
  `CHANGELOG.md`, `LICENSE`.
- Windows is `vix-tui-<target>.zip` with `vix.exe` (and the same files) at
  the root, plus an `.msi`.
- Every archive has a `<name>.sha256` beside it.
- `aarch64-pc-windows-msvc` (T658c) is new in 2.0.0, so 1.8.0 has no
  Windows ARM assets.

The templates carry version 1.8.0 and its real SHA-256 sums, so they can be
checked against a release that exists. Bump them to the release being
submitted (version, URLs, hashes) before submitting.

## The channels

### `cargo binstall`

- **File:** `[package.metadata.binstall]` in
  [`tui/Cargo.toml`](../tui/Cargo.toml).
- **Publish:** nothing to submit. The metadata is in the published
  `vix-tui` 2.0.2 crate, so `cargo binstall vix-tui` should download the
  release archive instead of compiling; nobody has tried it yet.
- **Verify first:** on macOS, Linux and Windows:
  `cargo binstall --no-confirm --disable-strategies quick-install,compile
  vix-tui`, then `vix --version`. The two Windows overrides (`.zip`, binary
  at the root) matter most.

### Nix flake

- **File:** [`flake.nix`](../flake.nix). It builds `vix-tui` from source
  with the default features.
- **Publish:** nothing to submit. Once `flake.nix` and a `flake.lock` are on
  `main`, `nix run github:vixide/vix` works.
- **Verify first:** on a machine with Nix (Linux and macOS):
  `nix flake check`, `nix build`, `./result/bin/vix --version`,
  `nix run . -- --version`. Commit the `flake.lock` that the first build
  writes. If nixpkgs' Rust is older than `rust-version` in `Cargo.toml`,
  pin a newer nixpkgs or add a Rust overlay.
- **Later:** a `vix` package in nixpkgs itself is a separate submission.

### Container image (`ghcr.io/vixide/vix`)

- **Files:** [`docker/Dockerfile`](../docker/Dockerfile),
  [`.dockerignore`](../.dockerignore) and
  [`.github/workflows/container.yml`](../.github/workflows/container.yml).
- **Try it locally:**

  ```sh
  docker build -f docker/Dockerfile -t vix .
  docker run --rm -it vix                              # the demo workspace
  docker run --rm -it -v "$PWD:/work" -w /work vix .   # your own files
  ```

- **Publish:** the workflow pushes `ghcr.io/vixide/vix:<tag>` and `:latest`
  on a release tag, and did for 2.0.2. The package is still
  private, so an anonymous `docker pull` fails with HTTP 403. Make the
  package public (GitHub → vixide → Packages → vix → Package settings →
  Change visibility) and link it to the repository.

### winget

- **Files:** [`winget/`](winget/), a three-file manifest (version,
  installer, default locale) for `vixide.vix-tui`, installing the portable
  `.zip` with the command alias `vix`.
- **Publish:** on Windows, `winget validate --manifest packaging\winget`,
  then `winget install --manifest packaging\winget` and `vix --version` in
  a new terminal. Then copy the three files to
  `manifests/v/vixide/vix-tui/<version>/` in a fork of
  [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) and open
  a pull request (or run `wingetcreate update vixide.vix-tui --version
  <version> --urls <zip-url> --submit` after the first one is merged).
- **For 2.0.0:** add an `arm64` installer entry for
  `vix-tui-aarch64-pc-windows-msvc.zip`.

### Scoop

- **File:** [`scoop/vix-tui.json`](scoop/vix-tui.json), with `checkver` and
  `autoupdate` (`autoupdate` already lists the `arm64` zip of 2.0.0).
- **Publish:** either create a bucket repository (for example
  `vixide/scoop-bucket`, then `scoop bucket add vixide
  https://github.com/vixide/scoop-bucket` and `scoop install vix-tui`) or
  submit to Scoop's `extras` bucket. Verify first on Windows with
  `scoop install .\packaging\scoop\vix-tui.json`, then `vix --version`.

### AUR (`vix-tui-bin`)

- **File:** [`aur/vix-tui-bin/PKGBUILD`](aur/vix-tui-bin/PKGBUILD). It
  repackages the glibc release archive for x86_64 and aarch64, and
  conflicts with the `vix-tui` Arch package that each release already
  attaches.
- **Publish:** on Arch, in that directory: `makepkg -si`, `vix --version`,
  `namcap PKGBUILD *.pkg.tar.zst`, then `makepkg --printsrcinfo >
  .SRCINFO`. Push `PKGBUILD` and `.SRCINFO` to
  `ssh://aur@aur.archlinux.org/vix-tui-bin.git` from an AUR account with an
  SSH key.

### mise

- **File:** none needed. mise can install from crates.io or straight from
  the GitHub Release:

  ```sh
  mise use -g cargo:vix-tui        # via cargo-binstall when mise finds it, else compiles
  mise use -g ubi:vixide/vix[matching=vix-tui,exe=vix]   # the release archive
  ```

- **Verify first:** both commands on macOS and Linux, then `vix --version`.
  The `ubi:` form needs `matching=vix-tui`, because each release also
  carries the GUIs' archives.
- **Later:** an entry in mise's registry would allow `mise use -g vix`;
  that is a pull request to [jdx/mise](https://github.com/jdx/mise).
