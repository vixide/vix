# EditorConfig

Vix™ reads [EditorConfig](https://editorconfig.org/) files so a project can pin its
indentation and on-save normalization regardless of each contributor's global
settings. Support is on by default; set `editorconfig = false` in your config to
turn it off.

## What is applied

When you open a file, Vix walks up from its directory collecting `.editorconfig`
files (stopping at one whose preamble sets `root = true`) and applies the matching
properties:

| Property                   | Effect in Vix                                      |
| -------------------------- | -------------------------------------------------- |
| `indent_style`             | Tab inserts a tab (`tab`) or spaces (`space`)      |
| `indent_size` / `tab_width`| Number of spaces per indent (when style is `space`)|
| `trim_trailing_whitespace` | Strip trailing whitespace on save                  |
| `insert_final_newline`     | Ensure a final newline on save                     |

The indent is applied to the buffer when it opens; the trim / final-newline rules
are applied per file when it is saved, overriding the global
`trim_trailing_whitespace` / `ensure_final_newline` settings.

## Precedence

Properties from a `.editorconfig` **nearer** the file win over those farther up
the tree, and within one file a later matching `[section]` wins. Section globs
support the common forms (`*`, `**`, `?`, `[...]`, `{a,b}` and extension lists like
`*.{js,ts}`).

## Example

```ini
# .editorconfig at the project root
root = true

[*]
indent_style = space
indent_size = 4
trim_trailing_whitespace = true
insert_final_newline = true

[*.{js,ts}]
indent_size = 2

[Makefile]
indent_style = tab
```

See the specification at `spec/editorconfig/index.md`. EditorConfig and editor
[modelines](../modelines/index.md) cover similar ground; EditorConfig is
project-wide, modelines are per-file directives.

---

Vix™ and Vix IDE™ are trademarks.
