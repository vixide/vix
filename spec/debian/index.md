# Debian

Vix does not currently ship a `.deb` package. Releases are produced by
[`dist`](https://opensource.axo.dev/cargo-dist/) (see
[`spec/ci/index.md`](../ci/index.md)): shell/PowerShell installer scripts, an
npm package, a Homebrew formula, and a Windows MSI — no Debian packaging in
the pipeline today. [`debian-packaging.md`](debian-packaging.md) is a generic
reference on how Debian packaging works in general, kept here as a starting
point if `.deb` support is ever added; it is not a description of how Vix
itself ships.
