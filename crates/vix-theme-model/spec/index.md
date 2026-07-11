# Theme Model

The Vix theme model.

Every theme is a JSON [`CustomTheme`] with per-region colors and font
attributes (see `spec/index.md`). There is always exactly one active theme;
the bundled `Dark` and `Light` themes (from `themes/dark.json` /
`themes/light.json`) are just ordinary themes the host ships. The style
helpers ([`fg`], [`bg`], [`region_base`], …) read the active theme.
[`theme_names`] produces the de-duplicated, sorted list for the View → Theme
submenu.

Theme names are plain strings (also the value persisted in settings), so this
crate stays free of any localization dependency.

## See also

- [theme spec](../../vix-theme/spec/) — shared theme model
