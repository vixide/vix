//! `rust_i18n::i18n!` embeds every file under `locales/` at macro-expansion
//! time, but Cargo has no way to know those source files affect this crate's
//! output — it only tracks `.rs` files by default. Without this build
//! script, editing a `locales/*.yml` file alone never triggers a rebuild, so
//! the compiled binary silently keeps serving a stale translation table
//! until something else happens to recompile `vix-i18n` (e.g. touching its
//! own source). Declared per file, not per directory: on most filesystems a
//! directory's own mtime only changes when an entry is added or removed, not
//! when an existing file's *content* changes, so watching the directory
//! alone would miss the common case (editing a translation).
fn main() {
    let dir = std::path::Path::new("../../locales");
    println!("cargo:rerun-if-changed={}", dir.display());
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        println!("cargo:rerun-if-changed={}", entry.path().display());
    }
}
