# Debian

Vix ships `.deb` packages, plus `.rpm`, `.apk` and Arch packages, built from
each release's Linux binaries; see
[`spec/linux-packages/index.md`](../linux-packages/index.md). What follows
describes the release pipeline before that, and is kept for its pointer to
the generic Debian packaging reference.

Before T668, Vix did not ship a `.deb` package. Releases are produced by
[`dist`](https://opensource.axo.dev/cargo-dist/) (see
[`spec/ci/index.md`](../ci/index.md)): shell/PowerShell installer scripts, an
npm package, a Homebrew formula, and a Windows MSI — no Debian packaging in
the pipeline today. [`debian-packaging.md`](debian-packaging.md) is a generic
reference on how Debian packaging works in general, kept here as a starting
point if `.deb` support is ever added; it is not a description of how Vix
itself ships.
