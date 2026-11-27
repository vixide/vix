# ADR 2: Build the terminal UI on `ratatui` and `crossterm`

- **Status:** Accepted
- **Date:** 2026-06-08 (commit a7b9ebaf, the initial commit)

## Context

The initial commit (a7b9ebaf) describes STRIDE as "A keyboard- and
mouse-friendly TUI text editor built on ratatui 0.30, ratatui-code-editor
(Tree-sitter syntax highlighting), crossterm, regex, jiff, and ratatui-image."
Its Cargo.toml carries the comments "Terminal UI core. The whole widget
ecosystem must agree on this version." on `ratatui = "0.30"` and "Must match
the crossterm that ratatui 0.30 re-exports." on `crossterm = "0.29"`. The
editing area was the third-party `ratatui-code-editor` 0.0.4, and images used
`ratatui-image`.

**The repository does not record the alternatives considered**: no commit,
spec or plan compares `ratatui` with another terminal UI library. What it
does record is that the choice was made together with its widget ecosystem:
an editor widget and an image widget that both already tracked `ratatui`
0.30 (the first spec's crate table, now in
[`spec/index/index.md`](../../spec/index/index.md)).

## Decision

Vix draws its terminal UI with `ratatui` over the `crossterm` backend, and
the whole widget stack stays on one `ratatui` version. In the words of
[`AGENTS.md`](../../AGENTS.md): "The whole widget stack must agree on
`ratatui` 0.30 / `crossterm` 0.29. Don't add a widget crate on a different
version." Rendering lives in the App shell, in `tui/src/ui.rs` and its
`tui/src/ui/*.rs` submodules.

## Consequences

- **The editor widget became Vix's own.** The same day, 308cdd74 vendored
  `ratatui-code-editor` to put its Tree-sitter grammars behind features
  (release binary 18M with every grammar, 4.9M with the common set, 3.0M with
  none). On 2026-06-09 (b93e5695) the vendored copy became the internal
  `vix-code-editor-panel`, and on 2026-06-10 (48f152fa) it was replaced by
  "Vix's fully-custom code-editor widget": the Tree-sitter, rope and undo
  engine was kept, and the widget, input, mouse handling and a soft-wrap
  renderer were rewritten. It is now `crates/vix-editor-core`. So the one
  `ratatui` widget Vix depended on most is no longer a third-party crate.
- **One version, everywhere.** A widget crate that needs another `ratatui`
  cannot be added, and a `ratatui` upgrade has to move the editor, the image
  widget and every panel together.
- **Tests render without a terminal.** `ratatui`'s `TestBackend` lets tests
  draw a frame to a buffer: the initial commit already had "a TestBackend
  image render" test, and `tui/tests/snapshots.rs` now snapshots whole
  screens as text. See [`spec/test/index.md`](../../spec/test/index.md).
- **Terminal features come through `crossterm`.** Bracketed paste, focus
  events, the macOS `Command` key folded into `Control` and suspend/resume
  are handled in `tui/src/main.rs` and `App::on_key`
  ([`AGENTS.md`](../../AGENTS.md), "Where things live").
- **The desktop GUIs reuse the App shell.** The seven front ends in `gui/`
  wrap the same App shell instead of a terminal
  ([`docs/architecture/index.md`](../architecture/index.md),
  [`gui/spec/index.md`](../../gui/spec/index.md)).

## Enforced by

- The "One `ratatui` version" rule in [`AGENTS.md`](../../AGENTS.md).
- `Cargo.toml` `[workspace.dependencies]`, where `ratatui` and `crossterm`
  are declared once for every crate. `cargo deny` warns, but does not fail,
  when two versions of one crate end up in the build
  (`multiple-versions = "warn"` in [`deny.toml`](../../deny.toml)).
