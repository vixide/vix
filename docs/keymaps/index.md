# Keymaps

A **keymap** is a keyboard navigation style for moving through the editor, menus,
file explorer, and more. Vix™ ships fourteen keymaps — **Apple**, **VSCode macOS**,
**VSCode Windows**, **Emacs**, **Vi**, **Spacemacs**, **IntelliJ macOS**,
**IntelliJ Windows**, **Eclipse**, **Sublime**, **Zed**, **Fresh**, **ttt**, and **Helix** —
and exactly one is
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
| Sublime     | Sublime bindings         | `Control-P` (Goto Anything) | — |
| Zed         | Zed bindings             | `Control-P` (file finder)  | `Control-Q`   |
| Fresh       | Fresh bindings           | `Control-P` (Quick Open)   | `Control-Q`   |
| ttt         | ttt bindings             | `Control-K P` (Quick Open) | `Control-Q`   |
| Helix       | Helix modes              | `Space f`                  | `:q!`         |

Your selection is saved to the `keymap` setting, so it persists across sessions.

On macOS the `Command` key drives whatever `Control` drives, in every keymap —
Vix folds the modifier before dispatch. The terminal has to forward the
shortcut for that to happen; see `../keybindings/index.md`.

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
Copy, and `Ctrl-D` for forward delete (the character to the right of the
cursor), as in every macOS text field. Apple is not modal. See
`../keybindings/index.md` for the full list.

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

The status bar shows `-- NORMAL --`, `-- INSERT --`, or the `:` command line,
followed by any count or operator you've typed so far (`-- NORMAL -- 2d3`).

### Sublime

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

### Zed

Zed's default Linux shortcuts (on macOS Zed uses `Cmd` where Linux uses
`Ctrl`; here `Ctrl` stands in for both):

- `Ctrl+P` (or `Ctrl+E`) file finder, `Ctrl+Shift+P` Command Palette.
- `Ctrl+Shift+O` outline (symbols in this file), `Ctrl+T` project symbols,
  `Ctrl+G` go to line.
- `Ctrl+D` select the next occurrence, `Ctrl+Shift+L` select all occurrences,
  `Ctrl+L` select line, `Ctrl+M` jump to the enclosing bracket.
- `Ctrl+Shift+D` duplicate line, `Ctrl+Shift+K` delete line, `Ctrl+/` comment.
- `Ctrl+F` find, `Ctrl+H` replace, `Ctrl+Shift+F` project search.
- `Ctrl+.` code actions, `Ctrl+Shift+M` diagnostics.
- `Ctrl+B` left dock (the file explorer), `Ctrl+Shift+E` focus the explorer,
  `Ctrl+J` bottom dock, `` Ctrl+` `` terminal, `Ctrl+Shift+G` Git panel,
  `Ctrl+,` settings.
- `Ctrl+N` / `Ctrl+S` / `Ctrl+Shift+S` / `Ctrl+W` / `Ctrl+Shift+W` /
  `Ctrl+Shift+T` / `Ctrl+Q` new / save / save as / close / close all /
  reopen closed / quit.

- `Ctrl+K` then: `Ctrl+L` toggle fold, `Ctrl+0` fold all, `Ctrl+J` unfold
  all, `Ctrl+C` comment, `Ctrl+S` keyboard shortcuts, `Ctrl+W` close all,
  `Ctrl+Z` soft wrap, `Ctrl+O` open a path, `V` Markdown preview.

Zed's other `Ctrl+K` chords (the theme picker, pane splits with the arrow
keys, `Ctrl+K Ctrl+[`/`Ctrl+]` recursive folds) are not bound yet.

### Fresh

The default keymap of [Fresh](https://github.com/sinelaw/fresh), a terminal
editor with GUI-style keys:

- `Ctrl+P` Quick Open (the file finder), `Ctrl+O` open a file by path,
  `Ctrl+N` new, `Ctrl+S` save, `Ctrl+Q` quit.
- `Ctrl+E` focus the file explorer, `Ctrl+B` toggle it.
- `Ctrl+F` find, `Ctrl+R` replace, `Ctrl+Shift+N` find previous, `Ctrl+G`
  go to line, `Ctrl+]` jump to the matching bracket.
- `Ctrl+D` add a cursor at the next match, `Ctrl+L` select line, `Ctrl+W`
  select word, `Ctrl+T` transpose characters, `Ctrl+/` comment.
- `Ctrl+K` cut to the end of the line, `Ctrl+H` delete the word before the
  cursor.
- `Alt+Up` / `Alt+Down` move the line, `Alt+Left` / `Alt+Right` go back /
  forward, `Alt+.` code actions, `Alt+/` project search.

Fresh's Quick Open also runs commands after a `>`; in Vix, use the command
palette.

### ttt

The default keybindings of [ttt](https://github.com/eugenioenko/ttt), a
terminal editor with GUI-style keys. ttt groups many commands under two-key
chords: press `Ctrl+K` or `Ctrl+L`, release, then press the letter.

- `Ctrl+P` command palette, `Ctrl+N` new, `Ctrl+S` save, `Ctrl+W` close,
  `Ctrl+Q` quit.
- `Ctrl+B` toggle the sidebar (file explorer), `Ctrl+0` focus it, `Ctrl+T`
  terminal.
- `Ctrl+F` find, `Ctrl+R` replace, `Ctrl+G` go to line, `Ctrl+D` select the
  next match, `Ctrl+/` comment, `Ctrl+U` complete, `Ctrl+.` code actions.
- `Alt+.` / `Alt+,` next / previous tab, `Alt+Up` / `Alt+Down` move the line.
- `Ctrl+K` then: `P` Quick Open, `S` save as, `E` explorer, `F` project
  search, `R` project replace, `C` Git changes, `T` terminal, `I` hover,
  `L` select all matches, `K` delete line, `J` join lines, `O` sort lines,
  `M` matching bracket, `B` bottom panel, `Y` keyboard shortcuts, `,`
  settings, `[` toggle fold, `0` fold all, `9` unfold all.
- `Ctrl+L` (language) then: `F` format, `R` references, `I` go to
  implementation, `T` go to type definition, `O` organize imports.

Their `Alt`+letter keys are left out, because in every Vix keymap
`Alt`+letter opens a menu. An unbound second key ends a chord and is not
typed. While a chord is
pending, the status bar shows `C-k-` (or `C-l-`) and the which-key popup lists
the second keys.

### Helix

Selection-first modal editing, as in Helix: in Normal mode a motion
*selects*, then an action acts on the selection. With nothing selected, an
action acts on the character under the cursor.

- **Select:** `w` / `b` / `e` (and `W` / `B` / `E` over runs of
  non-whitespace) select to the next word start / previous word start / word
  end; `f` / `t` / `F` / `T` + a character select to it on the line; `x`
  selects the line (press again to add the next line); `%` selects
  everything. `h` / `j` / `k` / `l` move and drop the selection.
- **Act:** `d` delete, `c` change (delete, then Insert mode), `y` yank,
  `p` / `P` paste after / before, `r` + a character replace each selected
  character, `~` switch case, `>` / `<` indent / outdent, `J` join lines,
  `u` / `U` undo / redo.
- **Selections:** `v` toggles Select mode, where motions extend the
  selection; `;` collapses it; `,` drops extra cursors; `C` adds a cursor
  on the line below; `s` selects regex matches inside the selection; `*`
  jumps to the next occurrence of the selection.
- **Several selections:** with more than one selection (after `C` or `s`),
  the motions (`w` / `b` / `e`, `W` / `B` / `E`, `f` / `t` / `F` / `T`,
  `x`, `h` / `j` / `k` / `l`) move every one, and `d`, `c` (then typing at
  every cursor), `r`, `~`, `>` / `<`, `p` / `P` and `.` act on every one,
  each as one undo step. `y` copies them all into the one register, joined
  by newlines in buffer order; pasting into the same number of selections
  puts one line back at each. The goto keys, `J`, `I` / `A`, `o` / `O`, `*`
  and search still act on the primary selection only.
- **Insert:** `i` / `a` before / after the selection, `I` / `A` at the line
  start / end, `o` / `O` a new line below / above; `Esc` returns to Normal.
- **Counts:** a number before a command repeats it: `3w` selects the third
  word (`v3w` everything through it), `2x` two lines, `5j` moves down five,
  `3fx` finds the third `x`, `2p` pastes twice, `10gg` goes to line 10.
- **Repeat:** `.` repeats the last change on the current selection: `d`,
  `c` with what you typed, `i` / `a` with what you typed, `r`, `~`, `>` /
  `<`, and `p` / `P`. `3.` repeats it three times.
- **Goto (`g`):** `gg` / `ge` file start / end, `gh` / `gl` / `gs` line start
  / end / first non-blank, `gd` definition, `gD` declaration, `gy` type
  definition, `gi` implementation, `gr` references, `gn` / `gp` next /
  previous buffer; `mm` matching bracket.
- **Space leader:** `Space f` file picker, `Space b` buffers, `Space s` /
  `Space S` symbols / workspace symbols, `Space /` global search, `Space k`
  hover, `Space r` rename, `Space a` code action, `Space d` diagnostics,
  `Space p` / `Space P` / `Space y` paste / yank, `Space ?` command palette.
  The which-key popup lists the keys after `g`, `m`, or `Space`.
- **Command line and search:** `:w`, `:q`, `:q!`, `:wq` / `:x`, `:N`, plus
  Helix's `:write`, `:quit`, `:write-quit`, and `:o path` / `:open path`
  (bare `:o` opens the file picker); `/` and `?` find, `n` / `N` next /
  previous.

The status bar shows `-- NORMAL --`, `-- INSERT --`, `-- SELECT --`, or the
`:` command line.

Not yet: named registers (yank and paste use the one clipboard, so several
selections share one register value); match mode beyond `mm`;
view mode (`z`), window mode (`Ctrl+w`), the jump list, macros, `Alt-.`,
and jump labels; reverse search with `?`; and the block cursor.

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
