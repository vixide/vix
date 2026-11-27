# ADR 5: Optional capabilities as default-on Cargo features

- **Status:** Accepted. It reverses the earlier "wired in means always
  compiled" decision of T103 and T111 for the areas listed below.
- **Date:** 2026-10-18 (commit 33bd62b7, "feat(T586): capability
  Cargo-feature framework"); researched 2026-10-09 (`plan.md`, "Capability
  Cargo features")

## Context

Vix has had two kinds of Cargo feature.

**Grammar features, since 2026-06-08.** On the first day, 308cdd74 put each
Tree-sitter grammar behind a feature (`lang-rust`, `lang-json`, ...) with
`syntax-common` as the default and `syntax-all` for every grammar, because
grammars dominated the binary: 18M with all of them, 4.9M with the common
set, 3.0M with none. Files whose grammar is not compiled in open as plain
text.

**Capabilities, since 2026-10-18.** Before that, an area wired into the App
shell was always compiled: T101 had put scripting behind a feature, T103 made
it unconditional again, "reasoning that once a crate is wired into the App
shell (startup, palette, menu, prompts) it is as core as `vix-editor`", and
T111 did the same for `vix-modal` (`plan.md`). The 2026-10-09 research
measured, with `cargo metadata`, how many third-party crates each area
alone pulls in: `sql` 48, `image` 81, `script` 9, `terminal` 10, and
`sql`, `http` and `ai` together 88, while `git`, `jj`, `lsp`, `dap` and
`org` pulled in none (`plan.md`; `tasks.md`, Run L). It concluded that
the reasoning against features "holds only as long as there's no payoff".

## Decision

Capabilities with a measured payoff are Cargo features of the `vix-tui`
package, all on by default
([`spec/cargo-features/index.md`](../../spec/cargo-features/index.md)):

- **Two criteria.** A capability earns a feature when it removes many
  third-party crates or a whole outward surface, or (T595) when it is a place
  Vix starts another program: "In a build without them Vix starts no child
  process of its own." `org` stays unflagged: no payoff and the heaviest
  coupling.
- **Default-on.** `default = ["syntax-common", "full"]`, and `full` lists
  every capability (`tui/Cargo.toml`), so released binaries are unchanged.
  `cowork`, `jupyter` and `matrix` are opt-in and not in `full`.
- **Gate at the root, filter at runtime.** `cfg` lives only in the
  `vix-tui` package (`tui/src/capability.rs` and the gated modules); menus
  and the palette stay `cfg`-free and hide what `capability::is_available`
  rejects, and an action id of a compiled-out capability shows "X is not
  included in this build".
- **Settings never lose data.** Settings types stay compiled in every build,
  so a smaller build never deletes a section it does not know on save.

## Consequences

- **A smaller, locked-down build.** Measured on 2026-10-18 (macOS arm64):
  the default build is 21.8 MiB and `--no-default-features` 12.3 MiB, with
  no TLS stack and no outbound network code; built with `[profile.minimal]`
  it is 7.4 MiB. It ships as an extra release artifact (T594,
  `.github/workflows/release-minimal.yml`)
  ([`spec/cargo-features/index.md`](../../spec/cargo-features/index.md)).
- **Stand-ins for compiled-out code.** Shared call sites stay `cfg`-free
  through small `*_off.rs` modules (for example `tui/src/app/scripts.rs`
  has `scripts_off.rs` no-op stand-ins).
- **More builds to keep green.** Every feature combination is built in CI
  (T587, 40f31c24; [`spec/ci/index.md`](../../spec/ci/index.md), "Feature
  combinations"), and changes must also pass
  `cargo clippy -p vix-tui --no-default-features`.
- **Tests depend on the build.** Whole-product tests such as menu-bar
  snapshots gate on `full`, "since a build with any capability off
  legitimately has a different menu bar" (`tui/Cargo.toml`).
- **Two axes.** The grammar features (`syntax-*`, `lang-*`) are older and
  separate; they forward to `vix-editor-core` ([ADR 4](0004-crate-layout.md)).

## Enforced by

- `tui/Cargo.toml` `[features]` and `tui/src/capability.rs` (its tests
  cover the action-prefix map).
- `tui/tests/spawn_sites.rs`, which fails when a file outside its allow
  list starts a process, so a new spawn site has to be gated first.
- The `feature-combos` CI job ([`spec/ci/index.md`](../../spec/ci/index.md)).
