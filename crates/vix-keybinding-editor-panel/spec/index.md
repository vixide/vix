# Keybinding Editor

**Status:** In progress (improvement plan T204). Data and table state for
the keybinding editor overlay (rebind/reset): **Vix → Keybindings…**, a
sibling of the read-only `vix-keyboard-shortcut-panel` (`F1`) that adds
rebind and reset. The host (`src/app.rs`/`src/ui.rs`) builds the rows and
wires them to `vix-keybindings::user_bindings`; this crate holds no
knowledge of either.

## Contents

Rows cover the **active keymap's top-level bindings** plus the
keymap-agnostic shared bindings (`vix_keybindings::SHARED`), plus any
token a script has bound at runtime. A chorded keymap's chord-continuation
bindings (Emacs's `C-x`/`C-c` families, Spacemacs's leader) are **not**
listed: `vix-keybindings`' override/resolution layer only ever resolves
the top-level (`""`) context, so a chord binding can't actually be
rebound or reset through `keybindings.toml` — listing it here would
promise something the system underneath can't do.

Each row shows:

- **Action** — the translated action title;
- **Shortcut** — the key combo, shown verbatim (e.g. `Ctrl P`);
- **Source** — built-in, a user override, or the script that bound it,
  shown as a marker so an overridden/shadowed row stands out from the F1
  panel's plain list.

## Interaction

- **Type** to filter rows live, case-insensitively, against both columns
  (mirrors `vix-keyboard-shortcut-panel`).
- **Click a column header** to sort that column ascending, click again for
  descending; the other header resets to ascending. Natural order until a
  header is first clicked.
- **↑/↓**, **PgUp/PgDn**, and the mouse wheel move the selection — unlike
  the read-only F1 panel, this table tracks a selected row (`Panel::selected`),
  since rebind and reset act on it.
- **Enter** (or a "Rebind" affordance) on the selected row opens a prompt:
  type the new key as text (`vix-macros` grammar, e.g. `C-S-k`), Enter
  to confirm. Validated, then written to `keybindings.toml` via
  `vix_keybindings::user_bindings::upsert` and the live override table is
  re-resolved.
- **Reset to default**, enabled only when the selected row's source is a
  user override (`Row::resettable`), removes the entry from
  `keybindings.toml` via `vix_keybindings::user_bindings::remove` and
  re-resolves.
- **Esc** closes the overlay.

## Module (`vix_keybinding_editor_panel`)

- `Source` — `BuiltIn | User | Script(String)`; where a row's effective
  binding currently comes from.
- `Row` — `key_token` (raw `vix-macros` token), `key_display`
  (rendered), `action_id`, `action_title`, `source`.
  `Row::resettable()` is `true` only for `Source::User`.
- `Column` — `Action | Keys`.
- `Panel` — `rows`, `query`, `sort` (`None` = natural order, else column +
  ascending flag), `scroll`, `selected`. `open(rows)`, `matches()`
  (filter + sort, mirrors the F1 panel), `len()`/`is_empty()`,
  `selected_row()`, `toggle_sort(col)`, `push(c)`/`backspace()` (both
  reset `selected`/`scroll`), `select_up(n)`/`select_down(n)`
  (selection-based, not raw scroll), `clamp_scroll(view_h)` — unlike the
  F1 panel's independent pure-scroll, this one follows `selected` so the
  highlighted row stays on screen.

## Roadmap

- **Press the actual key** as an alternative to typing the token text,
  once there's a capture-mode precedent elsewhere in Vix to follow (T204
  deliberately chose text entry first — see `tasks.md`).
- Surfacing chorded (non-top-level) bindings read-only, once there's a
  reason to show them here rather than only in the F1 panel.
- Reverse lookup ("press a combo, see what it's bound to") — shared
  roadmap item with `vix-keyboard-shortcut-panel`.
