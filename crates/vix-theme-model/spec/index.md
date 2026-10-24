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
