# vix-keymap-model

The available keyboard navigation styles ("keymaps"). Pure data extracted from
the former `vix-keymap-chooser`; the chooser overlay was replaced by the static
**View → Keymap** submenu, which is built from this list.

## Data

`Keymap` is `{ id, name, tooltip }`:

- `id` — stable identifier persisted in `settings.keymap` (`apple`, `vscode`,
  `emacs`, `vim`).
- `name` — proper-noun title shown in the menu (not translated).
- `tooltip` — a short description.

`KEYMAPS` lists them in menu order (Apple first — Vix's default). `by_id(id)`
looks one up.

## Behavior

Selecting a keymap from the submenu dispatches the action `view.keymap:<id>`; the
host sets `settings.keymap`, resets per-keymap session state, and applies the
bindings. This crate is pure data with no dependencies.
