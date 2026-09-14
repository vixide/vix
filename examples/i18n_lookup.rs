//! Look up one translation key in every locale Vix ships (improvement plan
//! T503) -- `vix_i18n::t!("key", locale = "xx")` resolves against a specific
//! locale directly, without touching the process-global active locale
//! `rust_i18n::set_locale` would otherwise change.
//!
//! Run with: `cargo run --example i18n_lookup -- [key]`
//! Defaults to `status.opened` (a key that also takes an interpolated
//! argument, to show that working too).

#![warn(clippy::pedantic)]

// `t!`'s expansion needs `_rust_i18n_try_translate` in *this* crate's root --
// see `vix_i18n::surface!`'s own doc comment.
vix_i18n::surface!();

// The 15 locale codes every key in `locales/*.yml` carries. `available_locales!()`
// exists too, but (like `t!`) its expansion only resolves inside a crate that
// called `rust_i18n::i18n!` itself -- `vix_i18n::surface!` doesn't re-export
// it, so a plain list is simpler here than fighting the macro hygiene.
const LOCALES: &[&str] = &[
    "en", "es", "fr", "de", "cy", "ga", "gd", "pl", "pt", "ru", "ar", "hi", "bn", "zh", "ja",
];

fn main() {
    let key = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "status.opened".to_string());

    for locale in LOCALES {
        let text = vix_i18n::t!(&key, locale = *locale, path = "example.rs");
        println!("{locale:>3}: {text}");
    }
}
