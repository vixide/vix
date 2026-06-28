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
- **Locale** — a UI language, identified by a code (`en`, `es`, `fr`, `de`, `cy`,
  `ga`, `gd`, `pl`, `pt`, `ru`, `ar`, `hi`, `bn`, `zh`, `ja`) and shown by its
  endonym (its name in itself).
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
  default), **macOS VSCode**, **Emacs** (`Ctrl` chords), **Vi** (modal),
  **Spacemacs** (Vi + `Space` leader), **IntelliJ + macOS**, **IntelliJ +
  Windows**, or **Eclipse**. Exactly one is active; chosen in **View → Keymap…**
  and persisted in the `keymap` setting (id `vi`, `intellij-mac`, …).
- **Mode (Vi)** — within the Vi keymap, **Normal** (motions/commands) vs
  **Insert** (typing); the `:` command line is a third input state.
- **Edit surface** — a full-screen overlay editor for a non-plain-text view of the
  active buffer, each a module owning its state and a `handle_key` returning an
  `Outcome`: **Edit Table** (`edit_table`, CSV/TSV grid), **Edit Outline**
  (`edit_outline`, indented prose hierarchy with folding), **Edit JSON / Edit
  YAML** (`edit_value`, a foldable structured-value tree), **Edit Bytes**
  (`edit_bytes`, a hex/ASCII byte editor), **Edit SQL** (`edit_sql`, a SQL
  statement list). All under **Edit → Mode**.
- **Insert (Tools menu)** — the submenu (formerly "Generate") whose items insert
  generated content at the cursor: UUID, ZID, Markdown/HTML/SQL/LaTeX/Org
  snippets, inline Org markers and blocks, Lorem ipsum, and Date/Time presets.
  Actions are `tools.insert.*`.
- **Media type** — a MIME-style content type (`text/rust`, `image/png`) from the
  `media_type` catalog (`spec/media-types`). Each is classified **text** or
  **binary** (the `Base` column) and maps to file extension(s).
- **Snippet** — a reusable template with tabstops, defined in JSON files
  (bundled, global, per-media-type, project scopes; `crate::snippets`) and
  inserted via the picker or prefix-and-Tab expansion. Tabstops are parsed by
  `snippet_tool::parse`.
- **Org (menu)** — basic Org-mode editing on the active buffer (`crate::org`):
  headline promote/demote, subtree move, TODO cycle, checkbox toggle, fold cycle,
  and export to Markdown/HTML.
- **DAP / debugger** — the Debug menu's Debug Adapter Protocol client
  (`crate::dap`): breakpoints, stepping, call stack, variables, watches, REPL.
- **Column selection** — a rectangular multi-caret selection spanning the same
  columns across consecutive lines (Alt+Shift+↑/↓, `Editor::column_select`); the
  block edits together via the multi-caret path.
- **Zen mode** — a focus mode that hides the docks and status bar
  (`view.zen`); **breadcrumbs** — an optional `file ▸ symbol` bar above the editor
  (`view.breadcrumbs`).
