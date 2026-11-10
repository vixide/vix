# Vix™ for Zed users

You picked a fast, native editor written in Rust that ships LSP, Tree-sitter,
a debugger, a terminal, Git, AI and real-time collaboration with no plugins.
Vix shares most of that list and the language it's written in. The big
difference is where it runs: Vix is a terminal program first, and Zed is a
GPU-rendered desktop app. This page covers what carries over and what
doesn't.

## The one-minute pitch

Vix is a terminal IDE written in Rust, shipped as one binary with no runtime.
LSP (diagnostics, rename, references, code actions), a DAP debugger,
Tree-sitter highlighting, multiple cursors, a command palette, a Git client
and an integrated terminal are all built in, as in Zed. Because it runs in a
terminal, it also runs over SSH, inside tmux, in a container, or on a machine
with no display.

```sh
brew install vixide/homebrew-tap/vix-tui   # or: cargo install vix-tui
vix .                                      # open the current directory
```

## What carries over

- **Zero-config language support.** Open a file and you get diagnostics,
  go-to-definition and highlighting. You don't install an extension per
  language, only the language server itself.
- **Your Zed shortcuts.** The **Zed** keymap (**View → Keymap → Zed**) binds
  Zed's default keys, with `Ctrl` standing in for `Cmd`: `Ctrl+P` file
  finder, `Ctrl+Shift+P` command palette, `Ctrl+Shift+O` outline, `Ctrl+T`
  project symbols, `Ctrl+.` code actions, `Ctrl+Shift+G` Git panel and more.
  See [`docs/keymaps/index.md`](../keymaps/index.md#zed).
- **Multiple cursors.** Select all occurrences, column selection and manual
  cursor placement are built in. See
  [`docs/multiple-cursors/index.md`](../multiple-cursors/index.md).
- **Git, terminal and debugger in the box.** Vix stages hunks, commits, and
  shows logs and blame. Its integrated terminal opens with `` Ctrl+` `` in
  most keymaps, and its debugger lives under the **Run** menu.
- **AI assistance.** The AI chat panel talks to a CLI (Claude by default) or
  straight to a provider's API. Inline suggestions (ghost text) are optional.
  See [`docs/agent-panel/index.md`](../agent-panel/index.md) and
  [`docs/ghost-text/index.md`](../ghost-text/index.md).
- **Editing together.** Vix's **Cowork** feature lets several people edit
  the same files live and see each other's cursors. It is peer to peer: one
  person hosts and the others join with an invite, with no account or
  server. See [`docs/cowork/index.md`](../cowork/index.md).
- **A Vim mode, if you use Zed's.** The **Vi** keymap (and **Spacemacs**,
  which adds a `Space` leader) is one menu choice away. Like Zed's Vim mode,
  it is off by default. See [`docs/keymaps/index.md`](../keymaps/index.md).

## What's different

- **A terminal, not a window.** Vix draws with characters, so it has no
  smooth scrolling, proportional fonts, inline images or 120 fps animation.
  In return it works anywhere a terminal does. If you want a window,
  the experimental desktop front ends (one is built on GPUI, the same UI
  framework Zed uses) run the same editor natively. See
  [`gui/spec/index.md`](../../gui/spec/index.md).
- **Remote editing works differently.** Zed runs a headless server on the
  remote machine. Vix can simply be run on that machine over `ssh`, or
  (new in 2.0, experimental) open a remote folder with **File → Open
  Remote…** without installing anything there. See
  [`docs/remote-editing/index.md`](../remote-editing/index.md).
- **Scripting, not extensions.** Zed extensions are WebAssembly modules that
  add languages, themes and debug adapters. Vix has Rhai scripts for palette
  commands and key bindings
  ([`docs/scripting/index.md`](../scripting/index.md)), and no extension
  marketplace.
- **More bundled tools.** Vix also ships Org mode, a Jujutsu client, a
  database workbench, an HTTP client for `.http` files, and persistent,
  branching undo. Zed leaves those out.

## Where Zed still wins

- **Rendering and feel.** GPU rendering, proportional fonts, inline image
  previews and smooth animation are things a terminal cannot do.
- **Collaboration as a service.** Zed's channels, shared screens and voice
  calls come with its hosted service. Vix's Cowork is deliberately smaller:
  shared editing and cursors, with no service in between.
- **Ecosystem.** Zed has a larger community, an extension gallery and a
  bigger team behind it. Vix's plugin story is younger and smaller.
- **AI depth.** Zed's agent panel and edit predictions are built around its
  own hosted models and agent features. Vix's AI is a thinner layer over
  whatever CLI or provider you configure.

## An honest experiment

Give it one afternoon on a real task:

1. Install Vix and open a project: `vix .`
2. Pick the **Zed** keymap (**View → Keymap…**), so your Zed reflexes
   work, or **Vi** if you use Zed's Vim mode.
3. Do what you'd do in Zed: jump to a definition, rename a symbol, stage a
   hunk from **Git → Changes…**, run a test in the integrated terminal.
4. Then try what Zed can't: `ssh` into a server and run `vix` there, or open
   an `.http` file and send a request.

If GPU rendering and Zed's hosted collaboration are why you use it, Vix
won't replace it, and that's a fair conclusion. If what you want is the same
built-in IDE in a terminal, so it goes everywhere your shell goes, it's
worth the afternoon.

## See also

- [`docs/compare/index.md`](../compare/index.md) — the full feature-parity
  matrix, including Zed.
- [`docs/keymaps/index.md`](../keymaps/index.md) — every keymap, every
  binding.
- [`index.md`](../../index.md) — the full feature tour.

---

Vix™ and Vix IDE™ are trademarks.
