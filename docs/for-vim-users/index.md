# Vix for Vim users

You live in a terminal. You think in motions. You have a `.vimrc` you have been
tending for a decade, and every other editor feels like typing through
molasses. Fair — so did we. This page is an honest pitch: what Vix gives you on
day one, where it genuinely beats a stock Vim setup, where Vim still wins, and
how to try Vix for an afternoon without betraying your muscle memory.

## The one-minute pitch

Vix is a terminal IDE written in Rust: one static binary, no runtime, no plugin
manager, no config archaeology. It ships with the things you would otherwise
assemble from twenty plugins — LSP, a DAP debugger, Tree-sitter highlighting,
fuzzy finding, git hunk staging, an integrated terminal, snippets, multi-cursor
editing, and a persistent branching undo tree — already wired together and
turned on. And it has a **Vi keymap**, so `hjkl`, `dd`, `gg`, and `:wq` work the
moment you launch it.

```sh
cargo run                    # open Vix in the current directory
# then: View -> Keymap -> Vi (or the command palette: "keymap")
```

## What works out of the box

Switch to the Vi keymap and the status bar shows `-- NORMAL --`, exactly where
you expect it. Currently supported:

| Vim habit | In Vix |
| --------- | ------ |
| `h` `j` `k` `l` | Motions, plus the arrow keys |
| `w` / `b` | Next / previous word |
| `0` / `^` / `$` | Line start / first non-blank / line end |
| `gg` / `G` | File start / end |
| `x`, `dd`, `yy`, `p` | Delete char, cut line, copy line, paste |
| `u` | Undo (backed by a branching, *persistent* undo tree) |
| `/` then `n` / `N` | Search, next / previous match |
| `%` | Jump to the matching bracket |
| `i` `a` `I` `A` `o` `O` | Enter Insert mode the usual ways |
| `Esc` | Back to Normal mode |
| `:w` `:q` `:q!` `:wq` `:x` | The classics |
| `:42` | Go to line 42 |
| `:e path` | Open a file |
| `:Ex` | Open the file explorer (a netrw nod) |

If you prefer a leader-key workflow, try the **Spacemacs keymap** instead: it is
Vi-modal editing plus a `Space` leader (`SPC f f` find file, `SPC g s` git
status, `SPC w /` split, …), with a which-key popup that shows the candidate
keys while you hesitate. Many Vim users find it the best of both worlds.

## Where Vix beats a stock Vim

- **Zero assembly required.** LSP (diagnostics, rename, references, code
  actions, call hierarchy), a debugger, Tree-sitter, fuzzy finder, git hunk
  staging, terminal, test runner: in Vim these are `nvim-lspconfig` +
  `nvim-dap` + `telescope` + `gitsigns` + a weekend. In Vix they are menu
  items that already work.
- **Discoverability without leaving the keyboard.** Menus (`F10`), a command
  palette (`Ctrl+P`, `>` for commands), and `F1` for every shortcut. You never
  need to remember whether it was `:vsplit` or `:vs` — though `:` still works.
- **Relative line numbers, jump labels, and friends.** `View -> Editor ->
  Relative Line Numbers` for count-free vertical jumps; `Go -> Jump to Line`
  gives leap/EasyMotion-style labels with no plugin.
- **Persistent undo by default.** The undo *tree* (not a line) survives closing
  the file, guarded by a content hash. Vim needs `undofile` plus an undo-tree
  plugin to match this.
- **Safety.** Rust, `#![forbid(unsafe_code)]`, no exploit-friendly embedded
  script runtime. Your editor is not also a package manager.
- **Modern niceties Vim needs plugins for**: clipboard history, minimap,
  sticky scroll, indent guides, rainbow brackets, word-occurrence highlighting,
  Org mode with roam-style notes, an HTTP client, and a spreadsheet-style CSV
  editor — all built in, all optional.

## Where Vim still wins

Honesty is the best advocacy:

- **The full Vi language.** Vix's Normal mode is a practical subset. There are
  no counts (`3w`), no text objects (`ciw`, `da"`), no registers, no visual
  mode, no macros-via-`q` (Vix has recorded macros, but in the Edit menu). If
  your hands speak fluent `d2f)`, you will notice.
- **Extensibility.** No Vimscript, no Lua, no plugin ecosystem. Vix's answer is
  to build the common 90% in — but if your workflow depends on a niche plugin,
  Vix cannot replace it today.
- **Ubiquity.** Vim is on every server you will ever ssh into. Vix is a binary
  you must bring along (though it *is* a single static binary, so `scp` works).
- **Maturity.** Vim has thirty years of edge cases handled. Vix is young and
  moving fast.

## An honest experiment

Give it one afternoon on a real task:

1. `cargo build --release` and put `vix` on your `PATH`.
2. Open a project: `vix .` — then switch the keymap: `Ctrl+P`, type `keymap`,
   pick **Vi** (or **Spacemacs**).
3. Do your normal work. When a Vim reflex misses, hit `F1` (all shortcuts) or
   `Ctrl+P` `>` (all commands) instead of reaching for the docs.
4. Try the things Vim makes hard: stage a single hunk from `Git -> Stage Hunk`,
   rename a symbol with LSP, set a breakpoint from the Run menu, pop open the
   undo *branch* you abandoned an hour ago (`Edit -> Undo Branch`).

If it does not stick, your `.vimrc` will still be there, unjudging. But you may
find — as we did — that the parts of Vim you love are the *motions*, and Vix
keeps those while deleting the parts you merely tolerate.

## See also

- [`docs/keymaps/index.md`](../keymaps/index.md) — every keymap, every binding.
- [`spec/keymaps/index.md`](../../spec/keymaps/index.md) — the authoritative spec.
- [`index.md`](../../index.md) — the full feature tour.
