# Vix™ for Helix users

You picked an editor that ships LSP, Tree-sitter, and multiple cursors with
zero configuration, and you like that it stayed out of your way about
everything else — no terminal, no git client, philosophically. Vix shares
the "zero-config IDE in one binary" instinct, but takes the opposite bet on
scope: it tries to be the terminal, the git client, and more, not just the
editor. This page is honest about both directions.

## The one-minute pitch

Vix is a terminal IDE written in Rust: one static binary, no runtime. Like
Helix, LSP (diagnostics, rename, references, code actions), a DAP debugger,
Tree-sitter highlighting, and multiple cursors are already wired together —
no plugin, no config. Unlike Helix, Vix also bundles a git client, an
integrated terminal, and a fair amount more (Org-mode, an HTTP client, a
spreadsheet-style CSV editor…), on the theory that a single binary can hold
all of it without becoming a mess.

```sh
cargo run                    # open Vix in the current directory
```

## What's the same instinct

- **Zero-config LSP, DAP, and Tree-sitter.** Same as Helix: open a file,
  get diagnostics, go-to-definition, and highlighting with nothing to
  install. Vix's debugger is menu-driven (**Run**: breakpoints, step
  over/into, call stack, variables, watches) rather than Helix's own
  keybindings, but it's the same "already wired up" deal.
- **Multiple cursors as a first-class feature.** **Edit → Select All
  Occurrences** (bound to `Ctrl+D` in the **Sublime** keymap,
  `Ctrl+Shift+L` under **VSCode**), column (rectangular) selection, and
  manual multi-cursor placement are all built in — see
  [`docs/multiple-cursors/index.md`](../multiple-cursors/index.md).
- **A single static binary.** No Node, no Python, no separate language
  servers to wire up beyond the servers themselves.

## What's genuinely different

- **A Helix keymap, newer and smaller than Helix.** **View → Keymap →
  Helix** gives you select-then-act editing: `w`, `x` or `%` select, then
  `d`, `c`, `y` or `p` act; `v` extends; `g` goto, `mm`, the `Space` leader
  (`Space f`, `Space s`, `Space /`, `Space k`, `Space r`, `Space a`) and the
  `:` command line work as in Helix, with which-key hints after each
  prefix. What's missing: counts, `.` repeat, actions on every selection at
  once (they act on the primary one), named registers, view and window
  modes, and the block cursor. See
  [`docs/keymaps/index.md`](../keymaps/index.md#helix).
- **Not modal by default.** Vix starts in the **Apple** keymap; Helix is
  one menu choice away and the choice persists.
- **Vix keeps things Helix deliberately excludes.** An integrated
  terminal, a git client (stage/commit/log/blame, not just a diff gutter),
  and a menu bar are all in scope for Vix; Helix's own stated design keeps
  it as an editor, not a terminal emulator or git client.
- **Plugin/scripting.** Vix has Rhai scripts today (palette commands, key
  bindings — see [`docs/scripting/index.md`](../scripting/index.md)).
  Helix's Scheme-based Steel plugin layer is still under active
  development, not yet a stable public API — so as of this writing, Vix's
  is the more finished (if smaller-community) story, not the other way
  around.

## Where Helix still wins

- **The full selection-first model.** Vix's Helix keymap covers the core,
  but not multi-selection actions, counts, `.` repeat or Helix's other
  modes. If you rely on those daily, you'll notice.
- **A tighter, more opinionated core.** Helix's smaller surface area is a
  deliberate design choice, not a gap to fill — some people prefer that.

## An honest experiment

Give it one afternoon on a real task:

1. `cargo build --release` and put `vix` on your `PATH`.
2. Open a project: `vix .`
3. Pick the **Helix** keymap (**View → Keymap → Helix**), then edit the way
   you do in Helix: `w` then `d`, `x` then `y`, `Space f` for files.
4. Try what Helix leaves to you: stage a hunk from **Git → Changes…**, open
   the integrated terminal (`` Ctrl+` `` in most keymaps), or set a
   breakpoint from **Run** and step through it.

If you depend on the parts of Helix's model Vix doesn't have yet
(multi-selection actions, counts, `.` repeat), Vix won't replace Helix for
you yet — and that's a fair place to land. But if
what you actually wanted was "zero-config IDE, one binary, plus the things
Helix leaves out on purpose," this is worth the afternoon.

## See also

- [`docs/compare/index.md`](../compare/index.md) — the full
  feature-parity matrix.
- [`docs/keymaps/index.md`](../keymaps/index.md) — every keymap, every
  binding.
- [`docs/multiple-cursors/index.md`](../multiple-cursors/index.md) —
  Vix's multi-cursor and column-selection support.
- [`index.md`](../../index.md) — the full feature tour.

---

Vix™ and Vix IDE™ are trademarks.
