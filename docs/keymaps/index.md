# Keymaps

A **keymap** is a keyboard navigation style for moving through the editor, menus,
file explorer, and more. Vix™ ships ten keymaps — **Apple**, **VSCode macOS**,
**VSCode Windows**, **Emacs**, **Vi**, **Spacemacs**, **IntelliJ macOS**,
**IntelliJ Windows**, **Eclipse**, and **Sublime Text** — and exactly one is
active at a time.

**Status:** Shipped. The choice persists in the `keymap` setting (default
`apple`).

## Choosing a Keymap

Open **View → Keymap…** to pick the active keymap. The options are:

| Title       | Tooltip                  | Example: open file chooser | Example: quit |
| ----------- | ------------------------ | -------------------------- | ------------- |
| Apple       | Apple controls           | `Control-O`                | `Control-Q`   |
| VSCode macOS | VS Code (macOS) bindings | `Control-P` (Quick Open)   | `Control-Q`   |
| VSCode Windows | VS Code (Windows) bindings | `Control-P` (Quick Open) | `Control-Q` |
| Emacs       | Emacs chords             | `Control-X Control-F`      | `Control-X Control-C` |
| Vi         | Vi modes                | `:Ex`                      | `:q!`         |
| Spacemacs   | Vi modes + Space leader | `SPC f f`                  | `SPC q q`     |
| IntelliJ macOS | IntelliJ (macOS) | `Control-Shift-O` (Go to File) | `Control-Q` |
| IntelliJ Windows | IntelliJ (Windows) | `Control-Shift-N` (Go to File) | `Control-Q` |
| Eclipse     | Eclipse (Windows)        | `Control-Shift-R` (Open Resource) | `Control-Q` |

Your selection is saved to the `keymap` setting, so it persists across sessions.

## IntelliJ

Two keymaps mirror IntelliJ's defaults (macOS uses `Control` in place of
`Cmd`). Common bindings: `Ctrl+F`/`Ctrl+R` find/replace, `Ctrl+Shift+F`/`R`
in-project, `Ctrl+Shift+A` find action, `Ctrl+B` go to declaration, `Ctrl+D`
duplicate line, `Ctrl+/` comment, `Ctrl+Alt+L` reformat, `Ctrl+Alt+O` go to
symbol. The platforms differ on a few: **macOS** uses `Ctrl+O`/`Ctrl+Shift+O` (go
to class/file), `Ctrl+L` (go to line), `Ctrl+,` (settings); **Windows** uses
`Ctrl+N`/`Ctrl+Shift+N` (go to class/file), `Ctrl+G` (go to line), `Ctrl+Y`
(delete line). Editing chords (`Ctrl+Z/X/C/V/A`) are the editor's own.

## Eclipse

Mirrors Eclipse's Windows defaults: `Ctrl+Shift+R` open resource, `Ctrl+Shift+T`
open type, `Ctrl+O` quick outline, `Ctrl+L` go to line, `Ctrl+K`/`Ctrl+Shift+K`
find next/previous, `Ctrl+H` search, `Ctrl+D` delete line, `Ctrl+/` comment,
`Ctrl+Shift+F` format, `Ctrl+3` quick access (palette), `Ctrl+Y` redo, `Alt+/`
word completion, `F3` open declaration.

## Spacemacs

The **Spacemacs** keymap is the Vi modal model — the *same* Normal-mode
vocabulary as the Vi keymap (`hjkl`, `w`/`b`, `gg`/`G`, `dd`/`yy`/`p`, `u`,
`/` + `n`/`N`, `%`, `i`/`a`/`o` to insert, `:` command line) — plus a
**`Space` leader** in Normal mode
that opens menu-like command sequences. Press `Space`, then the keys for the
command; the status bar shows the pending sequence (`SPC …`).

| Sequence | Action                         |
| -------- | ------------------------------ |
| `SPC SPC`| Command palette                |
| `SPC f f`| Open file                      |
| `SPC f r`| Open recent                    |
| `SPC f s`| Save                           |
| `SPC f p`| Switch project                 |
| `SPC b n` / `b p` / `b d` | Next / previous / close buffer |
| `SPC p p` / `p f` / `p t` | Switch project / palette / file tree |
| `SPC g s` / `g g` / `g b` | Git changes / status / blame   |
| `SPC w /` / `w -` / `w d` / `w w` | Split vertical / horizontal / unsplit / focus other |
| `SPC s s` / `s p`         | Find / search workspace        |
| `SPC t n` / `t w`         | Toggle line numbers / whitespace |
| `SPC ;`  | Toggle comment                 |
| `SPC q q`| Quit                           |

`Esc` cancels a pending leader sequence; an unknown sequence is reported in the
status bar.

## What Each Keymap Does Today

The bindings below are exactly what Vix dispatches today. Menu mnemonics (`Alt+…`)
and the function keys (`F1`, `F3`, `F10`, `F12`) work in **every** keymap.

### Apple (default)

Modifier shortcuts in the style of macOS and Windows — for example, `Ctrl-C` for
Copy. Apple is not modal. See `../../keybindings/index.md` for the full list.

### VSCode macOS

VS Code's signature shortcuts (with `Ctrl` standing in for `Cmd`):

- `Ctrl+P` Quick Open (open file by name), `Ctrl+Shift+P` Command Palette.
- `Ctrl+Shift+O` Go to Symbol, `Ctrl+T` workspace symbol, `Ctrl+G` Go to Line.
- `Ctrl+B` toggle the sidebar, `Ctrl+Shift+E` focus the explorer, `Ctrl+J`
  toggle the bottom panel, `` Ctrl+` `` the terminal, `Ctrl+Shift+M` Problems.
- `Ctrl+\` split the editor; `Ctrl+]` / `Ctrl+Shift+\` jump to the matching
  bracket.
- The familiar editing chords: `Ctrl+S` save, `Ctrl+W` close, `Ctrl+F` find,
  `Ctrl+Shift+F` find in workspace, `Ctrl+/` comment, `Ctrl+R` replace,
  `Ctrl+Shift+K` delete line, `Ctrl+Shift+L` select all occurrences.

### Emacs

Chorded commands with a `Ctrl+X` prefix and `Meta` (Alt) bindings:

- `Ctrl+X Ctrl+F` open, `Ctrl+X Ctrl+S` save, `Ctrl+X Ctrl+C` quit,
  `Ctrl+X k` close, `Ctrl+X b` switch buffer.
- Windows: `Ctrl+X 2` / `Ctrl+X 3` split, `Ctrl+X 1` / `Ctrl+X 0` unsplit,
  `Ctrl+X o` other pane. The which-key popup lists chords while `C-x` pends.
- Motion: `Ctrl+F` / `Ctrl+B` (character), `Alt+F` / `Alt+B` (word),
  `Ctrl+N` / `Ctrl+P` (line), `Ctrl+A` / `Ctrl+E` (line ends),
  `Ctrl+V` / `Alt+V` (page), `Alt+<` / `Alt+>` (buffer ends).
- Kill ring: `Ctrl+W` kill, `Alt+W` save, `Ctrl+Y` yank, `Ctrl+K` kill line.
- `Alt+X` command palette (M-x), `Ctrl+T` / `Alt+T` transpose, `Ctrl+/` undo,
  `Ctrl+D` delete, `Ctrl+S` find, `Ctrl+G` cancel.

### Vi

Modal editing:

- **Normal mode:** `h` / `j` / `k` / `l`, `w` / `b` words, `0` / `^` / `$`,
  `gg` / `G` file ends, `x`, `dd` / `yy` / `p` cut/copy/paste line, `u` undo,
  `/` + `n` / `N` search, `%` matching bracket, and `i` / `a` / `I` / `A` /
  `o` / `O` (which enter Insert mode).
- **Insert mode:** `Esc` returns to Normal mode.
- **Command line:** `:w`, `:q`, `:q!`, `:wq` / `:x`, `:N` (go to line),
  `:e [path]`, `:Ex`.

The status bar shows `-- NORMAL --`, `-- INSERT --`, or the `:` command line.

### Sublime Text

Sublime's signature shortcuts (with `Ctrl` standing in for `Cmd`):

- `Ctrl+P` Goto Anything (file finder), `Ctrl+Shift+P` Command Palette,
  `Ctrl+R` Goto Symbol, `Ctrl+G` Goto Line.
- `Ctrl+D` select all occurrences of the word (Sublime's add-next-occurrence,
  all at once), `Ctrl+L` expand selection to line, `Ctrl+J` join lines,
  `Ctrl+M` jump to the matching bracket.
- `Ctrl+Shift+D` duplicate line, `Ctrl+Shift+K` delete line, `Ctrl+/` comment.
- `Ctrl+F` find, `Ctrl+H` replace, `Ctrl+Shift+F` find in files.
- `Ctrl+B` build (runs the test suite), `` Ctrl+` `` the console (terminal).
- `Ctrl+S` / `Ctrl+Shift+S` / `Ctrl+W` / `Ctrl+Shift+T` save / save as /
  close / reopen closed.

## Not Yet Built

The following are described in the broader keymap philosophy but are **not**
implemented:

- Vi counts (`3w`, `2dd`), text objects (`diw`), registers, and visual mode.
- Emacs `Ctrl+Space` mark/region commands (use `Shift`+motion to select).

## Background

The keymaps treat the keyboard differently: as a way to trigger system actions
(Apple, and the VS Code variant), as a language for text manipulation (Vi), or
as a layered set of chords for executing functions (Emacs).

| Feature      | Apple (macOS)              | Vi                            | Emacs                      |
| ------------ | -------------------------- | ------------------------------ | -------------------------- |
| Logic        | System commands & UI focus | Modal "language" (verb + noun) | Layered modifiers & chords |
| Primary keys | Command (⌘) + Tab          | Home row (`h`, `j`, `k`, `l`)  | Control (⌃) + Meta (⌥)     |
| Philosophy   | Universal accessibility    | Speed and home-row efficiency  | Everything is a function   |

---

Vix™ and Vix IDE™ are trademarks.
