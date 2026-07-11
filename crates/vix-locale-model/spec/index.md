# Locale Model

Available UI languages for Vix.

Pure data: each [`Locale`] pairs a code (used with `rust-i18n`) with its
endonym (the language's name in itself, the convention for language pickers).
The locale chooser lists these; the host applies a selection via
`rust_i18n::set_locale` and persists it. Extracted from the former
`vix-locale-chooser` so the data has its own home.

## See also

- [i18n spec](../../vix-i18n/spec/) — shared localization model
