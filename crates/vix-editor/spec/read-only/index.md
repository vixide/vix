# Read-Only Buffer

Editor action `view.read_only`; per-tab `Tab::read_only`.

Lock the active buffer against edits: typing, deletes, and editing commands are blocked (with a status note), while navigation, selection, copy, and search still work. Toggling it off restores editing.

## Dependency and toolchain paths (T662)

A file whose canonical path matches a `read_only_paths` glob opens read-only, so Go to Definition into a dependency can't be edited by accident. The defaults cover `~/.cargo/registry/**`, `~/.cargo/git/**`, `~/.rustup/**`, `/nix/store/**`, Homebrew's `/opt/homebrew/Cellar/**` and `/usr/local/Cellar/**`, `**/node_modules/**` and `**/site-packages/**`. Such a tab sets `Tab::path_access` to `PathAccess::Protected` and shows a lock in its title while it stays read-only; the toggle below unlocks it, which also drops the lock. Patterns are compiled by `vix_fileops::PathPatterns`: `~/` is the home directory, `**` spans directories, `*` stays within one, and an invalid pattern is skipped.

## Toggle

From **View -> Editor -> Read-Only** or the command palette. Gated at `editor_key`, `insert_str`, `transform_selection_or_buffer`, and `run_edit_action` (default-deny over `edit.*` with a read-only-safe allowlist).

See `spec/index/index.md` for the project overview and `crates/vix-editor-core/spec/index.md` for the full action catalog.
