# Comparison

An honest feature-parity matrix, first against the two editors Vix is most
often weighed against: **Vim** and **Emacs**, both stock, with no plugins or
packages (the baseline the [for-vim-users](../for-vim-users/index.md) and
[for-emacs-users](../for-emacs-users/index.md) pages also assume). Then
**Helix** (the other "modern terminal editor written in Rust"), **Zed** (a
modern GUI editor, included as the "if you'd consider a GUI" comparison
point), and two
young terminal editors with the same "GUI conventions in a terminal" goal as
Vix: [**Fresh**](https://github.com/sinelaw/fresh) (Rust, GPL-3.0-or-later)
and [**ttt**](https://github.com/eugenioenko/ttt) (Go, MIT). Fresh and ttt
are marked from their own READMEs and manifests as read on 2026-11-10; both
move fast, so check them for anything that matters to you. See also
[for-emacs-users](../for-emacs-users/index.md),
[for-visual-studio-code-users](../for-visual-studio-code-users/index.md),
and [for-helix-users](../for-helix-users/index.md) for the prose version
of some of these same comparisons.

`✓` built in, no plugin needed. `-` not built in (a plugin may exist).
`~` partial — see the footnote.

| Feature | Vix | Vim | Emacs | Helix | Zed | Fresh | ttt |
| --- | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| Interface | TUI, GUI[^vix-gui] | TUI[^gvim] | TUI, GUI | TUI | GUI | TUI, GUI[^fresh-gui] | TUI |
| Modal editing, on by default | -[^keymap] | ✓ | - | ✓ | - | - | -[^ttt-modes] |
| Menu bar | ✓ | - | ✓ | - | ✓ | ✓ | ✓ |
| LSP (diagnostics, rename, go-to-def) | ✓ | - | ✓[^eglot] | ✓ | ✓ | ✓ | ✓ |
| Debugger (DAP) | ✓ | - | -[^gud] | ✓ | ✓ | - | - |
| Tree-sitter highlighting | ✓ | - | ~[^tree-sitter-emacs] | ✓ | ✓ | ✓ | -[^textmate] |
| Built-in fuzzy finder / palette | ✓ | - | ~[^mx] | ✓ | ✓ | ✓ | ✓ |
| Multiple cursors | ✓ | - | - | ✓ | ✓ | ✓ | ✓ |
| Git client (stage, commit, log, blame) | ✓ | - | ~[^vc] | ~[^helixgit] | ✓ | ~[^fresh-git] | ✓ |
| Jujutsu client | ✓ | - | - | - | - | - | - |
| Integrated terminal | ✓ | ✓ | ✓ | - | ✓ | ✓ | ✓ |
| Plugin/scripting system | ✓ (Rhai) | ✓ (Vimscript) | ✓ (Emacs Lisp) | ~[^steel] | ~[^zedext] | ✓ (TypeScript) | ✓ (Lua) |
| Persistent, branching undo | ✓ | ~[^undofile] | ~[^emacs-undo] | - | - | - | - |
| Org-mode (outline, TODO, agenda) | ✓ | - | ✓ | - | - | - | - |
| Database workbench | ✓ | - | ~[^sql-mode] | - | - | - | - |
| HTTP client for `.http` files | ✓ | - | - | - | - | - | - |
| Themes built in | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Interface in many languages | ✓ | ✓ | - | - | - | ✓ | - |
| Single static binary, no runtime | ✓ | ✓ | - | ✓ | - | ✓ | ✓ |
| Native remote/SSH project editing | ~[^vix-ssh] | ✓[^sshvim] | ✓[^tramp] | - | ✓ | ~[^fresh-ssh] | - |

[^vix-gui]: The desktop front ends (`vix-gpui`, `vix-egui` and five more)
    run the same editor in a native window; they are experimental. See
    [`gui/spec/index.md`](../../gui/spec/index.md).
[^gvim]: A GUI build (`gvim`) exists alongside the terminal one; the
    comparison above is against stock terminal Vim, matching the
    [for-vim-users](../for-vim-users/index.md) page's own baseline.
[^fresh-gui]: Fresh Desktop, a separate standalone build.
[^keymap]: Off by default (the **Apple** keymap), but a full **Vi** or
    **Spacemacs** (Vi-modal plus a `Space` leader) keymap is one menu
    choice away — see [keymaps](../keymaps/index.md).
[^ttt-modes]: Vim and Emacs editing are plugins from ttt's Plugins sidebar.
[^textmate]: ttt highlights with TextMate grammars (a Go port of VS Code's
    engine), not Tree-sitter.
[^eglot]: Eglot, Emacs's built-in LSP client since Emacs 29.
[^gud]: GUD drives `gdb` and other command-line debuggers, but Emacs has no
    built-in Debug Adapter Protocol client.
[^tree-sitter-emacs]: Built in since Emacs 29, but the grammars are installed
    separately.
[^mx]: `M-x` and `C-x p f` complete command and file names, and
    `fido-vertical-mode` narrows them, but there is no fuzzy palette out of
    the box.
[^vc]: VC commits, shows logs and annotates (blames) through one interface
    for many systems; hunk-by-hunk staging is Magit, a package.
[^emacs-undo]: Emacs never loses an undo state (undoing an undo is part of
    the history), but there is no tree browser or persistence without a
    package such as `vundo` or `undo-fu-session`.
[^sql-mode]: SQL mode runs `psql`, `mysql` or `sqlite3` in a console buffer:
    a query console, not a results grid or schema tree.
[^tramp]: TRAMP edits remote files over SSH and other protocols.
[^helixgit]: Helix shows a diff gutter and has some Git awareness, but by
    its own stated design does not aim to be a Git client — no stage/
    commit/log panel.
[^fresh-git]: Fresh's README lists a Git log, Git grep and a Git file
    finder, but no staging, committing or blame.
[^steel]: [Steel](https://github.com/mattwparas/steel), a Scheme-based
    scripting/plugin layer, is still under active development as of 2026,
    not yet a stable, documented plugin API.
[^zedext]: Zed's extensions add languages, themes, and debug adapters, not
    general-purpose editor scripting.
[^undofile]: Vim's `:set undofile` persists undo history across sessions,
    and its undo tree branches — the underlying data model is comparable.
    What Vix adds is a browsable **Undo Branch** UI with no config: Vim's
    branches exist but need a plugin (e.g. Gundo/undotree) to browse.
[^vix-ssh]: New in 2.0 and experimental: **File → Open Remote…** or
    `vix ssh://host/path` edits a folder on another machine with nothing
    installed there. See [remote editing](../remote-editing/index.md).
[^sshvim]: Not a feature of Vim itself — `ssh host -t vim file` runs Vim
    on the remote host over a plain terminal session, the way any TUI
    program can be used remotely.
[^fresh-ssh]: Fresh's README lists remote/SSH workspaces under its
    Orchestrator.

None of this is "Vix wins every row" — it doesn't, and a matrix this
compressed necessarily flattens nuance the linked prose pages restore. Read
it as: **Vix bundles into one binary what Helix and Zed also ship built in
(LSP/DAP/tree-sitter/pickers), Vim leaves to plugins and Emacs only partly
builds in**, plus a few
things none of the other six have at all (Org-mode, a Jujutsu client,
persistent undo with no setup, a bundled DB/HTTP/JWT/regex tool shelf) —
traded against real gaps of its own: remote/SSH editing that is still
experimental, no modal editing by default (opt-in instead), and a much
younger, smaller plugin ecosystem than Vim's or Zed's (Rhai scripts today; see
[`docs/scripting/index.md`](../scripting/index.md)).

## Where Vix is simplest

No plugin manager, no extension marketplace, no JavaScript/TypeScript
runtime to sandbox. What ships in the binary is everything there is —
smaller attack surface, nothing to configure before it works.

## Where Vix does more than stock Vim and Emacs

Vim and Emacs are both deeply extensible, and almost every row above can be
filled in with plugins or packages: LSP and DAP clients, a fuzzy finder,
multiple cursors, Magit, an HTTP client. Vix's difference is that these are
present, configured and documented on first launch, alongside a
**left-side file explorer**, a **right-side message drawer**, a **bottom
dock** for logs, output, test results and the DB workbench, 21 switchable
**themes**, and a **switchable UI language** in 15 languages — see
[`index.md`](../../index.md) for the full tour. What Vim and Emacs do
better: decades of ecosystem, Vimscript and Emacs Lisp reach far deeper
than Vix's Rhai scripts, and Emacs's Org mode and TRAMP are more mature.

## Easier to pick up than Vim or Emacs, for a newcomer

Vim (like Helix) defaults to modal editing, and Emacs to its own chords
(`C-x C-s` to save) and vocabulary (kill and yank for cut and paste), both
unfamiliar territory for most people's first day. Vix's default (**Apple**) keymap uses ordinary
`Ctrl+C`/`Ctrl+V`-style shortcuts and shows a menu bar — modal editing is a
menu choice away (**View → Keymap**) for whoever wants it, not the only
door in.

---

Vix™ and Vix IDE™ are trademarks.
