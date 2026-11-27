# EditorConfig

Minimal EditorConfig and modeline support: resolve the indent settings for a
file. Vix applies [EditorConfig](https://editorconfig.org/) rules per opened
file, overriding the global indent and on-save settings, controlled by the
`editorconfig` setting (default `true`); and then the file's own Vim
modelines and Emacs file variables (see "Modelines" below), controlled by
`modeline_lines` (default `5`).

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
tested (glob matching, precedence, indent string). Property tests
(`crates/vix-editorconfig/tests/props.rs`, T831): `resolve` over a generated `.editorconfig` (sections
over a fixed set of globs, recognized, junk and upper-case keys, comments, a
preamble) agrees with applying the matching sections' keys in file order, and
`modeline::parse` never panics, never returns a width out of bounds, sets
nothing with `lines == 0`, and reads back generated Vim `set` modelines and
Emacs `-*-` lines exactly.

## Modelines

`editorconfig::modeline::parse(head, tail, lines) -> Modeline` reads the
per-file settings a file states in itself. The host passes the buffer's first
`max(lines, 2)` lines as `head` and its last `max(TAIL_LINES, lines)` lines
(`TAIL_LINES` = 64) as `tail`, read line by line, so a large buffer costs no
more than a small one.

- **Vim modelines**, in the first and last `lines` lines: `vim:`, `Vim:`,
  `vi:` or `ex:` at the start of the line or after a blank, followed by
  either `set opt opt …:` (ending at the first `:`) or options separated by
  blanks or colons to the end of the line.
- **Emacs file variables**: a `-*- var: value; … -*-` line, which must be
  the first line, or the second after a `#!` line (a bare `-*- mode -*-`
  sets nothing); and a `Local Variables:` … `End:` block in `tail`, whose
  lines carry the opener's prefix and suffix (stripped).

Recognized, and nothing else:

| Setting | Vim | Emacs |
| --- | --- | --- |
| Indent with a tab, or spaces | `noet`/`noexpandtab`, `et`/`expandtab` | `indent-tabs-mode: t`/`nil` |
| Indent size | `sw`/`shiftwidth=N` | `*-basic-offset`, `*-indent-offset`, `*-indent-level: N` |
| Tab width | `ts`/`tabstop=N` | `tab-width: N` |
| Hard-wrap column | `tw`/`textwidth=N` | `fill-column: N` |
| Soft wrap | `wrap`, `nowrap` | `truncate-lines: nil`/`t` |
| Final newline on save | `eol`/`endofline`/`fixeol`, and their `no` forms | `require-final-newline: t`/`nil` |

Widths must be 1–16 and the wrap column 1–1000; anything else, and every
other option or variable, is ignored. Vim's other options (and Emacs's
`eval:` and `mode:`) are never applied, so a file cannot make Vix run
anything or change more than that buffer's layout. `ft`/`filetype` and
`mode` are not applied either: a buffer's language comes from its file name.

**Precedence.** Emacs settings win over Vim ones, and the `-*-` line over a
`Local Variables:` block. Vim modelines apply in file order, as in Vim, so a
later one wins. `lines = 0` turns all of it off.

**In the App.** On open, after the `.editorconfig` indent,
`App::apply_editorconfig_indent` applies the modeline's indent string
(`Modeline::indent_string`: a tab; or the indent size, else the tab width,
else 4, in spaces; a size with no style means spaces) and soft wrap.
`App::save_options` lets `insert_final_newline` override the setting and the
`.editorconfig`, and **Wrap Text** uses the modeline's wrap column. A tab
width alone does not change how Vix draws tabs (that is not per buffer); it
only sizes a spaces indent.

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). Modeline and comment parsing use `split_once`, `split_at_checked`, `strip_prefix` and iterator `skip` instead of slicing. **Declared:** a file shorter than the modeline window scans all of its lines (the window start is 0).
