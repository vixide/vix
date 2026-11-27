//! Build script: bundles the **Help → What's New** release notes (T826).
//!
//! Cuts the repository's `CHANGELOG.md` to the sections from
//! `[Unreleased]` through this version's
//! (`vix_welcome_panel::whats_new::excerpt`) and writes them to
//! `$OUT_DIR/whats_new.md`, which `src/app/whats_new.rs` embeds. A build
//! without the changelog (the crates.io package does not carry it) embeds an
//! empty text, and the panel says no notes were bundled.

use std::error::Error;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    let manifest =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").ok_or("no CARGO_MANIFEST_DIR")?);
    let changelog = manifest.join("..").join("CHANGELOG.md");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", changelog.display());
    let text = std::fs::read_to_string(&changelog).unwrap_or_default();
    let version = std::env::var("CARGO_PKG_VERSION")?;
    let out = PathBuf::from(std::env::var_os("OUT_DIR").ok_or("no OUT_DIR")?).join("whats_new.md");
    std::fs::write(out, vix_welcome_panel::whats_new::excerpt(&text, &version))?;
    Ok(())
}
