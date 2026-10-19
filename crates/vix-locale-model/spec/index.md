# Locale Model

Available UI languages for Vix; the View → Locale submenu lists them.

Pure data: each [`Locale`] pairs a code (used with `rust-i18n`) with its
endonym (the language's name in itself, the convention for language pickers).
The locale chooser lists these; the host applies a selection via
`rust_i18n::set_locale` and persists it. Extracted from the former
`vix-locale-chooser` so the data has its own home.

## Right-to-left

`is_rtl(code)` names right-to-left languages by primary subtag, and
`visual_order(chars)` returns the UAX #9 visual order of one line (identity when
the line has no strong right-to-left character). The App's `ui::bidi` pass applies
it to each drawn frame under the `bidi` setting (`auto` | `on` | `off`, T564).
Panels are not mirrored.

## See also

- [i18n spec](../../vix-i18n/spec/) — shared localization model
