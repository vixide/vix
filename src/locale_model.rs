#![allow(clippy::pedantic)] // folded subcrate: kept at its original (non-pedantic) lint level
//! Available UI languages for Vix.
//!
//! Pure data: each [`Locale`] pairs a code (used with `rust-i18n`) with its
//! endonym (the language's name in itself, the convention for language pickers).
//! The locale chooser lists these; the host applies a selection via
//! `rust_i18n::set_locale` and persists it. Extracted from the former
//! `vix-locale-chooser` so the data has its own home.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// One selectable UI language.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Locale {
    /// Locale code passed to `rust-i18n` (e.g. `"en"`).
    pub code: &'static str,
    /// Endonym shown in the chooser (e.g. `"Español"`).
    pub name: &'static str,
}

/// All bundled locales, in chooser order. English is first (the fallback).
///
/// Each language is selectable; translation coverage is filled in incrementally
/// in `locales/app.yml`, with any untranslated key falling back to English.
pub const LOCALES: &[Locale] = &[
    Locale { code: "en", name: "English" },
    Locale { code: "es", name: "Español" },
    Locale { code: "fr", name: "Français" },
    Locale { code: "de", name: "Deutsch" },
    Locale { code: "cy", name: "Cymraeg" },
    Locale { code: "ga", name: "Gaeilge" },
    Locale { code: "gd", name: "Gàidhlig" },
    Locale { code: "pl", name: "Polski" },
    Locale { code: "pt", name: "Português" },
    Locale { code: "ru", name: "Русский" },
    Locale { code: "ar", name: "العربية" },
    Locale { code: "hi", name: "हिन्दी" },
    Locale { code: "bn", name: "বাংলা" },
    Locale { code: "zh", name: "中文" },
    Locale { code: "ja", name: "日本語" },
    Locale { code: "it", name: "Italiano" },
    Locale { code: "ko", name: "한국어" },
    Locale { code: "tr", name: "Türkçe" },
    Locale { code: "nl", name: "Nederlands" },
    Locale { code: "vi", name: "Tiếng Việt" },
    Locale { code: "id", name: "Bahasa Indonesia" },
    Locale { code: "th", name: "ไทย" },
    Locale { code: "fa", name: "فارسی" },
    Locale { code: "uk", name: "Українська" },
    Locale { code: "el", name: "Ελληνικά" },
    // Constructed languages, last.
    Locale { code: "tlh", name: "tlhIngan Hol" },
    Locale { code: "sjn", name: "Edhellen" },
];

/// The locale with the given `code`, if bundled.
#[must_use]
pub fn by_code(code: &str) -> Option<&'static Locale> {
    LOCALES.iter().find(|l| l.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_first_and_lookup_works() {
        assert_eq!(LOCALES[0].code, "en");
        assert_eq!(by_code("fr").map(|l| l.name), Some("Français"));
        assert!(by_code("zz").is_none());
    }
}
