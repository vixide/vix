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

## Trust boundary: a bundle file is untrusted input (T558)

A bundle is just a JSON file — nothing stops someone from crafting one by
hand and sharing it (as a theme, a "handy config," …) for someone else to
import. Two protections specifically because of that, both found the
hard way (a real self-audit, not designed in up front) and both verified
against a real crafted malicious bundle, not just reasoned about:

- **Theme entry names are restricted to a bare filename.** A `theme/…`
  entry's name, after the prefix, must contain no path separator (`/` or
  `\`, both rejected on every platform regardless of which one built the
  bundle) and must not be `.`/`..`. Without this, a crafted entry named
  `theme/../../../../.ssh/authorized_keys` would resolve outside
  `Settings::themes_dir()` entirely on import — a real arbitrary-file-
  write, not a theoretical one.
- **`config.toml`'s command-bearing fields are never silently imported
  if they differ.** `ai_command`, `ai_api_key_command`, `test_command`,
  and every `lsp_servers[].command` are free-text shell command lines/
  argv the running editor executes on an ordinary action (an AI query,
  "Run Tests", opening a matching file for LSP) — exactly the class of
  risk this spec already reasoned through for `.rhai` scripts above, and
  `config.toml`'s own command fields deserve the identical treatment,
  not a silent overwrite on every import. On import, if any of these
  differ from the *current* settings, they're reset back to the current
  values before the rest of `config.toml` is applied, and the caller is
  told so (the CLI prints a note; the menu action shows a message-drawer
  warning) — everything else in `config.toml` (theme name, locale, UI
  toggles, …) still imports normally either way.

## Design notes

A new crate rather than a method on [`vix::app::App`], same reasoning as
`vix-doctor` (T553): the CLI path (`vix --export-settings`/
`--import-settings`) runs before any `App` exists, so the logic needs to
work from a bare `Settings` value and real filesystem paths only.
Depends on `vix-settings` (for the `Settings::*_path()`/`themes_dir()`
accessors) and `vix-theme-model` (to parse each themes-directory file
and match it by `name` against `Settings::theme`).
