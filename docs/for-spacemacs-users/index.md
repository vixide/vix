# Vix for Spacemacs users

Spacemacs found the synthesis: Vim's modal editing, Emacs's depth, and a
`Space` leader that turns "what was that binding?" into a guided menu. The
cost is carrying all of Emacs underneath — the init time, the elisp layers,
the occasional `.spacemacs` archaeology after an update. Vix keeps the
synthesis and drops the substrate: modal editing plus a `Space` leader with a
which-key popup, in one native binary that starts before Emacs finds its
config.

## The one-minute pitch

Vix is a terminal IDE written in Rust with LSP, a DAP debugger, Tree-sitter,
git hunk staging, and Org-mode notes built in. Its **Spacemacs keymap** is
Vi-style modal editing (Normal/Insert, a `:` command line) with a `Space`
leader whose sequences mirror the ones in your fingers — and a **which-key
popup** that lists the candidate keys the moment you pause, exactly the
discoverability that made Spacemacs click.

```sh
cargo run                    # open Vix in the current directory
# then: View -> Keymap -> Spacemacs
```

## Your leader, mapped

In Normal mode over the editor, `SPC` opens the leader (the status bar echoes
the sequence; the which-key popup lists what can follow):

| Spacemacs habit | In Vix |
| --------------- | ------ |
| `SPC SPC` (M-x) | Command palette |
| `SPC f f` / `SPC f r` / `SPC f s` | Find file / recent files / save |
| `SPC f p` / `SPC p p` | Switch project |
| `SPC p f` | Project file/command palette |
| `SPC p t` | Project tree (file explorer) |
| `SPC b n` / `SPC b p` / `SPC b d` | Next / previous / delete buffer |
| `SPC g s` / `SPC g g` / `SPC g b` | Git status / git summary / blame |
| `SPC w /` / `SPC w -` | Split vertical / horizontal |
| `SPC w d` / `SPC w w` | Delete split / other window |
| `SPC s s` / `SPC s p` | Search buffer / search project |
| `SPC t n` / `SPC t w` | Toggle line numbers / whitespace |
| `SPC ;` | Comment |
| `SPC q q` | Quit |

**Modal editing:** Normal mode is the full Vi-keymap vocabulary — `h j k l`,
`w`/`b`, `0`/`^`/`$`, `gg`/`G`, `x`, `dd`/`yy`/`p`, `u`, `/` + `n`/`N`, `%`,
and `i a I A o O` into Insert mode; `Esc` returns. The `:` command line
handles `:w`, `:q`, `:wq`, `:N` (go to line), `:e path`, and `:Ex`.

## Where Vix beats Spacemacs

- **Startup and stability.** One native binary, milliseconds to open, and no
  layer system to break on update. Your setup cannot bit-rot because there is
  no setup.
- **The IDE layer is pre-assembled.** LSP, debugger, test runner, terminal,
  fuzzy finding, and git hunk staging are core features — not layers to
  enable, pin, and debug.
- **Which-key everywhere it matters,** built into the core rather than a
  package, covering both the `Space` leader and the Emacs keymap's `C-x`
  prefix.
- **Org, genuinely.** Headlines, TODO cycling, checkbox statistics, export,
  and roam-style notes with backlinks and dailies — the Spacemacs `org` layer
  is often *why* people endure the rest; here it is just a menu.
- **Memory safety.** Rust with `unsafe` forbidden.

## Where Spacemacs still wins

- **Depth of the leader tree.** Spacemacs binds thousands of sequences across
  dozens of layers; Vix's leader covers the daily core (files, buffers,
  projects, git, windows, search, toggles). The command palette (`SPC SPC`)
  fills the gaps, but deep `SPC`-trees are not replicated.
- **Evil's completeness.** Text objects, registers, visual mode, macros-on-`q`
  — Spacemacs inherits all of Vim's grammar via Evil. Vix's modal layer is a
  practical subset.
- **elisp and layers.** If you write your own layers or lean on niche ones
  (mail, IRC, exotic languages), Emacs remains irreplaceable.
- **org-agenda.** Vix has rich Org editing and roam notes, but no agenda view.

## An honest experiment

1. `cargo build --release`; put `vix` on your `PATH`.
2. In a project: `vix .` — press `Ctrl+P`, type `keymap`, choose **Spacemacs**.
3. Lean on the leader: `SPC f f` to a file, `SPC g s` for git, `SPC w /` to
   split, and pause after `SPC` to let the which-key popup teach you the rest.
4. Try what your layers used to do: rename a symbol via LSP, stage one hunk,
   set a breakpoint from the Run menu, open your Org notes and follow a
   backlink.

If your `.spacemacs` sparks joy, keep it. But when you want the *idea* of
Spacemacs — modal speed, leader-key calm, discoverable everything — without
the weight underneath, Vix is that idea, compiled.

## See also

- [`docs/keymaps/index.md`](../keymaps/index.md) — every keymap, every binding.
- [`docs/for-vim-users/index.md`](../for-vim-users/index.md) — the Vi keymap's
  fuller modal vocabulary.
- [`docs/for-emacs-users/index.md`](../for-emacs-users/index.md) — the Emacs
  keymap and Org story.
- [`index.md`](../../index.md) — the full feature tour.
