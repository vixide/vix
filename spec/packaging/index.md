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
