//! Two jobs:
//!
//! 1. `rust_i18n::i18n!` used to embed every file under `locales/` at
//!    macro-expansion time, but Cargo has no way to know those source files
//!    affect this crate's output — it only tracks `.rs` files by default.
//!    Without the `cargo:rerun-if-changed` hints below, editing a
//!    `locales/*.yml` file alone never triggers a rebuild, so the compiled
//!    binary silently keeps serving a stale translation table until
//!    something else happens to recompile `vix-i18n`. Declared per file, not
//!    per directory: on most filesystems a directory's own mtime only
//!    changes when an entry is added or removed, not when an existing
//!    file's *content* changes, so watching the directory alone would miss
//!    the common case (editing a translation).
//!
//! 2. Split the merged `locales/*.yml` data by **locale** (T517): one JSON
//!    blob per locale under `$OUT_DIR`, plus a generated `locale_blobs.rs`
//!    listing them as `(code, include_str!(..))` pairs. `src/lib.rs`
//!    `include!`s that list and parses each locale's blob **lazily** (only
//!    the active locale, and English for the fallback walk, ever get
//!    deserialized) instead of the `i18n!` macro's own codegen, which
//!    unconditionally builds every locale's flat map at the first `t!()`
//!    call — see the module doc comment on `lib.rs` for the full story.
//!    Reuses `rust_i18n_support::load_locales` (a build-dependency, feature
//!    `codegen`) so the merge/multi-file/`_version: 2` parsing stays
//!    byte-for-byte the same code path the `i18n!` macro itself used.

use std::collections::BTreeMap;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

fn main() {
    let dir = Path::new("../../locales");
    println!("cargo:rerun-if-changed={}", dir.display());
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        println!("cargo:rerun-if-changed={}", entry.path().display());
    }

    let locales: BTreeMap<String, BTreeMap<String, String>> =
        rust_i18n_support::load_locales("../../locales", |_| false);

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR is set during a build script");
    let mut code = String::from("pub(crate) static LOCALE_BLOBS: &[(&str, &str)] = &[\n");
    for (locale, messages) in &locales {
        let json = serde_json::to_string(messages).expect("a BTreeMap<String, String> serializes");
        let blob_path = Path::new(&out_dir).join(format!("locale_{locale}.json"));
        fs::write(&blob_path, json).unwrap_or_else(|e| {
            panic!("failed to write {}: {e}", blob_path.display());
        });
        let _ = writeln!(
            code,
            "    ({locale:?}, include_str!({:?})),",
            blob_path.display()
        );
    }
    code.push_str("];\n");
    let list_path = Path::new(&out_dir).join("locale_blobs.rs");
    fs::write(&list_path, code)
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", list_path.display()));
}
