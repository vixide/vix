# Glossary

Shared terms used across the code, specs, and docs.

- **Action id** — a stable string (e.g. `file.save`, `view.theme`) that names a
  command. Menus, the palette, and shortcuts all dispatch one through
  `App::run_action`.
- **Buffer / Tab** — one open file (or untitled document) shown as one editor tab.
- **Preview tab** — an ephemeral tab opened by single-click or arrow-scan in the
  explorer; the next preview reuses it instead of accumulating tabs.
- **Dock** — a side drawer: the **left dock** is the file explorer, the **right
  dock** is the message drawer.
- **Region** — a themeable area of the UI: menu bar, status bar, left dock, right
  dock, editor. Custom themes color regions individually.
- **Mode (theme)** — a built-in monochrome theme: Dark or Light.
- **Custom theme** — a JSON file giving per-region RGB colors (plus optional
  cursor and syntax colors), layered over the built-in modes.
- **Choice (theme)** — one entry in the theme chooser: a built-in mode or a
  custom theme.
- **Locale** — a UI language, identified by a code (`en`, `es`, `fr`, `de`, `cy`)
  and shown by its endonym (its name in itself).
- **Endonym** — a language's name in that language (e.g. "Deutsch" for German).
- **i18n key** — a dotted name (e.g. `status.saved`) resolved to translated text
  by `t!` against `locales/app.yml`.
- **Fallback locale** — English (`en`); used for any key a language lacks.
- **Modal / overlay** — a UI layer that consumes all input while open (help,
  prompt, palette, search, choosers, …), handled in priority order in `on_key`.
- **Mark** — a non-selection highlight in the editor, on separate channels:
  **search marks** (other search hits / document-highlight occurrences,
  underline), **spell marks** (misspelled words in comments/strings, red
  underline; from the `spellcheck` module), **diagnostic marks** (LSP
  severity-colored underlines), and **gutter marks** (git diff bars in the
  line-number gutter; from `git::diff_marks` against the cached HEAD blob).
- **Fold** — a collapsed line range (from LSP `foldingRange`): the start line
  stays visible with a ▾/▸ gutter marker; inner lines are hidden by the renderer.
- **Inlay hint** — inline type/parameter annotation (LSP `inlayHint`) drawn
  dimmed between real glyphs, shifting following glyphs right.
- **Soft wrap** — a long logical line drawn across several screen rows instead of
  scrolling horizontally (**View → Toggle Soft Wrap**, the `soft_wrap` setting).
- **Visual row** — in soft-wrap mode, one screen row: a `[start, end)` char slice
  of a logical line. The shared layout (`Editor::visual_rows`) drives the wrapped
  renderer, cursor scroll, and mouse hit-testing.
- **Bracket match** — the partner of the bracket at (or just before) the cursor,
  highlighted by the editor (no pair auto-insertion).
- **Monochrome** — the built-in theme style: one foreground, one background, no
  hue; emphasis via dim and full intensity (no bold/italic); reversed only for
  selections/cursor.
- **Keymap** — the keyboard navigation style: **Apple** (modifier shortcuts, the
  default), **Emacs** (`Ctrl` chords), or **Vim** (modal). Exactly one is active;
  chosen in **View → Keymap…** and persisted in the `keymap` setting.
- **Mode (Vim)** — within the Vim keymap, **Normal** (motions/commands) vs
  **Insert** (typing); the `:` command line is a third input state.
