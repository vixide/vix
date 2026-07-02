# Vix for Emacs users

You do not use an editor; you inhabit one. Emacs is your mail client, your
calendar, your shell, and occasionally a text editor. We are not going to
pretend Vix can be your operating system. But if you have ever wished for the
*editing* half of Emacs — the chords, the kill ring, Org mode, the discipline —
without babysitting an init file measured in kilolines, read on.

## The one-minute pitch

Vix is a terminal IDE in Rust: a single static binary that starts instantly,
with LSP, a debugger, Tree-sitter, git tooling, and (yes, really) **Org mode
with roam-style notes** built in. It ships an **Emacs keymap** — `C-x C-s`,
`C-x C-f`, `M-x`, the kill ring — and a **which-key popup** that lists the
candidate chords while a prefix is pending, so the learning curve is a ramp,
not a wall.

```sh
cargo run                    # open Vix in the current directory
# then: View -> Keymap -> Emacs (or M-x after switching: "keymap")
```

## What works out of the box

| Emacs habit | In Vix |
| ----------- | ------ |
| `C-x C-f` / `C-x C-s` / `C-x C-c` | Find file / save / quit |
| `C-x k` / `C-x b` | Kill buffer / switch buffer |
| `C-x 2` `C-x 3` `C-x 1` `C-x 0` `C-x o` | Split, unsplit, other window |
| `M-x` | The command palette (fuzzy, with every command) |
| `C-f` `C-b` `C-n` `C-p` `C-a` `C-e` | Char / line / line-end motions |
| `M-f` / `M-b` | Word forward / backward |
| `C-v` / `M-v` | Page down / up |
| `M-<` / `M->` | Beginning / end of buffer |
| `C-w` / `M-w` / `C-y` | Kill region / kill-ring-save / yank |
| `C-k` | Kill line |
| `C-t` / `M-t` | Transpose chars / words |
| `C-s` | Incremental search |
| `C-/` | Undo |
| `C-g` | Quit (with the customary status message) |

While `C-x` is pending, the **which-key popup** shows what can follow — the
discoverability of `helm`/`vertico` marketing, built into the core.

And there is a **Spacemacs keymap** too: Vi-modal editing with a `Space`
leader (`SPC f f`, `SPC g s`, `SPC w /`), if that era of your life left marks.

## Org mode, seriously

This is the part most "Emacs alternatives" skip. Vix has a real Org menu:

- Headline promote/demote, subtree move, TODO cycling, checkbox toggling with
  auto-updating statistics cookies (`[2/3]`, `[66%]`), fold cycling.
- Export to Markdown and HTML.
- **Roam-style notes**: nodes with IDs, `[[` wiki-link completion, backlinks
  (including a live backlinks panel), dailies with a calendar picker,
  capture, and transclusion.
- Org tables, clocking in/out, and contacts (org-contacts style, with vCard
  import/export).

It is not org-agenda — there is no agenda view yet — but for notes, TODO lists,
and Zettelkasten-style linking, it covers the daily loop.

## Where Vix beats a stock Emacs

- **Batteries included, wired, and charged.** LSP + DAP + Tree-sitter + git
  hunk staging + fuzzy finding + terminal work on first launch. No `use-package`
  incantations, no `M-x package-refresh-contents` at the worst moment.
- **Startup is instant.** One native binary. No daemon required, no
  `esup`-driven guilt.
- **It cannot break itself.** Configuration is a TOML file, not a program. An
  upgrade cannot send you spelunking through your init file at 9am.
- **Memory safety.** Rust with `unsafe` forbidden — a different reliability
  contract than a C core running elisp.
- **The undo tree persists** across sessions by default, content-hash guarded —
  `undo-tree` semantics without the package.
- **Kill-ring history is visible**: `Edit -> Paste from History` shows the ring
  and lets you pick, mouse or keys.

## Where Emacs still wins

We will not insult you:

- **Elisp.** Emacs is a programmable environment; Vix is an editor with good
  defaults. There is no extension language. If your workflow *is* your
  `init.el`, Vix is a companion, not a replacement.
- **The ecosystem.** Magit, org-agenda, TRAMP, dired's depths, mu4e, ERC —
  decades of accumulated capability. Vix's git tooling is good (hunk staging,
  blame, branches, stash, merge-conflict resolution); it is not Magit.
- **The mark and the region.** Vix selects with `Shift`+motion (and has
  multi-cursor and rectangles), but there is no `C-SPC` mark, no `C-x C-x`.
- **Self-documentation.** `F1` lists every shortcut and the palette lists every
  command, but there is no `C-h f` that jumps to source.

## An honest experiment

1. `cargo build --release`; put `vix` on your `PATH`.
2. Open your notes directory: `vix ~/org` — switch the keymap to **Emacs**,
   then poke the Org menu at your existing `.org` files.
3. Lean on `M-x` (the palette) and the which-key popup instead of docs.
4. Try what Emacs makes you assemble: stage a hunk from the Git menu, run the
   debugger from the Run menu, open the HTTP client on a `.http` buffer, let
   LSP rename a symbol project-wide.

Keep Emacs for what only Emacs does. But the next time you are on a bare
server, or a colleague's machine, or you just want the editor to be *someone
else's* config problem — `vix .` is a very comfortable place for Emacs hands.

## See also

- [`docs/keymaps/index.md`](../keymaps/index.md) — every keymap, every binding.
- [`spec/keymaps/index.md`](../../spec/keymaps/index.md) — the authoritative spec.
- [`spec/org/index.md`](../../spec/org/index.md) — the Org feature spec.
- [`index.md`](../../index.md) — the full feature tour.
