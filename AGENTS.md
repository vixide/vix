# AGENTS.md

Guidance for AI agents and human contributors working in the Vix repository.
This file is the entry point; see [`agents/`](agents/) for topic guides and
[`index.md`](index.md) for the full documentation map.

## Governance

Read [`AI_STATEMENT.md`](AI_STATEMENT.md) before doing anything outward-facing
or hard to reverse (a force-push, publishing a release, publishing a package).
It says what an AI agent is and isn't pre-authorized to do here without asking
first — the default is to confirm; standing exceptions are listed there and
nowhere else. Two exceptions stand today: `cargo publish`, and *judging* that
a specific release is ready (version/`CHANGELOG.md` correct, `scripts/check`
green, scope complete) — an agent may decide that on its own rather than
asking first. Neither extends to cutting a tagged release or creating a
GitHub/GitLab/Codeberg Release; that's still confirmed first.

## What Vix is

Vix is a keyboard-friendly terminal text editor (a "Simple Terminal Rust IDE"),
built on `ratatui`. It is a **Cargo workspace** (edition 2024): a thin **App
shell** (root package `vix`, `src/`) — CLI, event loop, `App` state, rendering,
explorer — over **105 focused `vix-*` member crates** under `crates/`, including
the custom editor widget `vix-editor-core`. `src/lib.rs` re-exports the member
crates under short module names (`pub use vix_git as git;`), so `crate::git`,
`crate::menu`, `crate::db` still name them. See
[`docs/architecture/index.md`](docs/architecture/index.md).

## Source of truth

Specs are the **specification and the source of truth**, and development is
specification-driven. Each member crate owns its spec at
`crates/<crate>/spec/index.md` (multi-topic crates add `spec/<topic>/index.md`);
the repo-root `spec/` holds only cross-cutting / app-level and build/meta specs
(`index`, `navigation`, `tools`, `test`, `ci`, `comparisons`, `emacs-menus`,
`license`, `trademarks`, `rust-msrv-n-minus-2`, `rust-clippy-pedantic`,
`dependabot`, …).
When behavior and spec disagree, decide which is correct, then make them match —
update the spec when intent changes, update the code when the code drifted. Keep
specs and implementation in sync.

Specs are living documentation, so they are checked like code:
`scripts/check-docs` fails on a broken link or path, on a crate with no spec, on
a crate missing from the crate map, and on a `README.md` that has drifted from
its `index.md` twin. Some documentation directories carry both names — `index.md`
is what the doc map links, `README.md` is what a forge renders when you browse
to the directory — and where both exist they must stay identical. Make
`README.md` a symlink to `index.md` (`ln -s index.md README.md`), so they
can't drift apart; `scripts/check-docs` also accepts a real byte-identical
copy, but every one currently in the repo is a symlink.

## Build, test, lint

```sh
cargo build                 # build the vix binary + library
cargo test                  # integration + unit + doc tests (no terminal needed)
cargo clippy --workspace --all-targets -- -D warnings   # lints; kept clean
cargo run                   # run the editor in the current directory
cargo run -- --locale fr    # run in a specific language
cargo bench                 # Criterion benchmarks over the hot paths
cargo +nightly fuzz run <target>   # fuzz a pure core (see fuzz/README.md)
scripts/check-docs          # documentation integrity (links, twins, crate specs)
```

Edition 2024. The toolchain floor is `rust-version` in `Cargo.toml`; the policy
is **current stable minus two**, verified by CI — see
[`spec/rust-msrv-n-minus-2/index.md`](spec/rust-msrv-n-minus-2/index.md). Syntax
grammars are feature-gated: `--features syntax-all` for every grammar,
`--no-default-features` for none.

`scripts/check` (or `make check`) runs the whole gate locally — fmt, build,
clippy, tests, `cargo doc`, and the documentation checks. CI enforces the same gate on all
three forges Vix is pushed to (GitHub, GitLab, Codeberg); when the gate changes,
change every forge's config with it. See [`spec/ci/index.md`](spec/ci/index.md).

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
  macOS / IntelliJ Windows / Eclipse / Sublime Text) in `App::on_key`; keymaps
  translate keys into the same `run_action` calls and editor motions rather than
  duplicating behavior. On macOS, `Command` is folded into `Control` first, so
  every keymap sees a chord it already knows. See `crates/vix-keymap-model/spec`.
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
| Change clipboard access              | `crates/vix-clipboard/` (opt-in platform pasteboard)       |
| Change terminal input (paste, modifiers) | `src/main.rs` (terminal modes) + `App::on_paste` / `App::on_key` |
| Change Org tables / column view      | `crates/vix-org-table/`, `crates/vix-org/src/columns.rs`, `src/column_view.rs` |
| Add a benchmark or fuzz target       | `benches/`, `fuzz/fuzz_targets/` (see `spec/test/index.md`) |

See [`agents/share/crate-map.md`](agents/share/crate-map.md) for the full map.

## Making a change (checklist)

1. Read the owning crate's `spec/index.md` (or the cross-cutting root `spec/`);
   update it if intent is changing.
2. Implement in the owning crate; keep editing logic out of `src/ui.rs`.
3. Internationalize any new text (YAML key + `t!`).
4. Document every new public item (`deny(missing_docs)`).
5. Add/extend tests (`tests/integration.rs` or a module's unit tests).
6. `cargo test` and `cargo clippy --workspace --all-targets -- -D warnings` clean
   (or run `scripts/check`, the local CI-parity gate), and `scripts/check-docs`
   clean if you touched documentation.
7. Note user-visible changes in [`CHANGELOG.md`](CHANGELOG.md).
8. Spelling: prose/docs are checked with CSpell (`cspell.json`); add project terms
   to the external dictionary `project-words.txt`.

## Topic guides

The guides live in [`agents/`](agents/) — lowercase, like every other directory
here; only this entry-point file is uppercase, because agent tooling looks for
that name (see [`spec/agents-directory-name-is-lowercase/index.md`](spec/agents-directory-name-is-lowercase/index.md)).

- [`agents/conventions.md`](agents/conventions.md) — coding style and patterns.
- [`agents/workflow.md`](agents/workflow.md) — the spec-driven workflow in detail.
- [`agents/share/crate-map.md`](agents/share/crate-map.md) — every module and file.
- [`agents/share/glossary.md`](agents/share/glossary.md) — shared terms.
