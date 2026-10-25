# Linux packages

Every release carries native Linux packages beside dist's archives and
installers (T668), so Linux users can install with their own package manager:

| Distribution | Format | Install |
|---|---|---|
| Debian, Ubuntu and derivatives | `.deb` | `sudo apt install ./vix-tui_<version>-1_amd64.deb` |
| Fedora, RHEL, Rocky, Alma, openSUSE | `.rpm` | `sudo dnf install ./vix-tui-<version>-1.x86_64.rpm` |
| Arch Linux | `.pkg.tar.zst` | `sudo pacman -U ./vix-tui-<version>-1-x86_64.pkg.tar.zst` |

`apt`, `dnf` and `pacman` also accept the release URL directly for `.rpm`
(`dnf install https://…`); for `.deb` download the file first.

## What is packaged

- **Packages:** `vix-tui` (the `vix` command) and the four released desktop
  GUIs, `vix-gpui`, `vix-egui`, `vix-iced` and `vix-slint`. `vix-gtk4` is
  not released, so it has no package.
- **Architectures:** x86_64 (`amd64`) and aarch64 (`arm64`).
- **No rebuild.** `scripts/build-linux-packages` repackages the binaries in
  the release's own `<package>-<target>.tar.xz` archives with
  [nfpm](https://nfpm.goreleaser.com/), so a package's binary is
  byte-for-byte the one in the archive.
- **Files:**
  - the binary in `/usr/bin`
  - the license in `/usr/share/licenses/<package>/` (and, for `.deb`,
    `/usr/share/doc/<package>/copyright`)
  - README and CHANGELOG in `/usr/share/doc/<package>/`
  - `vix(1)` in `/usr/share/man/man1/` (`vix-tui`)
  - a `.desktop` launcher in `/usr/share/applications/` (each GUI)
- **License field:** each package declares its binary's own license, read
  from its `Cargo.toml` (see [`spec/license/index.md`](../license/index.md),
  "Binary licenses"), so `vix-slint` is GPL-3.0-only.

## Which build, and what it needs

- **Every package is a glibc build**, needing glibc 2.35 or newer: Ubuntu
  22.04+, Debian 12+, Fedora 36+, RHEL/Rocky 10. RHEL/Rocky 8 and 9 have an
  older glibc, so they can't use these packages; there, use `cargo install
  vix-tui` or build from source.
- **No Alpine package.** Alpine uses musl. The release's
  `x86_64-unknown-linux-musl` build should be the one to package, but it
  comes out dynamically linked against musl's loader rather than static, and
  under emulation it crashes at startup. That build is fixed in T670, and
  the `.apk` (and a glibc-free `vix-tui` for RHEL 8/9) follows from it.
- **Dependencies.** The packages declare glibc, plus each binary's directly
  linked libraries:
  - `vix-gpui`: xkbcommon, xkbcommon-x11, xcb
  - `vix-slint`: fontconfig
- **Run-time libraries.** The GUIs load the windowing and GPU libraries with
  `dlopen`, so these never appear as linked libraries: xkbcommon, Wayland or
  X11 with Xcursor/Xrandr/Xi, and Vulkan (`vix-gpui`, `vix-iced`) or
  EGL/GL (`vix-egui`, `vix-slint`).
  - `.deb` and `.rpm` list them as *recommends*.
  - Arch lists them as dependencies, because pacman has no recommends.

## Pipeline

- **Release.** `.github/workflows/linux-packages.yml` is a dist custom
  publish job (`publish-jobs = ["homebrew", "./linux-packages"]` in
  `dist-workspace.toml`), so it runs after the GitHub Release is created. It:
  1. downloads the release's Linux archives;
  2. builds the packages;
  3. install-tests them on Debian 12, Ubuntu 22.04, Fedora 41 and Arch. Each test checks that the package manager resolves the
     dependencies, `vix --version` runs, and every GUI's `ldd` has no
     "not found";
  4. uploads the packages and `linux-packages.sha256` to the release.
- **Manual run.** The same workflow can be run by hand (`workflow_dispatch`
  with a tag) to add packages to an existing release.
- **Locally.** `python3 scripts/build-linux-packages --version <v>
  --archives <dir> --out <dir>` uses nfpm from `PATH`, or otherwise the
  pinned `goreleaser/nfpm` image through docker or podman.

## Not yet

- **Package repositories.** There is no apt or yum *repository* yet, so
  `apt install vix-tui` without a file name doesn't work and updates don't
  arrive through `apt upgrade`/`dnf upgrade`. A repository needs a GPG
  signing key and hosting (tracked as T669 in `tasks.md`).
- **Distribution archives.** Getting Vix into Debian, Fedora, Alpine or the
  AUR proper is a packaging and maintenance commitment per distribution, not
  a build step.
- **Signatures.** The `.deb` and `.rpm` packages are unsigned. Use the
  release's `linux-packages.sha256` to verify downloads.
