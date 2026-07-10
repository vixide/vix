# Breadcrumb Bar

The breadcrumb bar is an optional one-row strip above the editor that shows where
you are: the active file name, then the enclosing symbol at the cursor, as
`file ▸ symbol`.

## Using it

Toggle it from **View → Layout → Breadcrumbs** or the command palette (*Toggle
Breadcrumbs*). The preference persists in settings (`show_breadcrumbs`, off by
default), so it stays on across restarts once enabled.

As you move the cursor, the symbol segment updates to the nearest enclosing
declaration (function, struct, class, etc.), found from the same symbol scan that
powers the Code Outline. Files with no recognized symbols show just the file name.

See the specification at `spec/breadcrumbs/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
