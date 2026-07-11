# EditorConfig

Vix applies [EditorConfig](https://editorconfig.org/) rules per opened file,
overriding the global indent and on-save settings. Controlled by the
`editorconfig` setting (default `true`).

## Behavior

- On open, the host resolves the file's `.editorconfig` chain (directory upward,
  stopping at `root = true`) and, if it specifies an indent, applies it to the
  buffer (`App::apply_editorconfig_indent` → `Editor::set_indent`).
- On save, `App::save_options` consults the active file's `.editorconfig` and lets
  `trim_trailing_whitespace` / `insert_final_newline` override the global
  `trim_trailing_whitespace` / `ensure_final_newline`.

## Resolution

- Recognized properties: `indent_style` (`tab`/`space`), `indent_size`,
  `tab_width`, `trim_trailing_whitespace`, `insert_final_newline`.
- Properties from a nearer `.editorconfig` win; within a file, a later matching
  section wins.
- Glob support (in `editorconfig::glob_to_regex`): `*`, `**`, `?`, `[...]`/`[!...]`,
  and `{a,b}` brace alternation (including extension lists `*.{js,ts}`). A pattern
  without `/` matches the file name in any directory. `{num1..num2}` ranges are
  not supported.

## As implemented in Vix

The `editorconfig` module is a dependency-free reader (reusing the crate's
`regex`). `editorconfig::resolve(path) -> Resolved` walks the chain and merges
properties; `Resolved::indent_string()` yields the Tab string. Pure data, unit
tested (glob matching, precedence, indent string).
