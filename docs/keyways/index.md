# Keyways

A **keyway** is a keyboard navigation style for moving through the editor, menus,
file explorer, and more. Vix ships three keyways — **Apple**, **Emacs**, and
**Vim** — and exactly one is active at a time.

**Status:** Shipped. The choice persists in the `keyway` setting (default
`apple`).

## Choosing a Keyway

Open **View → Keyway…** to pick the active keyway. The three options are:

| Title | Tooltip        | Example: open file chooser | Example: quit |
| ----- | -------------- | -------------------------- | ------------- |
| Apple | Apple controls | `Control-O`                | `Control-Q`   |
| Emacs | Emacs chords   | `Control-X Control-F`      | `Control-X Control-C` |
| Vim   | Vim modes      | `:Ex`                      | `:q!`         |

Your selection is saved to the `keyway` setting, so it persists across sessions.

## What Each Keyway Does Today

The bindings below are exactly what Vix dispatches today. Menu mnemonics (`Alt+…`)
and the function keys (`F1`, `F3`, `F10`, `F12`) work in **every** keyway.

### Apple (default)

Modifier shortcuts in the style of macOS and Windows — for example, `Ctrl-C` for
Copy. Apple is not modal. See `../docs/keybindings.md` for the full list.

### Emacs

Chorded commands with a `Ctrl+X` prefix:

- `Ctrl+X Ctrl+F` open, `Ctrl+X Ctrl+S` save, `Ctrl+X Ctrl+C` quit,
  `Ctrl+X k` close.
- Motion: `Ctrl+F` / `Ctrl+B` (character), `Ctrl+N` / `Ctrl+P` (line),
  `Ctrl+A` / `Ctrl+E` (line ends), `Ctrl+V` (page down).
- `Ctrl+D` delete, `Ctrl+S` find, `Ctrl+G` cancel.

### Vim

Modal editing:

- **Normal mode:** `h` / `j` / `k` / `l`, `0` / `$`, `x`, and `i` / `a` / `o` /
  `O` (which enter Insert mode).
- **Insert mode:** `Esc` returns to Normal mode.
- **Command line:** `:w`, `:q`, `:q!`, `:wq` / `:x`, `:Ex`.

The status bar shows `-- NORMAL --`, `-- INSERT --`, or the `:` command line.

## Not Yet Built

The following are described in the broader keyway philosophy but are **not**
implemented:

- Vim counts and operators (`3w`, `dd`, `gg` / `G`).
- Emacs `Meta` / `M-x` — `Alt` is reserved for menu mnemonics.
- Registers and visual mode.

## Background

The three keyways treat the keyboard differently: as a way to trigger system
actions (Apple), as a language for text manipulation (Vim), or as a layered set
of chords for executing functions (Emacs).

| Feature      | Apple (macOS)              | Vim                            | Emacs                      |
| ------------ | -------------------------- | ------------------------------ | -------------------------- |
| Logic        | System commands & UI focus | Modal "language" (verb + noun) | Layered modifiers & chords |
| Primary keys | Command (⌘) + Tab          | Home row (`h`, `j`, `k`, `l`)  | Control (⌃) + Meta (⌥)     |
| Philosophy   | Universal accessibility    | Speed and home-row efficiency  | Everything is a function   |
