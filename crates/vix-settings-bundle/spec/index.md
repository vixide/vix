# Settings bundle

Bundle a user's config.toml, macros.toml, keybindings.toml, user
dictionary, and active custom theme into one file, and restore them from
it. Module `settings_bundle`.

- menu "Vix"
  - menuitem "Export Settings…"
  - menuitem "Import Settings…"
- `vix --export-settings <PATH>` / `vix --import-settings <PATH>` (CLI
  flags, `src/cli.rs`)

## What's bundled

Every piece of the user's config-directory setup that isn't a live
cache, each only if it currently exists on disk:

- `Settings::config_path()` — `config.toml`.
- `Settings::macros_path()` — `macros.toml` (persisted keyboard macros).
- `Settings::keybindings_path()` — `keybindings.toml` (keybinding
  overrides).
- `Settings::user_dictionary_path()` — `user_dictionary.txt` (spellcheck
  personal word list).
- The **active custom theme**: the file in `Settings::themes_dir()` whose
  `name` (case-insensitive) matches `Settings::theme` — not present at
  all when the active theme is one of the themes built into the binary,
  since the importing machine already ships those.

Custom scripts (`Settings::scripts_dir()`) are deliberately **not**
included — a `.rhai` script can do anything a normal user script can
(read/write files, run commands), so silently importing and running
someone else's scripts on import would be a real risk a settings bundle
should not carry by default. Exporting/sharing scripts remains a manual
file-copy exercise.

## Bundle format

A single pretty-printed JSON file: `{"format": 1, "entries": {"<name>":
"<content>", ...}}`. Plain JSON, not a real archive format (`.tar`/`.zip`)
— every bundled file is already plain text, so no new archive-format
dependency earns its keep; JSON's string escaping already handles
arbitrary text content safely, and the result stays diffable/readable by
a person if they open it directly. `<name>` is one of `config.toml`,
`macros.toml`, `keybindings.toml`, `user_dictionary.txt`, or
`theme/<original filename>.json` for the active custom theme (the
original filename is preserved so re-importing writes back to the same
name).

## Import conflict handling

Importing backs up any file it's about to overwrite to `<path>.bak`
first (not timestamped — a second import in a row just replaces the
previous `.bak`, trading unbounded backup accumulation for "the state
right before the last import is always one rename away"), then writes
the new content. This is the same policy for both the CLI flag and the
menu action — deliberately not interactive (a "prompt per file" flow
would need the App/terminal to exist at all, which the CLI path runs
before), and deliberately not silent overwrite-with-no-recovery either.

## Design notes

A new crate rather than a method on [`vix::app::App`], same reasoning as
`vix-doctor` (T553): the CLI path (`vix --export-settings`/
`--import-settings`) runs before any `App` exists, so the logic needs to
work from a bare `Settings` value and real filesystem paths only.
Depends on `vix-settings` (for the `Settings::*_path()`/`themes_dir()`
accessors) and `vix-theme-model` (to parse each themes-directory file
and match it by `name` against `Settings::theme`).
