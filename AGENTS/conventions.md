# Coding conventions

Patterns specific to this codebase. General Rust style (rustfmt defaults) applies
on top.

## Documentation

- `#![deny(missing_docs)]` is on at the crate root. Document all public items
  with `///`; module headers use `//!`.
- Doc comments say *what and why*, briefly. Implementation details that would
  surprise a reader get an inline `//` comment explaining the *why*.

## Lints

- `#![forbid(unsafe_code)]` and `#![warn(clippy::pedantic)]` at the crate root,
  and `#![warn(clippy::pedantic)]` is repeated in **every module file** (lints are
  per-module). Keep `cargo clippy --workspace --all-targets -- -D warnings` clean.
- **No blanket allows.** There is no `#![allow(clippy::pedantic)]` and no
  `#![allow(missing_docs)]`; fix findings in code (saturating `try_from` casts,
  extract helpers for `too_many_lines`, context structs for `too_many_arguments`,
  add `# Errors`/`# Panics`, etc.). The reused `editor_core` engine modules keep
  `#[allow(clippy::all, clippy::pedantic)]` for upstream style; new editor code
  goes in a Vix-owned module (`wrap`, `brackets`, `lines`), held to pedantic.
- The only sanctioned exceptions are four targeted
  `#[allow(clippy::struct_excessive_bools)]` on `App`/`Settings`/`SearchBar`/`WorkspaceSearch`.

## Internationalization

- User-facing strings are i18n keys looked up with `t!`, never literals.
- Keys are dotted and namespaced (`menu.*`, `cmd.*`, `ui.*`, `status.*`,
  `msg.*`, `help.*`, `prompt.*`, `theme.*`, `palette.*`).
- Interpolation uses `%{name}` in YAML and `t!("k", name = value)` in code.
- `t!` returns `Cow<str>`; call `.to_string()` when a `String` is required.
- Data modules (menu/palette/theme/keyboard) hold keys; the host calls `t!`.

## Actions

- Commands are string ids dispatched by `App::run_action`. Menu (`src/menu.rs`)
  and palette (`src/palette.rs`) reference the same ids.
- To add a command: add the `run_action` arm, then reference it from a menu item
  and/or `palette::COMMANDS`, and add its i18n label key.

## Rendering

- All of the *app's* drawing is in `src/ui.rs`; no editing/state logic there. The
  editor widget renders itself (in `editor_core`); the app just hands it a `Rect`.
- Paint the whole frame in the theme background first, then panes, then overlays.
- Overlays `Clear` their rect and set the block `.style(theme::base())` so they
  read correctly in light mode.
- Use the region-aware styles (`theme::region_base(Region::…)`) for the menu bar,
  status bar, docks, and editor so custom themes can color them.

## Theme

- Built-ins are monochrome; get colors from `theme::fg/bg/base/title/dim/selected`
  (mode-aware) or `theme::region_*` (custom-theme-aware).
- Reversed video is reserved for selections and the block cursor.

## State and modals

- `App` holds all state. Overlays are `Option<…>` fields; an open overlay is a
  modal handled near the top of `App::on_key` (strict priority order).
- The chooser/model modules expose `open()/up()/down()/selected_*()`; the app
  wires keys and applies the result.

## Errors and panics

- No `unsafe`. Avoid `unwrap`/`expect` on fallible runtime paths; prefer
  reporting to the message drawer (`self.messages.error(…)`) or status line.
- `expect` is acceptable for genuinely-infallible invariants, with a message.

## Tests

- Prefer terminal-independent tests: build an `App`, feed `KeyEvent`s, assert on
  state. Render checks use a sized `TestBackend`.
- Avoid asserting on translated text where a process-global locale could race;
  assert on state or i18n keys instead.
