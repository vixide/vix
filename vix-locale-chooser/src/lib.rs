//! The locale-chooser selection state.
//!
//! The language list itself lives in [`vix_locale_model`]; this crate re-exports
//! [`Locale`] and [`LOCALES`] and adds the overlay's selection state. Moving the
//! selection previews the language live; the host commits via
//! `rust_i18n::set_locale` and persists it.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub use vix_locale_model::{Locale, LOCALES};

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
