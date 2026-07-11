# AGENTS.md

Guidance for AI agents and human contributors working in the Vix repository.
This file is the entry point; see [`AGENTS/`](AGENTS/) for topic guides and
[`index.md`](index.md) for the full documentation map.

## What Vix is

Vix is a keyboard-friendly terminal text editor (a "Simple Terminal Rust IDE"),
built on `ratatui`. It is a **Cargo workspace** (edition 2024): a thin **App
shell** (root package `vix`, `src/`) — CLI, event loop, `App` state, rendering,
explorer — over ~98 focused **`vix-*` member crates** under `crates/`, including
the custom editor widget `vix-editor-core`. `src/lib.rs` re-exports the member
crates under short module names (`pub use vix_git as git;`), so `crate::git`,
`crate::menu`, `crate::db` still name them. See
[`docs/architecture/index.md`](docs/architecture/index.md).

## Source of truth

Specs are the **specification and the source of truth**, and development is
specification-driven. Each member crate owns its spec at
`crates/<crate>/spec/index.md` (multi-topic crates add `spec/<topic>/index.md`);
the repo-root `spec/` holds only cross-cutting / app-level and build/meta specs
(`index`, `navigation`, `comparisons`, `license`, `tools`, `rust-clippy-pedantic`,
…). When behavior and spec disagree, decide which is correct, then make them
match — update the spec when intent changes, update the code when the code
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

Every crate sets `#![deny(missing_docs)]` and `#![forbid(unsafe_code)]`
(see each crate's `src/lib.rs`). Therefore:

- **Every public item needs a doc comment.** A new `pub fn`/`struct`/`field`
  without `///` fails the build.
- **No `unsafe`.**
- **`#![warn(clippy::pedantic)]`** is on at the crate root **and repeated in
  every module file**. There is no blanket `#![allow(clippy::pedantic)]` and no
  `#![allow(missing_docs)]`; fix findings in code. Sanctioned allows are only a
  few **targeted** ones: `#[allow(clippy::struct_excessive_bools)]` on genuine
  state structs (`App`, `Settings`, `SearchBar`, `WorkspaceSearch`, `editor_core`
  `Editor`) and a handful of `#[allow(clippy::too_many_lines)]` /
  `too_many_arguments` on specific functions that resist further extraction.
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
  (Apple / VSCode macOS / VSCode Windows / Emacs / Vi / Spacemacs / IntelliJ
  macOS / IntelliJ Windows / Eclipse / Sublime Text) in `App::on_key`; keymaps translate keys
  into the same `run_action` calls and editor motions rather than duplicating
  behavior. See `crates/vix-keymap-model/spec`.
- **One `ratatui` version.** The whole widget stack must agree on `ratatui` 0.30
  / `crossterm` 0.29. Don't add a widget crate on a different version.

## Where things live

| You want to…                         | Go to…                                                       |
| ------------------------------------ | ------------------------------------------------------------ |
| Add/route a command                  | `src/app.rs` (`run_action`), `crates/vix-menu/`, `crates/vix-palette/` |
| Change rendering                     | `src/ui.rs`                                                  |
| Add/translate UI text                | `locales/app.yml` (+ `t!` at the call site)                  |
| Add a setting                        | `crates/vix-settings/`                                       |
| Change the editor widget             | `crates/vix-editor-core/` (engine reused; widget is Vix's)  |
| Change soft-wrap / bracket rendering | `crates/vix-editor-core/src/wrap.rs`, `.../brackets.rs`     |
| Change theme colors/model            | `crates/vix-theme/`, `crates/vix-theme-model/`             |
| Change available UI languages        | `crates/vix-locale-model/`, `crates/vix-i18n/`             |
| Change keyboard navigation styles    | `crates/vix-keymap-model/` + keymap dispatch in `src/app.rs` |
| Change the calendar                  | `crates/vix-calendar-panel/`                               |
| Change spell checking                | `crates/vix-spellcheck/` + wiring in `src/app.rs` / `src/ui.rs` |
| Change git status/diff/staging       | `crates/vix-git/` + wiring in `src/app.rs` / `src/ui.rs`   |
| Change the find/replace engine       | `crates/vix-find-panel/` (matches/replace_all/unescape/PathFilter) |
| Change LSP support                   | `crates/vix-lsp/` (host) + `crates/vix-lsp-core/` (protocol) |
| Change the database workbench        | `crates/vix-db/` (module tree + `crates/vix-db/spec`)      |

See [`AGENTS/share/crate-map.md`](AGENTS/share/crate-map.md) for the full map.

## Making a change (checklist)

1. Read the owning crate's `spec/index.md` (or the cross-cutting root `spec/`);
   update it if intent is changing.
2. Implement in the owning crate; keep editing logic out of `src/ui.rs`.
3. Internationalize any new text (YAML key + `t!`).
4. Document every new public item (`deny(missing_docs)`).
5. Add/extend tests (`tests/integration.rs` or a module's unit tests).
6. `cargo test` and `cargo clippy --workspace --all-targets -- -D warnings` clean
   (or run `scripts/check`, the local CI-parity gate).
7. Note user-visible changes in [`CHANGELOG.md`](CHANGELOG.md).
8. Spelling: prose/docs are checked with CSpell (`cspell.json`); add project terms
   to the external dictionary `project-words.txt`.

## Topic guides

- [`AGENTS/conventions.md`](AGENTS/conventions.md) — coding style and patterns.
- [`AGENTS/workflow.md`](AGENTS/workflow.md) — the spec-driven workflow in detail.
- [`AGENTS/share/crate-map.md`](AGENTS/share/crate-map.md) — every module and file.
- [`AGENTS/share/glossary.md`](AGENTS/share/glossary.md) — shared terms.
