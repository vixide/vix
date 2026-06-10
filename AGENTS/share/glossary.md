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
- **Mark** — a non-selection highlight in the editor (e.g. other search hits),
  rendered as an underline in monochrome.
- **Monochrome** — the built-in theme style: one foreground, one background, no
  hue; emphasis via dim and full intensity (no bold/italic); reversed only for
  selections/cursor.
- **Keyway** — the keyboard navigation style: **Apple** (modifier shortcuts, the
  default), **Emacs** (`Ctrl` chords), or **Vim** (modal). Exactly one is active;
  chosen in **View → Keyway…** and persisted in the `keyway` setting.
- **Mode (Vim)** — within the Vim keyway, **Normal** (motions/commands) vs
  **Insert** (typing); the `:` command line is a third input state.
