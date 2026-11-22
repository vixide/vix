//! Available UI languages for Vix.
//!
//! Pure data: each [`Locale`] pairs a code (used with `rust-i18n`) with its
//! endonym (the language's name in itself, the convention for language pickers).
//! The locale chooser lists these; the host applies a selection via
//! `rust_i18n::set_locale` and persists it. Extracted from the former
//! `vix-locale-chooser` so the data has its own home.

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

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
/// in `locales/`, with any untranslated key falling back to English.
pub const LOCALES: &[Locale] = &[
    Locale {
        code: "en",
        name: "English",
    },
    Locale {
        code: "es",
        name: "Español",
    },
    Locale {
        code: "fr",
        name: "Français",
    },
    Locale {
        code: "de",
        name: "Deutsch",
    },
    Locale {
        code: "cy",
        name: "Cymraeg",
    },
    Locale {
        code: "ga",
        name: "Gaeilge",
    },
    Locale {
        code: "gd",
        name: "Gàidhlig",
    },
    Locale {
        code: "pl",
        name: "Polski",
    },
    Locale {
        code: "pt",
        name: "Português",
    },
    Locale {
        code: "ru",
        name: "Русский",
    },
    Locale {
        code: "ar",
        name: "العربية",
    },
    Locale {
        code: "hi",
        name: "हिन्दी",
    },
    Locale {
        code: "bn",
        name: "বাংলা",
    },
    Locale {
        code: "zh",
        name: "中文",
    },
    Locale {
        code: "ja",
        name: "日本語",
    },
    Locale {
        code: "it",
        name: "Italiano",
    },
    Locale {
        code: "ko",
        name: "한국어",
    },
    Locale {
        code: "tr",
        name: "Türkçe",
    },
    Locale {
        code: "nl",
        name: "Nederlands",
    },
    Locale {
        code: "vi",
        name: "Tiếng Việt",
    },
    Locale {
        code: "id",
        name: "Bahasa Indonesia",
    },
    Locale {
        code: "th",
        name: "ไทย",
    },
    Locale {
        code: "fa",
        name: "فارسی",
    },
    Locale {
        code: "uk",
        name: "Українська",
    },
    Locale {
        code: "el",
        name: "Ελληνικά",
    },
    // Constructed languages, last.
    Locale {
        code: "tlh",
        name: "tlhIngan Hol",
    },
    Locale {
        code: "sjn",
        name: "Edhellen",
    },
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

/// Whether a locale code names a right-to-left language (Arabic, Hebrew,
/// Persian, Urdu, …). Only the primary subtag matters.
#[must_use]
pub fn is_rtl(code: &str) -> bool {
    let primary = code.split(['-', '_']).next().unwrap_or("");
    matches!(
        primary,
        "ar" | "he" | "fa" | "ur" | "yi" | "ps" | "sd" | "ug"
    )
}

/// Visual (left-to-right screen) order of one line of text, per UAX #9.
///
/// `chars` is the line in logical order, one entry per screen unit. Returns,
/// for each screen position, the logical index to draw there. The base
/// direction comes from the first strong character. A line with no
/// right-to-left character comes back as the identity permutation.
///
/// # Errors
/// [`BidiError::Inconsistent`] if the bidi library returns no paragraph for text
/// that has a right-to-left character, or a run with no level (a broken library
/// contract, never expected); no order is guessed.
///
/// # Examples
///
/// ```
/// let order = vix_locale_model::visual_order(&['a', 'b']).unwrap();
/// assert_eq!(order, vec![0, 1]);
/// ```
pub fn visual_order(chars: &[char]) -> Result<Vec<usize>, BidiError> {
    let identity: Vec<usize> = (0..chars.len()).collect();
    if !chars.iter().any(|&c| strong_rtl(c)) {
        return Ok(identity);
    }
    let text: String = chars.iter().collect();
    let info = unicode_bidi::BidiInfo::new(&text, None);
    let inconsistent = BidiError::Inconsistent {
        figure: "bidi levels",
    };
    let para = info.paragraphs.first().ok_or(inconsistent)?;
    let starts: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    let (levels, runs) = info.visual_runs(para, para.range.clone());
    let mut out = Vec::with_capacity(chars.len());
    for run in runs {
        let first = starts.partition_point(|&b| b < run.start);
        let end = starts.partition_point(|&b| b < run.end);
        let odd = levels.get(run.start).ok_or(inconsistent)?.is_rtl();
        if odd {
            out.extend((first..end).rev());
        } else {
            out.extend(first..end);
        }
    }
    Ok(out)
}

/// Why a visual order could not be computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BidiError {
    /// The bidi library's answer does not match the text it was given.
    Inconsistent {
        /// The figure that is inconsistent.
        figure: &'static str,
    },
}

impl std::fmt::Display for BidiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Inconsistent { figure } => write!(f, "{figure}: inconsistent with the text"),
        }
    }
}

impl std::error::Error for BidiError {}

fn strong_rtl(c: char) -> bool {
    matches!(
        unicode_bidi::bidi_class(c),
        unicode_bidi::BidiClass::R | unicode_bidi::BidiClass::AL
    )
}

#[cfg(test)]
mod bidi_tests {
    use super::*;

    #[test]
    fn rtl_codes() {
        assert!(is_rtl("ar") && is_rtl("he-IL") && !is_rtl("en") && !is_rtl("ru"));
    }

    #[test]
    fn ltr_line_is_identity() {
        let c: Vec<char> = "hello 42".chars().collect();
        assert_eq!(visual_order(&c).unwrap(), (0..c.len()).collect::<Vec<_>>());
    }

    #[test]
    fn rtl_line_reverses_and_keeps_embedded_numbers_ltr() {
        // "ابج 12" : logical ا ب ج space 1 2 ; RTL base, digits stay in order.
        let c: Vec<char> = "ابج 12".chars().collect();
        assert_eq!(visual_order(&c).unwrap(), vec![4, 5, 3, 2, 1, 0]);
    }

    #[test]
    fn is_a_permutation() {
        let c: Vec<char> = "abc ابج def 9 ه".chars().collect();
        let mut o = visual_order(&c).unwrap();
        o.sort_unstable();
        assert_eq!(o, (0..c.len()).collect::<Vec<_>>());
    }

    #[test]
    fn a_bidi_error_names_the_figure() {
        assert_eq!(
            BidiError::Inconsistent {
                figure: "bidi levels"
            }
            .to_string(),
            "bidi levels: inconsistent with the text"
        );
    }
}
