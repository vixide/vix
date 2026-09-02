# Coding conventions

Patterns specific to this codebase. General Rust style (rustfmt defaults) applies
on top.

## Documentation

- `#![deny(missing_docs)]` is on at every crate root. Document all public items
  with `///`; module headers use `//!`.
- Doc comments say *what and why*, briefly. Implementation details that would
  surprise a reader get an inline `//` comment explaining the *why*.

## Lints

- `#![forbid(unsafe_code)]` and `#![warn(clippy::pedantic)]` at every crate root,
  and `#![warn(clippy::pedantic)]` is repeated in **every module file** (lints are
  per-module). Keep `cargo clippy --workspace --all-targets -- -D warnings` clean.
- **No blanket allows.** There is no `#![allow(clippy::pedantic)]` and no
  `#![allow(missing_docs)]`; fix findings in code (saturating `try_from` casts,
  extract helpers for `too_many_lines`, context structs for `too_many_arguments`,
  add `# Errors`/`# Panics`, etc.). The reused `editor_core` engine modules keep
  `#[allow(clippy::all, clippy::pedantic)]` for upstream style; new editor code
  goes in a Vix-owned module (`wrap`, `brackets`, `lines`), held to pedantic.
- Sanctioned exceptions are only a few **targeted** allows:
  `#[allow(clippy::struct_excessive_bools)]` on `App`/`Settings`/`SearchBar`/
  `WorkspaceSearch`/`editor_core::Editor`, and a handful of `too_many_lines` /
  `too_many_arguments` on specific functions that resist further extraction.

## Internationalization

- User-facing strings are i18n keys looked up with `t!`, never literals.
- Keys are dotted and namespaced (`menu.*`, `cmd.*`, `ui.*`, `status.*`,
  `msg.*`, `help.*`, `prompt.*`, `theme.*`, `palette.*`).
- Interpolation uses `%{name}` in YAML and `t!("k", name = value)` in code.
- `t!` returns `Cow<str>`; call `.to_string()` when a `String` is required.
- Data modules (menu/palette/theme/keyboard) hold keys; the host calls `t!`.

## Actions

- Commands are string ids dispatched by `App::run_action` (in the App shell,
  `src/app.rs`). The menu (`crates/vix-menu/`) and palette (`crates/vix-palette/`,
  re-exported as `crate::menu` / `crate::palette`) reference the same ids.
- To add a command: add the `run_action` arm, then reference it from a menu item
  and/or `palette::COMMANDS`, and add its i18n label key.

## Pure-logic modules

- Prefer a small **pure module** (`text -> text` or `(text, cursor) -> …`) with
  unit tests for any non-trivial transform, and keep the `App` method a thin
  wrapper that reads the buffer, calls the pure fn, and writes back. Examples:
  `align`, `emmet`, `tags`, `http_client::parse_request`, `org_table`, and
  `textops` (whole-text transforms plus the cursor-relative rewrites
  `bump_number_at` / `smart_toggle_at` / `transpose_*_at` / `delete_*_at`).
  A family of related transforms shares its unit builders rather than each
  re-deriving ranges — see `textops`'s `word_units` / `sentence_units` /
  `paragraph_units` / `section_units`, used by both transpose and delete.
- Buffer-mutating actions funnel through `App::transform_selection_or_buffer`
  (selection-or-whole-buffer), `App::rewrite_at_cursor` (cursor-relative), or
  `insert_str`; all are read-only-aware.

## Rendering

- All of the *app's* drawing is in `src/ui.rs`; no editing/state logic there. The
  editor widget renders itself (in `editor_core`); the app just hands it a `Rect`.
- Paint the whole frame in the theme background first, then panes, then overlays.
- Overlays `Clear` their rect and set the block `.style(theme::base())` so they
  read correctly in light mode.
- Use the region-aware styles (`theme::region_base(Region::…)`) for the menu bar,
  status bar, docks, and editor so custom themes can color them.

## Terminal input

- The terminal is owned by `src/main.rs` alone: raw mode, mouse capture,
  bracketed paste, and (on macOS) the kitty keyboard flags that make the
  `Command` modifier reportable. Every mode it turns on it turns off again, on
  exit **and** around suspend.
- Events reach exactly three entry points — `App::on_key`, `App::on_mouse`,
  `App::on_paste`. Anything that needs a fourth is a bug in the routing, not a
  new entry point.
- Normalize at the entry point, not per binding: the macOS `Command` → `Control`
  fold happens once at the top of `on_key`, so every keymap sees a chord it
  already knows.
- A pasted chunk is one edit, not a replay of keystrokes: replaying re-triggers
  auto-indent and auto-pairing and leaves one undo entry per character.

## Theme

- Built-ins are monochrome; get colors from `theme::fg/bg/base/title/dim/selected`
  (mode-aware) or `theme::region_*` (custom-theme-aware).
- Reversed video is reserved for selections and the block cursor.

## State and modals

- `App` holds all state. Overlays are `Option<…>` fields; an open overlay is a
  modal handled near the top of `App::on_key` (strict priority order).
- `App::overlay_capturing_keys` mirrors that dispatch chain for callers that must
  ask "would a keypress reach the editor?" without dispatching one (the paste
  router does). Add an overlay to the chain and to that predicate together.
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
- **Tests must not touch the machine.** The platform clipboard is opt-in
  (`vix_clipboard::use_system`, called only by `main`), so a test run cannot
  overwrite what the developer copied; hold anything else that reaches outside
  the process to the same standard, and write to a temp directory keyed by
  `std::process::id()`.
- A platform-specific behavior gets a test that asserts the *platform-appropriate*
  outcome (`if cfg!(target_os = "macos") { … } else { … }`), so it still runs —
  and still means something — on the CI that is not that platform.
- Pure parsers and text transforms get a fuzz target under `fuzz/fuzz_targets/`;
  anything on the per-keystroke or per-frame path gets a Criterion benchmark
  under `benches/`. Run one with `cargo +nightly fuzz run <target>` (needs
  `cargo install cargo-fuzz`; add `-- -max_total_time=600` for a fixed-length
  run rather than fuzzing until Ctrl+C) — it's a separate workspace
  (`fuzz/Cargo.toml`) so `cargo build`/`cargo check --workspace` at the repo
  root never touch it. See [`fuzz/README.md`](../fuzz/README.md) (target list,
  reproducing/minimizing a crash) and [`spec/test/index.md`](../spec/test/index.md).
- When the *layout itself* is what's under test (a dialog's framing, a dock's
  column widths, a long line truncating), add a golden-screen test to
  `tests/snapshots.rs` instead of a state assertion — pin the locale to `en`
  first, since `rust_i18n::locale()` is process-global. Reviewing a changed
  snapshot: `INSTA_UPDATE=always cargo test --test snapshots`, then read every
  line of `git diff tests/snapshots/` (or `cargo insta review` with
  `cargo-insta` installed) before committing — a snapshot diff is a signal to
  explain, not a formality to wave through. See
  [`spec/test/index.md`](../spec/test/index.md)'s "Snapshot testing" section.
