# Theme Model

The Vix theme model: the JSON theme model, ratatui styles, and theme loading.

Every theme is a JSON [`CustomTheme`] with per-region colors and font
attributes (see `crates/vix-theme/spec/index.md`). There is always exactly one active theme;
the bundled `Dark` and `Light` themes (from `tui/themes/dark.json` /
`tui/themes/light.json`) are just ordinary themes the host ships. The style
helpers ([`fg`], [`bg`], [`region_base`], …) read the active theme.
[`theme_names`] produces the de-duplicated, sorted list for the View → Theme
submenu.

Theme names are plain strings (also the value persisted in settings), so this
crate stays free of any localization dependency.

## Following the terminal (`theme = "auto"`, T579)

`background` is the pure half of `theme = "auto"`: `parse_osc11_reply` (an OSC 11
`rgb:` reply, 1–4 hex digits per component), `from_colorfgbg`, `classify` (Rec. 709
luminance), `set_detected`/`detected` (the session's one answer) and `resolve_theme`
(`"auto"` → `theme_light` or `theme_dark`; dark when nothing was detected). The
terminal I/O that asks lives in the binary.

## See also

- [theme spec](../../vix-theme/spec/) — shared theme model

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). `to_json` returns `Result<String, serde_json::Error>` instead of an empty string when serialization fails, so a theme is never saved as an empty file; the theme editor reports the error. The OSC 11 reply parser splits on `rgb:` with `split_once` and scales each component with `checked_*` (a reply that does not fit is `None`). The color-distance and grey-ramp arithmetic are plain under `RCA-EXC-2` expectations with their bounds.
