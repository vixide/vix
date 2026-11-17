# Modelines

A modeline is a comment in a file that sets how that file is edited: its
indentation, its wrap column, whether it ends with a newline. Vix™ reads Vim
modelines and Emacs file variables, so files that already carry them for
Vim or Emacs users work the same way in Vix.

```python
# vim: set et sw=4 tw=79:
```

```c
/* -*- indent-tabs-mode: t; tab-width: 8; c-basic-offset: 8 -*- */
```

A modeline wins over the file's [`.editorconfig`](../editorconfig/index.md),
which wins over your [settings](../configuration/index.md).

## What Vix reads

| Setting | Vim | Emacs |
| --- | --- | --- |
| Indent with a tab, or with spaces | `noet` / `noexpandtab`, `et` / `expandtab` | `indent-tabs-mode: t` / `nil` |
| Indent size | `sw=N` / `shiftwidth=N` | `c-basic-offset: N`, `python-indent-offset: N`, `js-indent-level: N`, … |
| Tab width | `ts=N` / `tabstop=N` | `tab-width: N` |
| Wrap column for **Wrap Text** | `tw=N` / `textwidth=N` | `fill-column: N` |
| Soft wrap | `wrap`, `nowrap` | `truncate-lines: nil` / `t` |
| End the file with a newline on save | `eol`, `fixeol`, and `noeol`, `nofixeol` | `require-final-newline: t` / `nil` |

Everything else in a modeline is ignored. That is deliberate: a file can set
its own layout, but it cannot make Vix run anything or change any other
setting. A file type (`ft=`, `mode:`) is ignored too; Vix picks the language
from the file name.

## Where Vix looks

- **Vim** modelines in the first and last five lines of the file: `vim:`,
  `vi:` or `ex:` at the start of a line or after a space. Both forms work:
  `vim: set ts=4 sw=4:` and `vim: ts=4 sw=4`.
- An **Emacs** `-*- … -*-` line, as the first line of the file, or the second
  after a `#!` line.
- An **Emacs** `Local Variables:` block near the end of the file:

  ```text
  # Local Variables:
  # fill-column: 72
  # End:
  ```

When a file has both, the Emacs settings win. Among Vim modelines, as in
Vim, a later one wins.

## Turning it off

The `modeline_lines` setting is how many lines at each end of a file Vix
searches. `0` turns modelines off:

```toml
modeline_lines = 0
```

---

Vix™ and Vix IDE™ are trademarks.
