# Comparison

An honest feature-parity matrix against four editors people usually compare
Vix™ to: **Vim** (stock, no plugins — the baseline the
[for-vim-users](../for-vim-users/index.md) page also assumes), **Helix**
(the other "modern terminal editor written in Rust"), **Micro** (the
simplest terminal editor with a plugin system), and **Zed** (a modern GUI
editor, included as the "if you'd consider a GUI" comparison point). See
also [for-emacs-users](../for-emacs-users/index.md),
[for-visual-studio-code-users](../for-visual-studio-code-users/index.md),
and [for-helix-users](../for-helix-users/index.md) for the prose version
of some of these same comparisons.

`✓` built in, no plugin needed. `✗` not built in (a plugin may exist).
`~` partial — see the footnote.

| Feature | Vix | Vim | Helix | Micro | Zed |
| --- | :---: | :---: | :---: | :---: | :---: |
| Interface | TUI | TUI[^gvim] | TUI | TUI | GUI |
| Modal editing, on by default | ✗[^keymap] | ✓ | ✓ | ✗ | ✗ |
| LSP (diagnostics, rename, go-to-def) | ✓ | ✗ | ✓ | ✗ | ✓ |
| Debugger (DAP) | ✓ | ✗ | ✓ | ✗ | ✓ |
| Tree-sitter highlighting | ✓ | ✗ | ✓ | ✗ | ✓ |
| Built-in fuzzy finder / palette | ✓ | ✗ | ✓ | ✗ | ✓ |
| Multiple cursors | ✓ | ✗ | ✓ | ✓ | ✓ |
| Git client (stage, commit, log, blame) | ✓ | ✗ | ~[^helixgit] | ✗ | ✓ |
| Integrated terminal | ✓ | ✓ | ✗ | ✗ | ✓ |
| Plugin/scripting system | ✓ (Rhai) | ✓ (Vimscript) | ~[^steel] | ✓ (Lua) | ~[^zedext] |
| Persistent, branching undo | ✓ | ~[^undofile] | ✗ | ✗ | ✗ |
| Org-mode (outline, TODO, agenda) | ✓ | ✗ | ✗ | ✗ | ✗ |
| Single static binary, no runtime | ✓ | ✓ | ✓ | ✓ | ✗ |
| Native remote/SSH project editing | ✗ | ✓[^sshvim] | ✗ | ✗ | ✓ |

[^gvim]: A GUI build (`gvim`) exists alongside the terminal one; the
    comparison above is against stock terminal Vim, matching the
    [for-vim-users](../for-vim-users/index.md) page's own baseline.
[^keymap]: Off by default (the **Apple** keymap), but a full **Vi** or
    **Spacemacs** (Vi-modal plus a `Space` leader) keymap is one menu
    choice away — see [keymaps](../keymaps/index.md).
[^helixgit]: Helix shows a diff gutter and has some Git awareness, but by
    its own stated design does not aim to be a Git client — no stage/
    commit/log panel.
[^steel]: [Steel](https://github.com/mattwparas/steel), a Scheme-based
    scripting/plugin layer, is still under active development as of 2026,
    not yet a stable, documented plugin API.
[^zedext]: Zed's extensions add languages, themes, and debug adapters, not
    general-purpose editor scripting.
[^undofile]: Vim's `:set undofile` persists undo history across sessions,
    and its undo tree branches — the underlying data model is comparable.
    What Vix adds is a browsable **Undo Branch** UI with no config: Vim's
    branches exist but need a plugin (e.g. Gundo/undotree) to browse.
[^sshvim]: Not a feature of Vim itself — `ssh host -t vim file` runs Vim
    on the remote host over a plain terminal session, the way any TUI
    program can be used remotely.

None of this is "Vix wins every row" — it doesn't, and a matrix this
compressed necessarily flattens nuance the linked prose pages restore. Read
it as: **Vix bundles into one binary what Helix and Zed also ship built in
(LSP/DAP/tree-sitter/pickers), Vim and Micro leave to plugins**, plus a few
things none of the other four have at all (Org-mode, persistent undo with
no setup, a bundled DB/HTTP/JWT/regex tool shelf) — traded against real
gaps of its own: no native remote/SSH editing, no modal editing by default
(opt-in instead), and a much younger, smaller plugin ecosystem than Vim's
or Zed's (Rhai scripts today; see
[`docs/scripting/index.md`](../scripting/index.md)).

## Where Vix is simplest

No plugin manager, no extension marketplace, no JavaScript/TypeScript
runtime to sandbox. What ships in the binary is everything there is —
smaller attack surface, nothing to configure before it works.

## Where Vix is more capable than Micro specifically

Beyond the table above: a **left-side file explorer**, a **right-side
message drawer**, a **bottom dock** for logs/output/test-results/DB
workbench, switchable **themes**, and a **switchable UI language**
(internationalization) — see [`index.md`](../../index.md) for the full
tour.

## Easier to pick up than Vim or Helix, for a newcomer

Both default to modal editing, which is unfamiliar territory for most
people's first day. Vix's default (**Apple**) keymap uses ordinary
`Ctrl+C`/`Ctrl+V`-style shortcuts and shows a menu bar — modal editing is a
menu choice away (**View → Keymap**) for whoever wants it, not the only
door in.

---

Vix™ and Vix IDE™ are trademarks.
