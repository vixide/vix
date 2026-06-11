# AGENTS.md

Guidance for AI agents and human contributors working in the Vix repository.
This file is the entry point; see [`AGENTS/`](AGENTS/) for topic guides and
[`index.md`](index.md) for the full documentation map.

## What Vix is

Vix is a keyboard-friendly terminal text editor (a "Simple Terminal Rust IDE"),
built on `ratatui`. It is a Cargo workspace: a main `vix` application crate plus
several small internal crates. See [`docs/architecture.md`](docs/architecture.md).

## Source of truth

`spec/*.md` is the **specification and the source of truth**. Development is
spec-driven: when behavior and spec disagree, decide which is correct, then make
them match — update the spec when intent changes, update the code when the code
drifted. Keep specs and implementation in sync.

## Build, test, lint

```sh
cargo build                 # build the vix binary + library
cargo test                  # vix integration + doc tests (no terminal needed)
cargo test --workspace      # also every internal crate's unit tests
cargo clippy --workspace    # lints; the tree is kept warning-clean
cargo run                   # run the editor in the current directory
cargo run -- --locale fr    # run in a specific language
```

The toolchain floor is Rust 1.86 (the editor crate uses edition 2024).

## Hard rules enforced by the build

The `vix` crate sets `#![deny(missing_docs)]` and `#![forbid(unsafe_code)]`
(see `src/lib.rs`). Therefore:

- **Every public item needs a doc comment.** A new `pub fn`/`struct`/`field`
  without `///` fails the build.
- **No `unsafe`.**
- **`#![warn(clippy::pedantic)]`** is on for every `vix` target (`lib.rs`,
  `main.rs`, `tests/`, `examples/`) and for the Vix-owned crates and modules,
  each with a small curated `allow` list for the noisy casts/etc.
- Keep the tree warning-clean: `cargo clippy --workspace` must be clean.

## Non-negotiable conventions

- **Internationalize all user-facing text.** Never hard-code a display string;
  add a key to `locales/app.yml` and render it with `t!`. Data crates store i18n
  _keys_; the host translates. See [`docs/i18n.md`](docs/i18n.md).
- **One action, one implementation.** Menu items, palette commands, and
  shortcuts all dispatch through `App::run_action` using string action ids
  (`file.save`, `view.theme`, …). Add the behavior there once.
- **Built-in themes are monochrome.** One fg, one bg; emphasis via dim and full
  intensity (no bold or italic); reversed video only for selections and the
  cursor. Color belongs only to custom JSON themes. See
  [`docs/themes.md`](docs/themes.md).
- **Keep the logic terminal-independent.** Editing/state logic lives in the
  library and is tested without a TTY. Rendering lives only in `src/ui.rs`.
- **Input dispatch is keyway-aware.** Raw keys route through the active _keyway_
  (Apple / Emacs / Vim) in `App::on_key`; keyways translate keys into the same
  `run_action` calls and editor motions rather than duplicating behavior. See
  `spec/keyway-chooser.md`.
- **One `ratatui` version.** The whole widget stack must agree on `ratatui` 0.30
  / `crossterm` 0.29. Don't add a widget crate on a different version.

## Where things live

| You want to…                         | Go to…                                                       |
| ------------------------------------ | ------------------------------------------------------------ |
| Add/route a command                  | `src/app.rs` (`run_action`), `src/menu.rs`, `src/palette.rs` |
| Change rendering                     | `src/ui.rs`                                                  |
| Add/translate UI text                | `locales/app.yml` (+ `t!` at the call site)                  |
| Add a setting                        | `src/settings.rs`                                            |
| Change the editor widget             | `vix-editor/` (engine reused; widget is Vix's)               |
| Change soft-wrap / bracket rendering | `vix-editor/src/wrap.rs`, `vix-editor/src/brackets.rs`       |
| Change theme colors/model            | `vix-theme-chooser/`                                         |
| Change available UI languages        | `vix-locale-chooser/`                                        |
| Change keyboard navigation styles    | `vix-keyway-chooser/` + keyway dispatch in `src/app.rs`      |
| Change the calendar                  | `vix-date-time-calendar-panel/`                              |

See [`AGENTS/share/crate-map.md`](AGENTS/share/crate-map.md) for the full map.

## Making a change (checklist)

1. Read the relevant `spec/*.md`; update it if intent is changing.
2. Implement; keep editing logic out of `src/ui.rs`.
3. Internationalize any new text (YAML key + `t!`).
4. Document every new public item (`deny(missing_docs)`).
5. Add/extend tests (`tests/integration.rs` or the crate's unit tests).
6. `cargo test --workspace` and `cargo clippy --workspace` must be clean.
7. Note user-visible changes in [`CHANGELOG.md`](CHANGELOG.md).

## Topic guides

- [`AGENTS/conventions.md`](AGENTS/conventions.md) — coding style and patterns.
- [`AGENTS/workflow.md`](AGENTS/workflow.md) — the spec-driven workflow in detail.
- [`AGENTS/share/crate-map.md`](AGENTS/share/crate-map.md) — every crate and file.
- [`AGENTS/share/glossary.md`](AGENTS/share/glossary.md) — shared terms.
