# AGENTS.md

Guidance for AI agents and human contributors working in the Vix repository.
This file is the entry point; see [`AGENTS/`](AGENTS/) for topic guides and
[`index.md`](index.md) for the full documentation map.

## What Vix is

Vix is a keyboard-friendly terminal text editor (a "Simple Terminal Rust IDE"),
built on `ratatui`. It is a **single Cargo crate** (edition 2024, no workspace
members): the application plus ~70 focused modules under `src/`, including the
custom editor widget `editor_core`. See [`docs/architecture/index.md`](docs/architecture/index.md).

## Source of truth

`spec/*.md` is the **specification and the source of truth**. Development is
spec-driven: when behavior and spec disagree, decide which is correct, then make
them match — update the spec when intent changes, update the code when the code
drifted. Keep specs and implementation in sync.

## Build, test, lint

```sh
cargo build                 # build the vix binary + library
cargo test                  # integration + unit + doc tests (no terminal needed)
cargo clippy --workspace --all-targets -- -D warnings   # lints; kept clean
cargo run                   # run the editor in the current directory
cargo run -- --locale fr    # run in a specific language
```

Edition 2024; the toolchain floor is in `Cargo.toml` (`rust-version`). Syntax
grammars are feature-gated: `--features syntax-all` for every grammar,
`--no-default-features` for none.

## Hard rules enforced by the build

The `vix` crate sets `#![deny(missing_docs)]` and `#![forbid(unsafe_code)]`
(see `src/lib.rs`). Therefore:

- **Every public item needs a doc comment.** A new `pub fn`/`struct`/`field`
  without `///` fails the build.
- **No `unsafe`.**
- **`#![warn(clippy::pedantic)]`** is on at the crate root **and repeated in
  every module file**. There is no blanket `#![allow(clippy::pedantic)]` and no
  `#![allow(missing_docs)]`; fix findings in code. The only sanctioned allows are
  four targeted `#[allow(clippy::struct_excessive_bools)]` on `App`, `Settings`,
  `SearchBar`, `WorkspaceSearch`.
- Keep the tree clean: `cargo clippy --workspace --all-targets -- -D warnings`.

## Non-negotiable conventions

- **Internationalize all user-facing text.** Never hard-code a display string;
  add a key to `locales/app.yml` and render it with `t!`. Data modules store i18n
  _keys_; the host translates. See [`docs/internationalization/`](docs/internationalization/).
- **One action, one implementation.** Menu items, palette commands, and
  shortcuts all dispatch through `App::run_action` using string action ids
  (`file.save`, `view.theme`, …). Add the behavior there once.
- **Built-in themes are monochrome.** One fg, one bg; emphasis via dim and full
  intensity (no bold or italic); reversed video only for selections and the
  cursor. Color belongs only to custom JSON themes. See
  [`docs/themes/index.md`](docs/themes/index.md).
- **Keep the logic terminal-independent.** Editing/state logic lives in the
  library and is tested without a TTY. Rendering lives only in `src/ui.rs`.
- **Input dispatch is keymap-aware.** Raw keys route through the active _keymap_
  (Apple / Emacs / Vim) in `App::on_key`; keymaps translate keys into the same
  `run_action` calls and editor motions rather than duplicating behavior. See
  `spec/keymaps`.
- **One `ratatui` version.** The whole widget stack must agree on `ratatui` 0.30
  / `crossterm` 0.29. Don't add a widget crate on a different version.

## Where things live

| You want to…                         | Go to…                                                       |
| ------------------------------------ | ------------------------------------------------------------ |
| Add/route a command                  | `src/app.rs` (`run_action`), `src/menu.rs`, `src/palette.rs` |
| Change rendering                     | `src/ui.rs`                                                  |
| Add/translate UI text                | `locales/app.yml` (+ `t!` at the call site)                  |
| Add a setting                        | `src/settings.rs`                                            |
| Change the editor widget             | `src/editor_core/` (engine reused; widget is Vix's)         |
| Change soft-wrap / bracket rendering | `src/editor_core/wrap.rs`, `src/editor_core/brackets.rs`    |
| Change theme colors/model            | `src/theme_model.rs`                                        |
| Change available UI languages        | `src/locale_model.rs`                                       |
| Change keyboard navigation styles    | `src/keymap_model.rs` + keymap dispatch in `src/app.rs`     |
| Change the calendar                  | `src/calendar_panel.rs`                                     |
| Change spell checking                | `src/spellcheck.rs` + wiring in `src/app.rs` / `src/ui.rs`  |
| Change git status/diff/staging       | `src/git.rs` + wiring in `src/app.rs` / `src/ui.rs`         |
| Change the find/replace engine       | `src/find_panel.rs` (matches/replace_all/unescape/PathFilter) |
| Change LSP support                   | `src/lsp.rs` (host) + `src/lsp_core/` (protocol)            |

See [`AGENTS/share/crate-map.md`](AGENTS/share/crate-map.md) for the full map.

## Making a change (checklist)

1. Read the relevant `spec/*.md`; update it if intent is changing.
2. Implement; keep editing logic out of `src/ui.rs`.
3. Internationalize any new text (YAML key + `t!`).
4. Document every new public item (`deny(missing_docs)`).
5. Add/extend tests (`tests/integration.rs` or a module's unit tests).
6. `cargo test` and `cargo clippy --workspace --all-targets -- -D warnings` clean.
7. Note user-visible changes in [`CHANGELOG.md`](CHANGELOG.md).

## Topic guides

- [`AGENTS/conventions.md`](AGENTS/conventions.md) — coding style and patterns.
- [`AGENTS/workflow.md`](AGENTS/workflow.md) — the spec-driven workflow in detail.
- [`AGENTS/share/crate-map.md`](AGENTS/share/crate-map.md) — every module and file.
- [`AGENTS/share/glossary.md`](AGENTS/share/glossary.md) — shared terms.
