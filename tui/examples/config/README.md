# Config Examples

Real, verified configuration files (improvement plan T504, `tasks.md`) --
each one is checked by actually loading/parsing it with the real code that
reads it, not just hand-typed and hoped correct:

- **`config.toml`** -- every one of Vix's 63 settings, each with its real
  default value and its own doc comment (cross-checked against
  `docs/reference/settings.md`, T305's generated reference). Round-trips
  through `Settings::load_from` unchanged. See `crates/vix-settings/spec/
  index.md` for where the real file lives per platform.
- **`theme.json`** -- a small custom theme in the real bundled-theme
  format (`themes/*.json`), parseable as `vix_theme_model::CustomTheme`.
  Drop a copy in `~/.config/vix/themes/` to try it, or see
  `examples/theme_roundtrip.rs` for loading one from code.
  See `crates/vix-theme-editor-panel/spec/index.md`.
- **`snippets.json`** -- three snippets in the real VS-Code-style format
  `crates/vix-snippets/spec/index.md` documents (tabstops, a
  single-string or array `"prefix"`). Point `project_snippets` at a copy
  of this file (see `config.toml`) or drop it in the global snippets
  directory.
- **`macros.toml`** -- two hand-written macros in the real format
  `crates/vix-macros/spec/index.md` documents; see
  `examples/macro_replay.rs` for replaying one from code. Real macros are
  normally recorded with Edit -> Start Recording Macro rather than
  hand-written, but the token format is plain enough to write directly.

Sample Rhai scripts already exist at `../scripts/` (T105) -- nothing to
duplicate here.
