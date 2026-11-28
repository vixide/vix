# Packaging channels

Beyond the channels `dist` publishes itself (Homebrew, the shell and
PowerShell installers, npm, the MSI, and the Linux packages of
[`spec/linux-packages`](../linux-packages/index.md)), Vix keeps in-repo
templates for winget, Scoop, the AUR, `cargo binstall`, mise and a Nix flake
(T742, T743). This spec says what those files must guarantee and what the
repository does and does not do with them.

## Hard rules

- **Nothing here publishes by itself.** No workflow submits to winget-pkgs,
  pushes a Scoop bucket or the AUR, or opens a pull request on another
  repository unless a maintainer has both set the repository variable
  `PACKAGING_PUBLISH` to `true` and created the named secret. Absent either,
  the job is skipped.
- **Templates describe a real release.** `packaging/` carries a real version,
  real asset URLs and the real SHA-256 sums of that release; the asset names
  follow dist's scheme (no `v` in the tag, `vix-tui-<target>.tar.xz` with a
  `vix-tui-<target>/` directory on Unix, `.zip` with `vix.exe` at the root on
  Windows).
- **A channel is listed as installable only after it was verified on a real
  machine or runner and published.** Until then the docs list it under "Not
  yet published".

## The tooling

- `scripts/packaging check` is a static, offline consistency check, part of
  `scripts/check`: all manifests carry one version; every URL embeds that
  version and the right target; every hash is 64 hex digits and winget and
  Scoop agree where they name the same asset; every target named is in
  `dist-workspace.toml`; the `cargo binstall` metadata and the Nix flake are
  present; the PKGBUILD passes `bash -n`; the JSON and YAML parse.
- `scripts/packaging render <version>` rewrites the templates for a release.
  Sums come from the release's own `<asset>.sha256` files, or from
  `--sums FILE` (lines of `sha256sum` output) for an offline run. Windows ARM
  entries appear only when that asset exists (2.0.0 and later).
  A download that fails with a server error or a network error is retried
  up to five times; a 404 means the asset has no sum.

## Verifying a download

Administrators get one tested verification path per distribution route
(T870, audit item PQ-09), written up in
[`docs/verify-release`](../../docs/verify-release/index.md). For each route
the page names the producer, the exact file, the verification material, what
that material is trusted through, the command and the expected result, and it
keeps three kinds of evidence apart:

- **Signed manifest.** `SHA256SUMS` with `SHA256SUMS.sig` (`ssh-keygen -Y`,
  namespace `vix-release`), only from `scripts/release-local` (2.0.0 so far).
  Trusted through the maintainer's SSH signing key. The repository does not
  publish an allowed-signers file yet, so the page says to obtain the key out
  of band.
- **Checksums.** `<file>.sha256`, `sha256.sum` (dist) and
  `linux-packages.sha256`: integrity against the same release page, not
  authenticity.
- **Registry integrity and provenance.** Homebrew's formula SHA-256, npm's
  registry signatures and (only for versions published through OIDC)
  provenance, and crates.io's index checksums.

SBOMs are listed as dependency evidence, not as integrity material. Every
channel that is not published (winget, Scoop, AUR, `cargo binstall`, Nix,
mise, the container image) is listed under "Not published" with no route.

`scripts/verify-release DIR` is the offline verifier. It checks the
signature of `SHA256SUMS` against an allowed-signers file before trusting any
line of it, then every listed file against every manifest that names it
(`SHA256SUMS`, `sha256.sum`, `linux-packages.sha256`, each `<file>.sha256`,
which must name its own file). It rejects a missing listed file, a mismatch, a
disagreement between manifests, a name that is not a plain file name, and any
file in `DIR` that nothing covers. `--require-signature` also rejects every
file the signed manifest does not list. It prints each file's SHA-256, target
and coverage, and `--record FILE` writes them as tab-separated values with
the version. When a release has both, the signed manifest decides the
coverage.

`scripts/test-verify-release` (push tier) runs it in a clean environment
(`env -i`) on fixtures (real archives, dist-style checksum files, a
Linux-packages checksum file, an SBOM, a release signed with a generated key)
and checks that each modified fixture is rejected;
`scripts/test-release-local` runs it on what `scripts/release-local` really
produces. `scripts/packaging check` fails when the page misses one of
`dist-workspace.toml`'s installers or a fixed route, gives an unpublished
channel a route, or when `SECURITY.md` stops linking it or again says the
release artifacts are unsigned. Not yet done: running the page's
instructions on real downloads of a published release per route and recording
target, version and digest; that needs network access.

## CI

`.github/workflows/packaging.yml` (GitHub only):

| Job | When | What |
|---|---|---|
| `check` | every run | `scripts/packaging check` |
| `render` | release tag, manual | renders the manifests for the tag and uploads them as a workflow artifact |
| `verify-nix` | manual, release tag, changes to `flake.nix` or `Cargo.lock` | `nix flake check`, `nix build`, `vix --version` |
| `verify-winget` | manual, release tag | `winget validate` on a Windows runner |
| `verify-aur` | manual, release tag | `makepkg` of the PKGBUILD in an Arch container |
| `publish-*` | release tag, only with `PACKAGING_PUBLISH=true` and the secret | submit to winget-pkgs (`WINGET_PKGS_TOKEN`), push the Scoop bucket (`SCOOP_BUCKET_TOKEN`), push the AUR (`AUR_SSH_PRIVATE_KEY`) |

The verify and publish jobs have not run yet. The secrets are placeholders
only: none exists, and none is ever committed.

## Manual remainder

Creating `vixide/scoop-bucket`, the tokens and the AUR account, the first
winget-pkgs pull request (Microsoft reviews it), the nixpkgs and mise-registry
submissions, and making the container package public stay manual. They are
listed in [`packaging/index.md`](../../packaging/index.md).
