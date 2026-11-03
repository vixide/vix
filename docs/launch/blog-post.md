# DRAFT, not posted: Vix 2.0

> Draft. Not published anywhere. Replace the `[RELEASE DATE]` and `[LINK]`
> placeholders and re-check every claim against the released changelog first.

## Vix 2.0: edit anywhere, edit together

Vix is a keyboard-friendly terminal text editor written in Rust. It starts with
the shortcuts every other app uses (Ctrl+S saves, Ctrl+P opens the command
palette) and has a menu bar, so you can find things instead of memorizing them.
Vi, Emacs, Spacemacs, VS Code and other keymaps are one setting away. It also brings the
parts of an IDE you would otherwise leave the terminal for: language servers, a
debugger, Git, a database workbench, Org mode.

Vix 2.0 was released on [RELEASE DATE]. This release is about leaving your own
machine and your own chair.

### Open a folder on another machine

```sh
vix ssh://me@build-box/srv/app
```

or **File → Open Remote…**. Nothing needs installing on the other side. Search,
Git and the terminal run there, and once you trust the workspace so do the
language servers, the debugger and tasks. Docker and Podman containers and dev
containers open the same way. This is **experimental**: Kubernetes and WSL are
built but have only been tested against a stand-in so far.

### Edit the same file together

Cowork lets several people edit the same files in real time: single-use
invites, the host approves each guest, everyone's cursor in their own color,
conflict-free merging, a session chat, and a terminal shared read-only by
default. It is encrypted and has no server in between. It is an opt-in build
(`cargo install vix-tui --features cowork`), not part of the prebuilt releases,
and everyone in a session needs it.

Alongside it, the IM menu has a local Notes conversation, IRC, `path:line`
references that open on Enter, and threads anchored to lines of your files
that you can share through Git. (Matrix is planned, not built.)

### IDE conveniences

Local History with diff and restore, Search Everywhere, a Refactor menu,
Organize Imports, run configurations, Peek Definition, CodeLens, postfix
completion, an undo tree, multiple-caret depth, an interactive Git graph and
rebase, and a debugger with conditions, logpoints and exception filters.

### Big files, small footprint

The Large File Viewer opens multi-gigabyte files read-only without loading
them. On an Apple M4 Max with a warm cache, a 4 GiB log reached its first
screen in under a millisecond and was fully indexed in about half a second,
using under 5 MiB. Everyday editing is lighter too: a 100 MB log takes 126 MiB
instead of 1.1 GiB, and an idle Vix uses no CPU.

### You decide about AI

`ai = "off"` hides the AI menu and refuses every AI request. `vix --mcp` gives
an agent a read-only view of your workspace. Inline completion is off by
default.

### Upgrading

The `vix` command and your settings files keep working. 2.0 is a major version
because the `vix-*` library crates made breaking API changes (typed errors,
`#[non_exhaustive]` on growing types) so later features can be non-breaking.
The details are in the release notes ([LINK]).

### Try it

```sh
brew install vixide/homebrew-tap/vix-tui
# or: npm install -g @vixide/vix
# or: cargo install vix-tui
```

Then run `vix`, press Ctrl+P for the palette and F1 for every shortcut.

Source, docs and the full changelog: [LINK]. Vix is available under your choice
of Apache-2.0, BSD-3-Clause, MIT, GPL-2.0 or GPL-3.0. Bug reports and feature
requests are welcome.
