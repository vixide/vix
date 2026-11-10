# Vix Packaging and Distribution Guide

This guide documents all Vix packaging channels, their current status, and
instructions for publishing and verifying each channel.

## Overview

Vix aims to be **easy to install everywhere**. This document tracks:

- ✅ **Currently published** channels
- 🟡 **Ready to publish** templates (exist but not yet published)
- ⏳ **Planned** channels (not yet implemented)

## Currently Published Channels

These channels are actively maintained and verified:

### 1. Homebrew (macOS/Linux)

**Status**: ✅ Published

**Install:**
```sh
brew install vixide/homebrew-tap/vix-tui
```

**Maintenance:**
- Formula is in `vixide/homebrew-tap` repository
- Updated automatically on new releases via GitHub Actions

---

### 2. npm

**Status**: ✅ Published

**Install:**
```sh
npm install -g @vixide/vix
```

**Maintenance:**
- Package is `@vixide/vix`
- Published via GitHub Actions on release
- Uses trusted publishing with OIDC

---

### 3. cargo install

**Status**: ✅ Published

**Install:**
```sh
cargo install vix-tui
```

**Features:**
- Install from crates.io
- Use `--features syntax-all` for all Tree-sitter grammars
- Use `--no-default-features` for minimal build

---

### 4. GitHub Releases

**Status**: ✅ Published

**Assets per release:**
- Linux: `.tar.xz` (x86_64, aarch64)
- macOS: `.tar.xz` (x86_64, aarch64)
- Windows: `.zip` + `.msi` (x86_64, aarch64)
- SHA-256 checksums for all assets
- Minimal builds (`vix-minimal-<target>`)

**Download:**
```sh
# Latest release
curl -L https://github.com/vixide/vix/releases/latest/download/vix-tui-$(uname -m)-unknown-linux-gnu.tar.xz | tar xJ
```

---

### 5. Linux Packages

**Status**: ✅ Published (built but distribution varies)

**Formats:**
- `.deb` (Debian/Ubuntu) - x86_64, aarch64
- `.rpm` (Fedora/RHEL) - x86_64, aarch64
- Alpine `.apk` - x86_64 (static musl build)

**Note:** These are built and attached to releases but may not be in official
repositories yet.

---

### 6. Windows MSI

**Status**: ✅ Published

**Install:**
- Download `.msi` from GitHub Releases
- Install via Windows Installer

---

### 7. Installer Scripts

**Status**: ✅ Published

**Install:**
```sh
# macOS/Linux
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/vixide/vix/releases/latest/download/vix-tui-installer.sh | sh

# Windows (PowerShell)
powershell -ExecutionPolicy Bypass -c "irm https://github.com/vixide/vix/releases/latest/download/vix-tui-installer.ps1 | iex"
```

---

## Ready to Publish Templates

These templates exist in `packaging/` and are ready for publication:

### 8. AUR (Arch Linux)

**Status**: 🟡 Template exists, not yet published

**Files:**
- [`packaging/aur/vix-tui-bin/PKGBUILD`](https://github.com/vixide/vix/blob/main/packaging/aur/vix-tui-bin/PKGBUILD)

**Features:**
- Repackages the official glibc release archive
- Supports x86_64 and aarch64
- Conflicts with `vix-tui` package (which is built from source)

**To Publish:**

1. **Create AUR account** (if you don't have one):
   - Register at [AUR.archlinux.org](https://aur.archlinux.org/)
   - Add SSH public key to your account

2. **Clone the AUR repository:**
   ```sh
   git clone ssh://aur@aur.archlinux.org/vix-tui-bin.git
   cd vix-tui-bin
   ```

3. **Copy the template:**
   ```sh
   cp /path/to/vix/packaging/aur/vix-tui-bin/PKGBUILD .
   ```

4. **Update for the version you're publishing:**
   - Update `pkgver` to the release version (e.g., `2.0.2`)
   - Update `source_*` URLs to match the release
   - Update `sha256sums_*` with the actual hashes

5. **Verify the package:**
   ```sh
   makepkg -si  # Install locally
   vix --version
   namcap PKGBUILD *.pkg.tar.zst  # Check for issues
   ```

6. **Generate .SRCINFO:**
   ```sh
   makepkg --printsrcinfo > .SRCINFO
   ```

7. **Push to AUR:**
   ```sh
   git add PKGBUILD .SRCINFO
   git commit -m "Update to vix-tui <version>"
   git push origin main
   ```

**Verification:**
- After pushing, users can install with:
  ```sh
  yay -S vix-tui-bin
  # or
  paru -S vix-tui-bin
  ```

**Updates:**
- Use `scripts/packaging render <version>` to update hashes automatically
- Run `makepkg -si` to verify the new version works
- Push updates to AUR

---

### 9. winget (Windows)

**Status**: 🟡 Template exists, not yet published

**Files:**
- [`packaging/winget/vixide.vix-tui.yaml`](https://github.com/vixide/vix/blob/main/packaging/winget/vixide.vix-tui.yaml)
- [`packaging/winget/vixide.vix-tui.locale.en-US.yaml`](https://github.com/vixide/vix/blob/main/packaging/winget/vixide.vix-tui.locale.en-US.yaml)
- [`packaging/winget/vixide.vix-tui.installer.yaml`](https://github.com/vixide/vix/blob/main/packaging/winget/vixide.vix-tui.installer.yaml)

**Features:**
- Installs the portable `.zip` release
- Supports x86_64 and ARM64
- Command alias: `vix`
- Auto-update via GitHub releases

**To Publish:**

#### First-Time Submission:

1. **Validate the manifest locally:**
   ```powershell
   cd packaging\winget
   winget validate --manifest .
   ```

2. **Test installation locally:**
   ```powershell
   winget install --manifest . --force --accept-source-agreements
   vix --version
   ```

3. **Fork the winget-pkgs repository:**
   ```sh
   git clone https://github.com/microsoft/winget-pkgs.git
   cd winget-pkgs
   ```

4. **Create the package directory:**
   ```sh
   mkdir -p manifests/v/vixide/vix-tui/2.0.2
   ```

5. **Copy the template files:**
   ```sh
   cp /path/to/vix/packaging/winget/*.yaml manifests/v/vixide/vix-tui/2.0.2/
   ```

6. **Create a PR to winget-pkgs:**
   - Push to your fork
   - Open a PR to `microsoft/winget-pkgs`
   - Wait for Microsoft's review and approval

#### Updates After First Publication:

Use `wingetcreate` to automate updates:

```powershell
wingetcreate update vixide.vix-tui --version <version> --urls <zip-url> --submit
```

**Verification:**
- After merging, users can install with:
  ```powershell
  winget install vixide.vix-tui
  ```

---

### 10. Scoop (Windows)

**Status**: 🟡 Template exists, not yet published

**File:**
- [`packaging/scoop/vix-tui.json`](https://github.com/vixide/vix/blob/main/packaging/scoop/vix-tui.json)

**Features:**
- Installs from GitHub release `.zip`
- Supports x86_64 and ARM64
- Auto-update via GitHub releases
- Checkver for version detection

**To Publish:**

#### Option A: Create Your Own Bucket (Recommended for Testing)

1. **Create a new repository:**
   ```sh
   gh repo create vixide/scoop-bucket --public
   cd scoop-bucket
   ```

2. **Add the manifest:**
   ```sh
   mkdir -p bucket
   cp /path/to/vix/packaging/scoop/vix-tui.json bucket/
   ```

3. **Create `bucket.json`:**
   ```json
   {
     "version": 1,
     "description": "Vixide apps for Scoop",
     "maintainers": ["joelparkerhenderson"]
   }
   ```

4. **Add the bucket to Scoop:**
   ```powershell
   scoop bucket add vixide https://github.com/vixide/scoop-bucket
   ```

5. **Install and verify:**
   ```powershell
   scoop install vix-tui
   vix --version
   ```

#### Option B: Submit to Scoop Extras

1. **Fork Scoop:**
   ```sh
   git clone https://github.com/ScoopInstaller/Scoop.git
   cd Scoop
   ```

2. **Add to extras bucket:**
   ```sh
   cp /path/to/vix/packaging/scoop/vix-tui.json bucket/vix-tui.json
   ```

3. **Create a PR** to `ScoopInstaller/Scoop`

**Verification:**
- After merging, users can install with:
  ```powershell
  scoop install extras/vix-tui
  ```

---

### 11. Docker Container

**Status**: 🟡 Built but not public

**Files:**
- [`docker/Dockerfile`](https://github.com/vixide/vix/blob/main/docker/Dockerfile)
- [`.dockerignore`](https://github.com/vixide/vix/blob/main/.dockerignore)
- [`workflows/container.yml`](https://github.com/vixide/vix/blob/main/.github/workflows/container.yml)

**Features:**
- Multi-arch (amd64, arm64)
- Alpine-based
- Static binary
- Supports volume mounting for editing local files

**To Publish:**

1. **Make the package public:**
   - Go to GitHub → vixide → Packages
   - Click on `vix` package
   - Go to Package settings
   - Change visibility from **Private** to **Public**

2. **Verify:**
   ```sh
   docker pull ghcr.io/vixide/vix:latest
   docker run --rm -it ghcr.io/vixide/vix:latest --version
   ```

**Usage:**
```sh
# Run with demo workspace
docker run --rm -it ghcr.io/vixide/vix:latest

# Edit local files
docker run --rm -it -v "$PWD:/work" -w /work ghcr.io/vixide/vix:latest .
```

---

### 12. Nix Flake

**Status**: 🟡 Template exists, not yet published

**File:**
- [`flake.nix`](https://github.com/vixide/vix/blob/main/flake.nix)

**Features:**
- Builds `vix-tui` from source with default features
- Can be run directly from GitHub

**To Publish:**

1. **Generate flake.lock:**
   ```sh
   cd /path/to/vix
   nix flake check
   nix build
   git add flake.lock
   git commit -m "Add flake.lock for Nix"
   git push
   ```

2. **Verify:**
   ```sh
   nix run github:vixide/vix -- --version
   ```

**Usage:**
```sh
# Run directly from GitHub
nix run github:vixide/vix

# Build and run
nix build github:vixide/vix
./result/bin/vix
```

---

### 13. cargo binstall

**Status**: 🟡 Metadata exists, not yet verified

**File:**
- Metadata in [`tui/Cargo.toml`](https://github.com/vixide/vix/blob/main/tui/Cargo.toml)

**Features:**
- Installs pre-built binaries from releases
- Falls back to compiling if binaries unavailable

**To Verify:**

```sh
# Test on each platform
cargo binstall --no-confirm --disable-strategies quick-install,compile vix-tui
vix --version
```

**Note:** The metadata is already in the published `vix-tui` 2.0.2 crate on
crates.io, so `cargo binstall vix-tui` should work once verified.

---

### 14. mise

**Status**: 🟡 Works but not officially documented

**Features:**
- Can install from crates.io (via cargo-binstall)
- Can install directly from GitHub releases

**Verification:**

```sh
# From crates.io
mise use -g cargo:vix-tui

# From GitHub releases
mise use -g ubi:vixide/vix[matching=vix-tui,exe=vix]

# Verify
vix --version
```

**Note:** The `ubi:` form needs `matching=vix-tui` because each release carries
both `vix-tui` and GUI archives.

---

## Planned Channels

These channels are planned but not yet implemented:

### 15. Chocolatey (Windows)

**Status**: ⏳ Not yet implemented

**To Implement:**
- Create `chocolatey/vix-tui.nuspec` template
- Follow Chocolatey packaging guidelines
- Submit to Chocolatey Community Repository

---

### 16. Flatpak

**Status**: ⏳ Not yet implemented

**To Implement:**
- Create `flatpak/org.vixide.vix.json` manifest
- Build and test on Flatpak Builder
- Submit to Flathub

---

### 17. Snap

**Status**: ⏳ Not yet implemented

**To Implement:**
- Create `snap/snapcraft.yaml`
- Build and test locally
- Submit to Snap Store

---

## Publishing Checklist

For each new channel, follow this checklist:

### Before Publishing

- [ ] Template exists in `packaging/` directory
- [ ] Template has been verified to work locally
- [ ] Template uses correct version and URLs
- [ ] All checksums/hashs are valid
- [ ] Auto-update mechanism is configured (if applicable)
- [ ] Documentation is updated

### Publishing Steps

- [ ] Create necessary accounts (AUR, winget-pkgs, etc.)
- [ ] Submit template to appropriate repository
- [ ] Wait for review/approval (if required)
- [ ] Verify installation works from published channel
- [ ] Update `docs/getting-started/index.md` with new channel
- [ ] Update README.md with new channel
- [ ] Update website install page

### After Publishing

- [ ] Announce on GitHub Releases
- [ ] Update CHANGELOG.md
- [ ] Monitor for issues
- [ ] Set up CI to verify future releases (if applicable)

---

## Maintenance

### Updating Templates for New Releases

When a new Vix version is released, update all templates:

```sh
# Update version and hashes in all templates
cd /path/to/vix

# Render winget, Scoop, AUR templates from release assets
scripts/packaging render <version>

# Commit changes
git add packaging/
git commit -m "packaging: update for v<version>"
```

The `scripts/packaging render` command:
- Downloads `.sha256` files from the release
- Updates version numbers
- Updates download URLs
- Updates SHA-256 hashes
- Adds ARM64 Windows entry if asset exists

### Verifying Channels

CI workflows can verify templates:

```sh
# In .github/workflows/packaging.yml
scripts/packaging check  # Offline validation
nix build                # Verify Nix flake
winget validate         # Verify winget manifest
makepkg                 # Verify AUR PKGBUILD
```

---

## Current Status Summary

| Channel | Status | Owner | Priority | Notes |
|---------|--------|-------|----------|-------|
| Homebrew | ✅ Published | Maintainer | High | Working |
| npm | ✅ Published | Maintainer | High | Working |
| cargo install | ✅ Published | Maintainer | High | Working |
| GitHub Releases | ✅ Published | Maintainer | High | Working |
| Linux packages | ✅ Published | Maintainer | High | Built, distribution varies |
| Windows MSI | ✅ Published | Maintainer | High | Working |
| Installer scripts | ✅ Published | Maintainer | High | Working |
| **AUR** | 🟡 Ready | Maintainer | High | Template exists |
| **winget** | 🟡 Ready | Maintainer | High | Template exists |
| **Scoop** | 🟡 Ready | Maintainer | High | Template exists |
| **Docker** | 🟡 Ready | Maintainer | High | Built, not public |
| **Nix flake** | 🟡 Ready | Maintainer | Medium | Template exists |
| **cargo binstall** | 🟡 Ready | Maintainer | Medium | Metadata exists |
| **mise** | 🟡 Ready | Maintainer | Medium | Works, not documented |
| Chocolatey | ⏳ Planned | - | Low | Not implemented |
| Flatpak | ⏳ Planned | - | Low | Not implemented |
| Snap | ⏳ Planned | - | Low | Not implemented |

---

## Recommendations

### Priority 1: Publish the Ready Templates (Next 30 Days)

1. **AUR** - High impact, large Arch Linux user base
2. **winget** - High impact, Windows users expect this
3. **Docker** - High impact, container users expect this
4. **Scoop** - Medium impact, popular with Windows devs

### Priority 2: Verify and Document (Next 3 Months)

1. **cargo binstall** - Verify it works
2. **Nix flake** - Generate and commit flake.lock
3. **mise** - Document official support

### Priority 3: Future Enhancements (Long Term)

1. **Chocolatey** - Another Windows option
2. **Flatpak** - For Linux desktop users
3. **Snap** - For Ubuntu users

---

## Success Metrics

Track these metrics to measure packaging success:

| Metric | Current | 3 Month Goal | 6 Month Goal |
|--------|---------|--------------|--------------|
| Download channels | 8 | 12 | 15 |
| Monthly downloads | ~5,000 | 10,000 | 25,000 |
| AUR votes | 0 | 50 | 100 |
| winget downloads | 0 | 500 | 1,000 |
| Scoop installs | 0 | 500 | 1,000 |
| Docker pulls | 0 | 1,000 | 5,000 |

---

## Resources

- [Packaging Spec](https://github.com/vixide/vix/blob/main/spec/packaging/index.md)
- [scripts/packaging](https://github.com/vixide/vix/tree/main/scripts/packaging)
- [.github/workflows/packaging.yml](https://github.com/vixide/vix/blob/main/.github/workflows/packaging.yml)
- [dist-workspace.toml](https://github.com/vixide/vix/blob/main/dist-workspace.toml)

---

## Conclusion

Vix has an excellent foundation with 8 published channels and 7 more ready
to publish. Publishing the ready templates (AUR, winget, Scoop, Docker) would
significantly improve discoverability and accessibility, especially for
Windows and Linux users.

**The templates are ready. They just need to be published.**
