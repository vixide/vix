//! Available UI languages and the locale-chooser selection state.
//!
//! Pure data: each [`Locale`] pairs a code (used with `rust-i18n`) with its
//! endonym (the language's name in itself, the convention for language pickers).
//! The host applies the selection via `rust_i18n::set_locale` and persists it.

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

/// Selection state for the locale chooser overlay. Moving the selection previews
/// the language live; the host commits or reverts.
pub struct Chooser {
    /// Index into [`LOCALES`] of the highlighted language.
    pub selected: usize,
    /// Index of the language active when the chooser opened, restored on cancel.
    pub original: usize,
}

impl Chooser {
    /// Open the chooser highlighting `current_code` (or the first locale if the
    /// code is not in [`LOCALES`]).
    #[must_use]
    pub fn open(current_code: &str) -> Self {
        let selected = LOCALES
            .iter()
            .position(|l| l.code == current_code)
            .unwrap_or(0);
        Chooser { selected, original: selected }
    }

    /// Highlight the previous language, wrapping around.
    pub fn up(&mut self) {
        self.selected = (self.selected + LOCALES.len() - 1) % LOCALES.len();
    }

    /// Highlight the next language, wrapping around.
    pub fn down(&mut self) {
        self.selected = (self.selected + 1) % LOCALES.len();
    }

    /// The highlighted language's code.
    #[must_use]
    pub fn selected_code(&self) -> &'static str {
        LOCALES[self.selected].code
    }

    /// The code of the language active when the chooser opened.
    #[must_use]
    pub fn original_code(&self) -> &'static str {
        LOCALES[self.original].code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_on_current_and_navigates() {
        let mut c = Chooser::open("fr");
        assert_eq!(c.selected_code(), "fr");
        assert_eq!(c.original_code(), "fr");
        c.up();
        assert_eq!(c.selected_code(), "es");
        c.down();
        assert_eq!(c.selected_code(), "fr");
    }

    #[test]
    fn unknown_code_defaults_to_first() {
        let c = Chooser::open("zz");
        assert_eq!(c.selected_code(), LOCALES[0].code);
    }

    #[test]
    fn navigation_wraps() {
        let mut c = Chooser::open("en");
        c.up();
        assert_eq!(c.selected_code(), LOCALES[LOCALES.len() - 1].code);
    }
}
