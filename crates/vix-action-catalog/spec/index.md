# Action Catalog

**Status:** Shipped (T147). A single `(action id -> i18n title key)` catalog
for every `App::run_action` id that has no `vix-menu` item of its own.

## Problem

`App::action_title` (F1 help, the keybinding editor, the which-key popup)
only ever learned a human title for an action by walking `vix_menu::menus()`
for a leaf whose `action` field matched, and the command palette's `>`
Commands mode only ever listed `palette::COMMANDS`, a separate hand-curated
`(label_key, action)` list. Most actions have a menu leaf (and so a
`COMMANDS` entry, mostly copied from it), but plenty don't — bare
editor-core primitives (`cursor_down`, `select_word_left`, …),
`Ctrl X`-chord-only Emacs actions, per-keymap motion aliases, and a handful
of Org/roam/project/tools actions never exposed through either. Those
showed their raw id (`nav.back`, `view.toggle_menu`, …) wherever a title
was expected, and were entirely absent from the palette; `keybindings.reload`
once needed a Tools menu leaf added *purely* to make it findable — a
workaround this catalog removes the need for.

## Contents

- `Action { id, title }` — `id` is the action id exactly as
  `App::run_action` matches it; `title` is the i18n key for its human title
  (e.g. `"action.cursor_down"`).
- `CATALOG: &[Action]` — every action id titled *nowhere else*, paired with
  its title key. Deliberately **not** every action id in the app: one with
  a menu leaf is titled by that leaf, and one already in `palette::
  COMMANDS` is titled there — either is the more specific, hand-placed
  source, so neither gets a `CATALOG` entry (see the "no double titling"
  tests below).
- `title_key(action_id) -> Option<&'static str>` — the lookup `App::
  action_title` falls back to when no menu item runs `action_id`.

Pure data, like `vix-keyboard-shortcut-panel`: no dependencies, the host
(`src/app.rs`) owns rendering and translation (`t!`).

## How the host reads it

Two independent hosts, each trying their own primary source first:

- **`App::action_title`** (`src/app.rs` — backs F1 help and the keybinding
  editor) tries the menu tree (`vix_menu::menus()`, walked for a leaf whose
  `action` matches), then this catalog (`title_key`), then falls back to
  the raw action id unchanged (matching the which-key popup).
- **The command palette's `>` mode** (`App::catalog_palette_entries` in
  `src/app/command_palette.rs`) appends every `CATALOG` entry to
  `palette::COMMANDS`'s own list, so an action with no `COMMANDS` entry is
  now searchable there too.

`vix-keybindings`' `shortcuts_for` (an `action_id -> [(keymap, context,
key)]` reverse lookup, not yet used by any UI) can pair its results with
`title_key` the same way once something renders them.

## Keeping it correct

- **Coverage**: `tests/action_catalog.rs` (repo root) walks the real
  `App::run_action` dispatch chain by source (brace-balancing each
  dispatcher function out of its file) and asserts every id it can match is
  titled by a menu leaf, a `CATALOG` entry, or a `palette::COMMANDS` entry
  — nothing falls through to a raw id.
- **No double titling**: the same test file asserts no `CATALOG` entry's id
  *also* has a menu leaf, and a second test asserts none is *also* in
  `palette::COMMANDS` — either combination is unreachable (the more
  specific source always wins) and just as likely to drift silently, so
  both are rejected rather than allowed to sit unused.
- **i18n**: the same test asserts every `CATALOG` title key exists in
  `locales/app.yml`.

## Roadmap

- A menu item or keymap-table binding added for a new action already
  covers it (no catalog entry needed) unless it's genuinely menu-less —
  `tests/action_catalog.rs` fails the build either way it's missed.
- Retitle `vix-editor-core/spec/actions.tsv`'s bare action ids to source
  from here directly, rather than duplicating the id list, once that
  crate's codegen has a reason to depend on catalog data (not yet true —
  the two lists are cross-checked by hand today, not code).
