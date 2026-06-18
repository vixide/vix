# Keymaps

A **keymap** is a keyboard navigation style for moving through the editor, menus,
file explorer, and more. Vix ships four keymaps — **Apple**, **macOS VSCode**,
**Emacs**, and **Vim** — and exactly one is active at a time.

**Status:** Shipped. The choice persists in the `keymap` setting (default
`apple`).

## Choosing a Keymap

Open **View → Keymap…** to pick the active keymap. The options are:

| Title       | Tooltip                  | Example: open file chooser | Example: quit |
| ----------- | ------------------------ | -------------------------- | ------------- |
| Apple       | Apple controls           | `Control-O`                | `Control-Q`   |
| macOS VSCode | VS Code (macOS) bindings | `Control-P` (Quick Open)   | `Control-Q`   |
| Emacs       | Emacs chords             | `Control-X Control-F`      | `Control-X Control-C` |
| Vim         | Vim modes                | `:Ex`                      | `:q!`         |

Your selection is saved to the `keymap` setting, so it persists across sessions.

## What Each Keymap Does Today

The bindings below are exactly what Vix dispatches today. Menu mnemonics (`Alt+…`)
and the function keys (`F1`, `F3`, `F10`, `F12`) work in **every** keymap.

### Apple (default)

Modifier shortcuts in the style of macOS and Windows — for example, `Ctrl-C` for
Copy. Apple is not modal. See `../../keybindings/index.md` for the full list.

### macOS VSCode

VS Code's signature shortcuts (with `Ctrl` standing in for `Cmd`):

- `Ctrl+P` Quick Open (open file by name), `Ctrl+Shift+P` Command Palette.
- `Ctrl+Shift+O` Go to Symbol, `Ctrl+G` Go to Line.
- `Ctrl+B` toggle the sidebar, `Ctrl+Shift+E` focus the explorer.
- The familiar editing chords: `Ctrl+S` save, `Ctrl+W` close, `Ctrl+F` find,
  `Ctrl+Shift+F` find in workspace, `Ctrl+/` comment, `Ctrl+R` replace.

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

The following are described in the broader keymap philosophy but are **not**
implemented:

- Vim counts and operators (`3w`, `dd`, `gg` / `G`).
- Emacs `Meta` / `M-x` — `Alt` is reserved for menu mnemonics.
- Registers and visual mode.

## Background

The keymaps treat the keyboard differently: as a way to trigger system actions
(Apple, and the VS Code variant), as a language for text manipulation (Vim), or
as a layered set of chords for executing functions (Emacs).

| Feature      | Apple (macOS)              | Vim                            | Emacs                      |
| ------------ | -------------------------- | ------------------------------ | -------------------------- |
| Logic        | System commands & UI focus | Modal "language" (verb + noun) | Layered modifiers & chords |
| Primary keys | Command (⌘) + Tab          | Home row (`h`, `j`, `k`, `l`)  | Control (⌃) + Meta (⌥)     |
| Philosophy   | Universal accessibility    | Speed and home-row efficiency  | Everything is a function   |
