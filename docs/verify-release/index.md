# Verifying a Vix download

For administrators installing Vix: for each way Vix is distributed, what
produced the file, what to check it against, what that check actually proves,
and the exact command. It was written for audit item PQ-09 (T870).

Three kinds of evidence appear below, and they answer different questions:

- **A signed manifest** (`SHA256SUMS` with `SHA256SUMS.sig`) ties every file it lists
  to the maintainer's SSH signing key. Only releases made with
  `scripts/release-local` have one. So far that is 2.0.0 alone.
- **Checksums** (`<file>.sha256`, `sha256.sum`, `linux-packages.sha256`) come from the same
  release page as the files. They prove that the download is complete and is
  the file that page lists. They do not prove who made it: someone who can
  change the page can change both.
- **Registry integrity and provenance** (Homebrew, npm, crates.io) is checked
  by the package manager against its own index or registry. Trust rests on
  that registry and on the account or workflow allowed to publish to it.

The **SBOMs** (`<program>.cdx.xml`) list what a program was built from. They are
evidence for a dependency review, not a way to check the binaries.

## Routes at a glance

| Route | Status | Producer | Verification material | Trust source |
|---|---|---|---|---|
| Release archives | published (2.0.1 and later) | `dist` in `.github/workflows/release.yml` | `<archive>.sha256`, `sha256.sum` | the GitHub Release page (HTTPS) |
| Locally signed release | published once (2.0.0) | `scripts/release-local` | `SHA256SUMS`, `SHA256SUMS.sig` | the maintainer's SSH signing key |
| Shell and PowerShell installers | published | `dist` | `sha256.sum` | the GitHub Release page |
| Windows MSI | published | `dist` (WiX) | `sha256.sum` | the GitHub Release page; the MSI is not code-signed |
| Linux packages | published | `.github/workflows/linux-packages.yml` | `linux-packages.sha256` | the GitHub Release page; the packages are not signed |
| Minimal build | published | `.github/workflows/release-minimal.yml` | `<archive>.sha256` | the GitHub Release page |
| Homebrew | published | `dist`, pushed to `vixide/homebrew-tap` | the SHA-256 in the formula | the tap repository |
| npm (`@vixide/vix`) | published | `.github/workflows/publish-npm.yml` | registry signature; provenance when published through OIDC | the npm registry |
| crates.io (source) | published | `cargo publish` by the maintainer | the index checksum of each `.crate` | crates.io |
| SBOMs | published | `dist` and `.github/workflows/sbom.yml` | `sha256.sum` where it lists them | the GitHub Release page |

## The verifier

`scripts/verify-release DIR` checks a directory of downloaded files offline, in
one pass, with every kind of material above that is in it. Download each
release into its own new, empty directory (the script rejects any file nothing
covers), then run:

```sh
scripts/verify-release DIR                                  # checksums
scripts/verify-release DIR --allowed-signers FILE --require-signature   # signed manifest
scripts/verify-release DIR --version 3.0.0 --record verified.tsv        # keep a record
```

It checks the signature of `SHA256SUMS` (namespace `vix-release`) before it trusts
any line of it, then that every listed file exists and matches, that all
manifests naming a file agree, and that each `.sha256` names its own file. It
refuses a name with a directory part. Each verified file is printed with its
SHA-256, its target (read from the asset name) and what covered it, for
example:

```text
ok  vix-tui-x86_64-unknown-linux-gnu.tar.xz  sha256=…  target=x86_64-unknown-linux-gnu  checksum only (checksum:sha256.sum,…)
scripts/verify-release: OK 6 file(s) in …: 0 in the signed SHA256SUMS, 6 by checksums only
```

The exit status is 0 only when everything verified. `--require-signature`
also fails any file the signed manifest does not list. The script needs only
`sh`, `awk`, `sha256sum` or `shasum`, and `ssh-keygen` for a signed manifest. Each
route below also gives the plain command, for a machine without the script.

## Release archives

- **Files:** `vix-tui-<target>.tar.xz` (`.zip` on Windows), and the same for each
  desktop GUI (`vix-gpui-<target>.tar.xz` and so on).
- **Material:** `<archive>.sha256` beside each, and `sha256.sum` for the whole
  release.
- **Command:** `sha256sum -c vix-tui-x86_64-unknown-linux-gnu.tar.xz.sha256`
  (macOS: `shasum -a 256 -c`), or `sha256sum -c --ignore-missing sha256.sum`.
- **Expected:** `vix-tui-x86_64-unknown-linux-gnu.tar.xz: OK`; a modified file
  prints `FAILED` and the command exits non-zero.

## Locally signed release

- **Files:** `vix-tui-<target>.tar.xz` / `.zip` for the three Makefile targets,
  their `.sha256`, `SHA256SUMS` and `SHA256SUMS.sig` (see
  [Releasing](../release/index.md)).
- **Trust source:** the maintainer's SSH signing key, the one that signs
  commits, in an allowed-signers file (`principal key-type key` per line).
  Vix does not publish that file yet; get the key from the maintainer through a
  channel you already trust.
- **Command:**

  ```sh
  ssh-keygen -Y verify -f allowed_signers -I <principal> -n vix-release -s SHA256SUMS.sig < SHA256SUMS
  sha256sum -c SHA256SUMS
  ```

- **Expected:** `Good "vix-release" signature for <principal>`, then `OK` for
  every file. An edited `SHA256SUMS`, another key or another namespace fails the
  first command; a modified archive fails the second.
  `scripts/release-local --verify-dir DIR` runs the same two checks.

## Shell and PowerShell installers

- **Files:** `vix-tui-installer.sh` and `vix-tui-installer.ps1` (one pair per GUI).
- **Material:** their lines in `sha256.sum`.
- **Command:** download the installer instead of piping it into a shell,
  check it with `sha256sum -c --ignore-missing sha256.sum`, read it, then run it.
  The installer downloads the release archive for your platform; to check
  that archive yourself, download it as under Release archives instead.
- **Expected:** `vix-tui-installer.sh: OK`.

## Windows MSI

- **File:** `vix-tui-<target>.msi` (and one per GUI). It is not Authenticode
  signed, so SmartScreen warns about an unknown publisher.
- **Command (PowerShell):**
  `(Get-FileHash .\vix-tui-x86_64-pc-windows-msvc.msi -Algorithm SHA256).Hash`
  and compare it with the file's line in `sha256.sum`.
- **Expected:** the two values are equal (case aside).

## Linux packages

- **Files:** `.deb`, `.rpm`, `.apk` and `.pkg.tar.zst` of `vix-tui` and the GUIs
  ([`spec/linux-packages`](../../spec/linux-packages/index.md)). They carry no
  package signature: the `.apk` needs `--allow-untrusted`, and there is no apt or
  dnf repository key.
- **Material:** `linux-packages.sha256`, written by the same job after its
  install tests.
- **Command:** `sha256sum -c --ignore-missing linux-packages.sha256`.
- **Expected:** `OK` for each package you downloaded.
- **Evidence:** before uploading, the job installs the packages on Debian 12,
  Ubuntu 22.04, Fedora 41 and Arch (and the static ones on Alpine and Rocky
  8 and 9) and runs `vix --version` there; `scripts/test-build-linux-packages`
  tests which build each package takes.

## Minimal build

- **Files:** `vix-minimal-<target>.tar.gz` (`.zip` on Windows), uploaded after the
  rest of the release, so not in `sha256.sum`.
- **Command:** `sha256sum -c vix-minimal-x86_64-unknown-linux-musl.tar.gz.sha256`.
- **Expected:** `OK`.

## Homebrew

- **Producer:** `dist` writes the formula with the archive's URL and SHA-256 and
  pushes it to the `vixide/homebrew-tap` repository.
- **Command:** `brew fetch --formula vixide/homebrew-tap/vix-tui` downloads the
  archive and checks it against the formula; `brew install` does the same.
- **Expected:** a `SHA256:` line equal to the archive's line in `sha256.sum`; a
  mismatch stops with `SHA256 mismatch`. That check trusts the tap, so comparing
  with `sha256.sum` is what ties it to the release.

## npm

- **Producer:** `.github/workflows/publish-npm.yml` publishes `@vixide/vix` with npm
  trusted publishing (OIDC) and keeps a token as a fallback until the trusted
  publisher is configured, so a version may have been published either way.
- **Command:** in a project that installed it, `npm audit signatures`.
- **Expected:** the package counts among the verified registry signatures. It
  also counts among the verified attestations only if that version was
  published through OIDC; one published with the token has no provenance.
- **Limit:** the package fetches the platform's release archive from the
  GitHub Release when it installs; npm's signature does not cover that
  download.

## crates.io (source)

- **Command:** `cargo install --locked vix-tui` builds from source.
- **Material and trust:** Cargo checks each downloaded `.crate` against the
  SHA-256 in the crates.io index, and `--locked` holds every dependency to the
  versions and checksums in the published `Cargo.lock`. crates.io has no
  publisher signature; trust rests on crates.io and the owners of each crate.
- **Expected:** the build finishes; a checksum mismatch stops it with an error
  naming the crate.

## SBOMs

- **Files:** `vix-tui.cdx.xml` from `dist`'s CycloneDX step and one per desktop GUI
  from `.github/workflows/sbom.yml`, both made by the same pinned
  `cargo-cyclonedx` ([`spec/ci`](../../spec/ci/index.md)).
- **Use:** verify the file like any release asset, then feed it to your
  dependency or license review. It says what went into the build; it does not
  check a binary.

## Not published

These channels have templates in [`packaging/`](../../packaging/index.md) but
are not published, so there is nothing to verify yet. Do not install Vix
through them.

- **winget**: manifest not submitted to winget-pkgs.
- **Scoop**: no bucket exists.
- **AUR** (`vix-tui-bin`): not pushed; its PKGBUILD pins the release's SHA-256
  sums.
- **cargo binstall**: the metadata ships in the crate, untested.
- **Nix** flake: not built with Nix yet; it builds from source.
- **mise**: untested.
- **Container image** (`ghcr.io/vixide/vix`): pushed on each release, but the
  package is private.

The [getting-started page](../getting-started/index.md) keeps the same list.

## How this is tested

- `scripts/test-verify-release` builds release fixtures (real archives, `dist`-style
  checksum files, a Linux-packages checksum file, an SBOM, and a release
  signed with a generated key), verifies each in a clean environment
  (`env -i`), and checks that every modified fixture is rejected: an archive, a
  package, an SBOM, a rewritten `SHA256SUMS`, a missing or foreign signature, a
  missing or extra file, a name with a directory part.
- `scripts/test-release-local` runs the verifier on what
  `scripts/release-local` really produces, with `--require-signature`, and on a
  tampered copy.
- `scripts/packaging check` fails when this page misses a `dist` installer or a
  route, lists an unpublished channel as a route, or when `SECURITY.md` no
  longer agrees with it.

**Not yet done:** running these instructions on real downloads of a published
release, per route, on a clean machine, and recording target, version and
digest (`--record` writes that record). That needs network access to the
release and the registries; the fixtures above stand in for it.
