# vix-locale-model

The list of available UI languages. Pure data extracted from
`vix-locale-chooser` (which now depends on this crate for the data and keeps only
the chooser-overlay selection state).

## Data

`Locale` is `{ code, name }`:

- `code` — the locale code passed to `rust-i18n` (e.g. `en`, `es`, `zh`).
- `name` — the endonym, the language's own name (e.g. `Español`), the convention
  for language pickers.

`LOCALES` lists them in chooser order — English first (the fallback), then other
natural languages, then constructed languages. `by_code(code)` looks one up.

The host applies a chosen locale with `rust_i18n::set_locale` and persists it in
`settings.locale`. This crate is pure data with no dependencies.
