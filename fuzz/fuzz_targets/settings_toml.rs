//! Fuzz the TOML settings loader (`vix-settings`), from a string only.
//!
//! `config.toml` and a project's `.vix/settings.toml` are hand-edited files,
//! so reading one must be total: TOML that does not parse, a key of the wrong
//! type, an unknown key or a deeply nested table is reported, never a panic.
//! This target never reads or writes a real config path.

#![no_main]

use libfuzzer_sys::fuzz_target;
use vix_settings::{Settings, language_key_problems, merge_layer, unknown_keys};

fuzz_target!(|text: &str| {
    // The whole-file loader (what `confy` does with `config.toml`).
    if let Ok(settings) = toml::from_str::<Settings>(text) {
        let _ = language_key_problems(&settings);
    }

    // The layer path: parse a table, report unknown keys, merge over defaults.
    let Ok(layer) = text.parse::<toml::Table>() else {
        return;
    };
    let _ = unknown_keys(&layer);
    let Ok(merged) = merge_layer(&Settings::default(), &layer) else {
        return;
    };
    let _ = language_key_problems(&merged);

    // Merged settings save and reload unchanged: serializing them and
    // merging that table over the defaults again is a fixed point.
    let table = toml::Table::try_from(&merged).expect("merged settings serialize");
    // What Vix itself writes never holds a key it would warn about.
    assert_eq!(
        unknown_keys(&table),
        Vec::<String>::new(),
        "saved settings hold unknown keys"
    );
    let reloaded = merge_layer(&Settings::default(), &table).expect("saved settings reload");
    let table2 = toml::Table::try_from(&reloaded).expect("reloaded settings serialize");
    assert_eq!(table2, table, "saving and reloading settings changed them");
});
