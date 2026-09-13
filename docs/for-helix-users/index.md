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
  Occurrences** (bound to `Ctrl+D` in the **Sublime Text** keymap,
  `Ctrl+Shift+L` under **VSCode**), column (rectangular) selection, and
  manual multi-cursor placement are all built in — see
  [`docs/multiple-cursors/index.md`](../multiple-cursors/index.md).
- **A single static binary.** No Node, no Python, no separate language
  servers to wire up beyond the servers themselves.

## What's genuinely different

- **No selection-first modal.** Helix's defining idea — select a range,
  *then* act on it, so you always see what you're about to change — has no
  equivalent in Vix. Vix's closest modal option, the **Vi** keymap, is
  traditional Vim-style verb-then-motion (`dw`, not select-then-`d`). If
  the selection-first model is *why* you use Helix, Vix's Vi keymap will
  feel like a step backward, not a lateral move — this is the honest
  answer, not a sales pitch.
- **A leader-key alternative, if you want one.** The **Spacemacs** keymap
  (Vi-modal plus a `Space` leader — `SPC f f` find file, `SPC g s` git
  status — with a which-key popup showing candidate keys) is the nearest
  thing Vix has to "modal editing with discoverable chords," which is
  closer in spirit to Helix's own discoverable-by-design command set than
  plain Vi mode is.
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

- **The selection-first editing model itself**, if that's what you're
  actually here for — see above. Vix does not replicate it.
- **A tighter, more opinionated core.** Helix's smaller surface area is a
  deliberate design choice, not a gap to fill — some people prefer that.
- **Kakoune-lineage muscle memory.** If your reflexes came from Kakoune or
  Helix specifically, Vix's Vi keymap won't match them; Vim's will, more or
  less, and Helix's own won't be replicated by either.

## An honest experiment

Give it one afternoon on a real task:

1. `cargo build --release` and put `vix` on your `PATH`.
2. Open a project: `vix .`
3. Try the **Spacemacs** keymap first (`Ctrl+P`, type `keymap`) — its
   which-key popup is the closest thing here to Helix's own discoverability.
   If verb-then-motion is fine, try plain **Vi** instead.
4. Try what Helix leaves to you: stage a hunk from **Git → Changes…**, open
   the integrated terminal (`` Ctrl+` `` in most keymaps), or set a
   breakpoint from **Run** and step through it.

If the selection-first model turns out to be the whole reason you're here,
Vix won't replace Helix for you — and that's a fair place to land. But if
what you actually wanted was "zero-config IDE, one binary, plus the things
Helix leaves out on purpose," this is worth the afternoon.

## See also

- [`docs/comparison/index.md`](../comparison/index.md) — the full
  feature-parity matrix.
- [`docs/keymaps/index.md`](../keymaps/index.md) — every keymap, every
  binding.
- [`docs/multiple-cursors/index.md`](../multiple-cursors/index.md) —
  Vix's multi-cursor and column-selection support.
- [`index.md`](../../index.md) — the full feature tour.

---

Vix™ and Vix IDE™ are trademarks.
