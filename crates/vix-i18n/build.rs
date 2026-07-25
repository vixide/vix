//! `rust_i18n::i18n!` embeds `locales/app.yml` at macro-expansion time, but
//! Cargo has no way to know that source file affects this crate's output —
//! it only tracks `.rs` files by default. Without this build script, editing
//! `locales/app.yml` alone never triggers a rebuild, so the compiled binary
//! silently keeps serving a stale translation table until something else
//! happens to recompile `vix-i18n` (e.g. touching its own source).
fn main() {
    println!("cargo:rerun-if-changed=../../locales/app.yml");
}
