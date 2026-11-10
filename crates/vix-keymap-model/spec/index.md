# Keymaps

The available keyboard navigation styles ("keymaps"); the View → Keymap
submenu. A *keymap* is a whole-keyboard philosophy for driving Vix — the editor, the
menus, and the file explorer. Exactly one keymap is active at a time, and it
decides how each raw key event is interpreted before the focused pane ever sees
it. Switching keymaps changes the meaning of keys, not the available actions: the
same commands (open, save, find, move the cursor) stay reachable; only the keys
that trigger them differ.

Vix ships fourteen keymaps. **Apple** is the default and matches Vix's own bindings.

## The keymaps

| Keymap | Id | Philosophy |
| ------ | -- | ---------- |
| Apple | `apple` | Modifier-key shortcuts (the default), e.g. `Ctrl+O` to open, `Ctrl+S` to save, `Ctrl+D` forward delete (the macOS text-field binding: it removes the character to the right of the cursor, exactly like the `Delete` key). |
| VSCode macOS | `vscode-macos` | VS Code's signature shortcuts, with `Ctrl` standing in for `Cmd` — `Ctrl+P` Quick Open, `Ctrl+Shift+P` Command Palette, `Ctrl+G` Go to Line. |
| VSCode Windows | `vscode-windows` | VS Code (Windows) shortcuts; the same `Ctrl`-based bindings as VSCode macOS in the terminal. |
| Emacs | `emacs` | Layered `Ctrl` chords and a `Ctrl+X` prefix, e.g. `Ctrl+X Ctrl+F` to open. |
| Vi | `vi` | Modal editing: a Normal mode for motions and commands, plus an Insert mode and a `:` command line. |
| Spacemacs | `spacemacs` | Vi modal editing plus a `Space` leader for menu-like command sequences (e.g. `SPC f f` find file). |
| IntelliJ macOS | `intellij-macos` | IntelliJ (macOS) defaults, `Ctrl` for `Cmd` — `Ctrl+Shift+O` Go to File, `Ctrl+B` Go to Declaration, `Ctrl+Alt+L` Reformat. |
| IntelliJ Windows | `intellij-windows` | IntelliJ (Windows) defaults — `Ctrl+Shift+N` Go to File, `Ctrl+G` Go to Line, `Ctrl+Y` delete line. |
| Eclipse | `eclipse` | Eclipse (Windows) defaults — `Ctrl+Shift+R` Open Resource, `Ctrl+Shift+T` Open Type, `Ctrl+3` Quick Access, `F3` Open Declaration. |
| Sublime | `sublime` | Sublime defaults, `Ctrl` for `Cmd` — `Ctrl+P` Goto Anything, `Ctrl+Shift+P` Command Palette, `Ctrl+R` Goto Symbol, `Ctrl+J` Join Lines, `Ctrl+Shift+D` Duplicate. |
| Zed | `zed` | Zed's Linux defaults, `Ctrl` for `Cmd` — `Ctrl+P` file finder, `Ctrl+Shift+P` Command Palette, `Ctrl+Shift+O` outline, `Ctrl+T` project symbols, `Ctrl+.` code actions, `Ctrl+Shift+G` Git panel, `Ctrl+D` select next occurrence. |
| Fresh | `fresh` | [Fresh](https://github.com/sinelaw/fresh)'s default keymap — `Ctrl+P` Quick Open, `Ctrl+E` focus the file explorer, `Ctrl+B` toggle it, `Ctrl+G` go to line, `Ctrl+]` matching bracket, `Ctrl+T` transpose. |
| ttt | `ttt` | [ttt](https://github.com/eugenioenko/ttt)'s defaults — `Ctrl+P` command palette, `Ctrl+T` terminal, and two-key `Ctrl+K …` (`Ctrl+K P` files, `Ctrl+K S` save as, `Ctrl+K F` project search) and `Ctrl+L …` language chords (`Ctrl+L F` format, `Ctrl+L R` references). |
| Helix | `helix` | Selection-first modal editing: in Normal mode a motion selects (`w`, `x`, `%`) and an action acts on the selection (`d`, `c`, `y`); `v` Select mode, `g` goto, `Space` leader, `:` command line. |

Each keymap gets first chance to consume a key. Apple and VS Code dispatch their
shortcuts directly; the others (Emacs, Vi, Spacemacs, both IntelliJ, Eclipse, Sublime, Zed, Fresh, ttt) try
their own handling and then fall back to a **shared** layer (menu-bar mnemonics
like `Alt+F` and function keys like `F10`) before the focused pane handles the
key. A mnemonic toggles its menu: pressing the open menu's own letter closes the
dropdown, and another menu's letter switches to it, whether or not a dropdown is
already open. The IntelliJ, Eclipse, Sublime, Zed, Fresh, and ttt keymaps let editing chords
(`Ctrl+Z/X/C/V/A`) fall through to the editor widget.

## Choosing a keymap

The keymap is selected from the **View → Keymap** submenu in the top menu bar.
The submenu lists the fourteen keymaps by their proper names (not translated), in
model order, and is kept in sync with the keymap model by a test.

| Item | Action |
| ---- | ------ |
| Apple | `view.keymap:apple` |
| VSCode macOS | `view.keymap:vscode-macos` |
| VSCode Windows | `view.keymap:vscode-windows` |
| Emacs | `view.keymap:emacs` |
| Vi | `view.keymap:vi` |
| Spacemacs | `view.keymap:spacemacs` |
| IntelliJ macOS | `view.keymap:intellij-macos` |
| IntelliJ Windows | `view.keymap:intellij-windows` |
| Eclipse | `view.keymap:eclipse` |
| Sublime | `view.keymap:sublime` |
| Zed | `view.keymap:zed` |
| Fresh | `view.keymap:fresh` |
| ttt | `view.keymap:ttt` |
| Helix | `view.keymap:helix` |

Choosing an item dispatches `view.keymap:<id>`. The host:

1. Looks up the id in the keymap model; an unknown id is ignored.
2. Persists the choice in `settings.keymap` (default `"apple"`).
3. Resets per-keymap session state so the new keymap starts clean.
4. Shows a status-bar confirmation.

Because the choice lives in settings, it survives across sessions.

### Reset on switch

Switching keymaps clears any in-progress modal state so a freshly chosen keymap
never inherits a stale mode:

- Vi begins in **Normal** mode (Insert mode off, no open `:` command line).
- Spacemacs begins in **Normal** mode with no pending `Space` leader.
- Helix begins in **Normal** mode (not Select mode) with no pending prefix.
- The Emacs `Ctrl+X` chord prefix is cleared.

## Emacs chords

In the Emacs keymap, `Ctrl`-key chords drive motions and commands, and `Ctrl+X`
and `Ctrl+C` act as prefixes whose next key completes a two-key chord. While a
prefix is pending, the status bar shows the mode indicator (`C-x-` or `C-c-`).

| Chord | Action |
| ----- | ------ |
| `Ctrl+F` / `Ctrl+B` | Move cursor right / left |
| `Ctrl+N` / `Ctrl+P` | Move cursor down / up |
| `Ctrl+A` / `Ctrl+E` | Line start / line end |
| `Ctrl+V` / `Alt+V` | Page down / page up |
| `Ctrl+D` | Delete forward |
| `Ctrl+S` | Find |
| `Ctrl+G` | Cancel (status message) |
| `Ctrl+W` / `Alt+W` / `Ctrl+Y` | Kill region (cut) / kill-ring-save (copy) / yank (paste) |
| `Ctrl+K` | Kill line (cut the whole line) |
| `Ctrl+T` / `Alt+T` | Transpose characters / words |
| `Ctrl+/` | Undo |
| `Alt+X` | Command palette (M-x) |
| `Alt+F` / `Alt+B` | Word forward / backward |
| `Alt+<` / `Alt+>` | Buffer start / end |
| `Ctrl+X Ctrl+F` | Open file |
| `Ctrl+X Ctrl+S` | Save file |
| `Ctrl+X Ctrl+C` | Quit |
| `Ctrl+X K` | Close buffer |
| `Ctrl+X B` (or `Ctrl+X Ctrl+B`) | Switch buffer (palette `#` mode) |
| `Ctrl+X O` | Focus the other pane |
| `Ctrl+X 2` / `Ctrl+X 3` | Split horizontal / vertical |
| `Ctrl+X 1` / `Ctrl+X 0` | Unsplit |
| `Ctrl+C Ctrl+T` | Org: cycle the headline's TODO state |
| `Ctrl+U Ctrl+C Ctrl+T` | Org: mark DONE with a closing note + `CLOSED:` stamp |
| `Ctrl+C Ctrl+C` | Org: context action (toggle checkbox / update statistics) |

`Ctrl+U` is the universal argument: it applies to the next command (here, the
Org close-with-note variant of `Ctrl+C Ctrl+T`) and is cancelled by any key
other than the `Ctrl+C` prefix.

A `Ctrl+X` / `Ctrl+C` prefix followed by an unrecognized key reports "no chord"
and clears the prefix; while a prefix is pending, the which-key popup lists the
chords.
The motion chords (`Ctrl+F`, `Ctrl+N`, …) act only when the editor pane is
focused. Bound `Alt` keys take priority over menu mnemonics in this keymap;
unbound `Alt` keys still open the menus.

## Vi modes

The Vi keymap is modal. The active mode is shown in the status-bar mode
indicator: **Normal**, **Insert**, or the live `:` command line (e.g. `:wq`).

**Normal mode** swallows ordinary keys so they never type into the buffer.
Modifier combos and function keys are deferred to the shared layer (menu
mnemonics, `F10`). Most Normal-mode keys act only over the editor; elsewhere the
focused pane keeps its own navigation. The `:` command line, however, can be
opened from any pane.

| Key | Action |
| --- | ------ |
| `h` `j` `k` `l` | Move left / down / up / right |
| `w` / `b` | Next / previous word start |
| `0` / `^` / `$` | Line start (`^` rides smart Home: first non-blank) / line end |
| `gg` / `G` | File start / file end |
| `x` | Delete character |
| `dd` / `yy` / `p` | Cut line / copy line / paste |
| `u` | Undo |
| `/` then `n` / `N` | Find; next / previous match |
| `%` | Jump to the matching bracket |
| `i` / `a` | Enter Insert mode (at / after the cursor) |
| `I` / `A` | Insert at first non-blank / at line end |
| `o` / `O` | Open a new line below / above and enter Insert mode |
| `:` | Open the command line |

`g`, `d`, and `y` are pending operators: the next key completes `gg`/`dd`/`yy`,
and any other key cancels.

**Insert mode** lets typing and shared keys flow through to the editor; `Esc`
returns to Normal mode.

The **`:` command line** echoes live in the mode indicator. `Enter` runs the
command, `Esc` cancels, and backspacing past the empty `:` closes it.

| Command | Action |
| ------- | ------ |
| `:w` | Save |
| `:q` | Close buffer |
| `:q!` | Force-quit (discard unsaved changes) |
| `:wq` / `:x` | Save, then close |
| `:N` (a number) | Go to line N |
| `:e` / `:e path` | Open-file prompt / open `path` |
| `:Ex` | Open the file explorer and focus it |

An unrecognized command reports "no command" in the status bar.

## Spacemacs modes and leader

The Spacemacs keymap shares the Vi keymap's Normal-mode handler, so the whole
Normal-mode vocabulary above (motions, `gg`/`dd`/`yy` operators, search, insert
entries) and the `:` command line behave identically. It reuses the Vi modal
model (Normal / Insert, the same
`hjkl`/`0`/`$`/`x`/`i`/`a`/`o`/`O` Normal-mode keys, and the shared `:` command
line) and adds a **`Space` leader**. In Normal mode, pressing `Space` over the
editor begins a leader sequence shown in the mode indicator as `SPC …`. Each
following key extends the sequence:

- An exact match runs its action and clears the leader.
- A prefix of a longer sequence keeps the leader pending.
- An unknown sequence aborts with `status.spacemacs_no_leader`; `Esc` cancels.

| Sequence | Action |
| -------- | ------ |
| `SPC SPC` | Command palette |
| `SPC f f` / `f r` / `f s` / `f p` | Open / open recent / save / switch project |
| `SPC b n` / `b p` / `b d` | Next / previous / close buffer |
| `SPC p p` / `p f` / `p t` | Switch project / palette / file tree |
| `SPC g s` / `g g` / `g b` | Git changes / status / blame |
| `SPC w /` / `w -` / `w d` / `w w` | Split vertical / horizontal / unsplit / focus other |
| `SPC s s` / `s p` | Find / search workspace |
| `SPC t n` / `t w` | Toggle line numbers / whitespace |
| `SPC ;` | Toggle comment |
| `SPC q q` | Quit |

The leader table lives in `App::spacemacs_leader_lookup` (sequence → action id);
the modal handler is `App::spacemacs_key` / `spacemacs_leader_key`.

## Helix modes, selections and leader

The Helix keymap is modal like Vi, but **selection-first**: in Normal mode a
motion *selects* text, and an action then acts on the selection. `w` selects
from the cursor to the next word start, `x` the whole line, `%` the buffer;
`d` then deletes what is selected, `c` deletes it and enters Insert mode, and
`y` copies it. With nothing selected, an action acts on the one character
under the cursor (Helix never acts on nothing). The status-bar mode
indicator shows **Normal**, **Insert**, **Select** (`status.helix_select`), or
the live `:` command line.

It reuses the Vi keymap's machinery rather than a second engine: the same
Insert-mode flag and mode field (Select mode is `vix_modal::Mode::Visual`),
the same `:` command line, the same pending-key slot, the same count
accumulator (`modal_count`), the `vim.*` insert
entries, the unnamed register (the clipboard), and `vix_modal::motion`'s
pure motions. The selection is the editor's own `[anchor, cursor)` selection;
the selection arithmetic (`x` growing by lines, `p` placement, `r`, `~`,
`WORD` motions) is `vix_modal::helix`. Keys look up `vix-keybindings`'
`"helix"` table, whose `helix.*` action ids carry the selection model.

**Normal mode** swallows ordinary keys so they never type. Modifier combos
and function keys go to the shared layer (menu mnemonics, `F10`). Like Vi,
`:` works from any pane; every other key acts only over the editor.

| Key | Action |
| --- | ------ |
| `h` `j` `k` `l` (and the arrows) | Move; Normal mode drops the selection, Select mode extends it |
| `w` / `b` / `e` | Select to the next word start / previous word start / word end |
| `W` / `B` / `E` | The same over `WORD`s (runs of non-whitespace) |
| `f` / `t` + char | Select through / up to the next char on the line |
| `F` / `T` + char | Select back to / just after the previous char on the line |
| `x` | Select the whole line; again to grow by the next line |
| `%` | Select the whole buffer |
| `d` / `c` | Delete the selection (to the register) / delete it and enter Insert |
| `y` | Copy the selection to the register |
| `p` / `P` | Paste after / before the selection, then select the pasted text (text ending in a newline pastes as whole lines) |
| `r` + char | Replace every selected character with char (line breaks kept) |
| `~` | Switch the case of the selection |
| `>` / `<` | Indent / outdent |
| `J` | Join lines |
| `u` / `U` | Undo / redo |
| `;` | Collapse the selection to the cursor |
| `,` | Keep only the primary selection (drop extra cursors) |
| `C` | Add a cursor on the line below |
| `s` | Select regex matches within the selection (prompts) |
| `*` | Jump to the next occurrence of the selection |
| `/` / `?` then `n` / `N` | Find; next / previous match |
| `v` | Toggle Select mode, where motions extend instead of replace |
| `i` / `a` | Insert before the selection / append after it |
| `I` / `A` | Insert at the first non-blank / append at the line end |
| `o` / `O` | Open a line below / above and enter Insert |
| `.` | Repeat the last change on the current selection |
| `Esc` | Leave Select mode (the selection stays) |
| `:` | Open the command line |

**Counts.** `1`-`9` start a count and `0` continues one; the next command
uses it, as in Helix. `h`/`j`/`k`/`l` move that many times. `w`/`b`/`e`
and `W`/`B`/`E` run that many times, each step selecting afresh from where
the last ended, so `3w` selects the third word (in Select mode the anchor
stays, so `v3w` selects through it). `x` adds that many lines (`2x` selects
two), `f`/`t`/`F`/`T` find the count-th character, `p`/`P` paste that many
copies, `u`/`U`/`>`/`<` run that many times, `{N}gg` goes to line `N`, and
`{N}.` repeats `N` times. Other commands ignore a count. A motion that
stops moving (at the buffer's end) ends its count early; repeats are capped
at 10,000.

**`.` repeat.** `.` repeats the last change on what is selected *now* (or
the character under the cursor): `d`; `c` and the text typed before `Esc`;
`i`/`a` and the text typed before `Esc`; `r` + char; `~`; `>`/`<`; and
`p`/`P` with their count. The change is remembered by what it did, not by
its keys, so `w d w .` deletes the next word too. Motions, `y`, undo and
redo do not change what `.` repeats. A repeated `c` is one undo step.

`g`, `m`, and `Space` are prefixes; `f`, `t`, `F`, `T`, and `r` wait for a
character. The next key completes them, and `Esc` or an unbound key cancels.
While `g`, `m`, or `Space` is pending, the which-key popup lists its keys.

| Goto (`g`) | Action |
| ---------- | ------ |
| `g g` / `g e` | File start / file end |
| `g h` / `g l` / `g s` | Line start / line end / first non-blank |
| `g d` / `g D` / `g y` / `g i` | Definition / declaration / type definition / implementation |
| `g r` | References |
| `g n` / `g p` | Next / previous buffer |
| `m m` | Matching bracket |

| Space leader | Action |
| ------------ | ------ |
| `Space f` | File picker |
| `Space b` | Buffer picker |
| `Space s` / `Space S` | Symbol picker / workspace symbol picker |
| `Space /` | Global search |
| `Space k` | Hover |
| `Space r` | Rename symbol |
| `Space a` | Code action |
| `Space d` | Diagnostics |
| `Space p` / `Space P` / `Space y` | Paste after / paste before / yank |
| `Space ?` | Command palette |

**Insert mode** lets typing through to the editor; `Esc` returns to Normal.
Typing at several cursors (after `C`) uses the editor's own multi-cursor
insert.

The **`:` command line** is the Vi one, so every Vi command above works
(`:w`, `:q`, `:q!`, `:wq`, `:x`, `:N`, `:e path`, `:Ex`), plus Helix's
names: `:write`, `:quit`, `:quit!`, `:write-quit`, and `:o` / `:open`
(`:o path` opens `path`; bare `:o` opens the file picker).

### Helix: not yet

These Helix features are not implemented in Vix yet:

- Multiple selections as first-class objects: `d`/`c`/`y`/`p`/`r`/`~`, the
  motions, and Select mode act on the primary selection only; extra cursors
  (from `C` or `s`) are the editor's multi-cursor carets, which typing in
  Insert mode honors. No `Alt-s` split here (use Edit → Select → Split
  Selection into Lines), no `(`/`)` rotate, no `Alt-;` flip.
- Named registers (`"a`); Helix's yank register and the system clipboard
  are one clipboard here, so `Space p`/`Space y` behave like `p`/`y`.
- Match mode beyond `m m` (`m s` surround, `m i`/`m a` text objects).
- View mode (`z`), window mode (`Ctrl+w`), jump list (`Ctrl+o`/`Ctrl+i`),
  macros (`q`/`Q`), `Alt-.` (repeat the last motion), `gw` jump labels.
- `?` searches forward like `/` (no reverse search), and `*` jumps to the
  next occurrence rather than only setting the search pattern.
- In Select mode the goto keys (`gg`, `ge`, `gh`, `gl`) move the cursor
  rather than extend the selection.
- The block cursor and per-mode cursor shape.

## As implemented in Vix

The list of keymaps is pure data in the `keymap_model` module
(`crates/vix-keymap-model/src/lib.rs`): the `Keymap { id, name, tooltip }` struct, the `KEYMAPS`
slice (Apple, VSCode macOS, VSCode Windows, Emacs, Vi, Spacemacs, IntelliJ macOS,
IntelliJ Windows, Eclipse — in menu order), and the `by_id` lookup.

The View → Keymap submenu is `VIEW_KEYMAP` in `crates/vix-menu/src/lib.rs`, one leaf per keymap
with action `view.keymap:<id>`, kept in sync with `KEYMAPS` by the
`keymap_submenu_matches_model` test.

The host wiring lives in `tui/src/app.rs`:

- `enum Keymap` and `Keymap::from_id` — the active style, parsed from
  `settings.keymap` (unknown ids fall back to `Apple`); `App::active_keymap`
  derives it.
- `App::on_key` — keymap-specific dispatch: `global_key` (Apple), `vscode_key`,
  `emacs_key`, and `vim_key`, with `global_shared_key` as the fallback for Emacs
  and Vi.
- `set_keymap` — handles `view.keymap:<id>`: validates against the model,
  persists `settings.keymap`, calls `reset_keymap_modes`, and sets the status.
- `reset_keymap_modes` — clears the Emacs chord (`emacs_chord` back to
  `EmacsChord::None`), the Vi `MODAL_INSERT` flag and `vim_cmd`, and resets the
  modal engine's mode, count, pending operator/find/text object and active
  register, so a newly chosen keymap starts clean (Vi in Normal mode).
- `emacs_key` (with the `emacs_chord` field) and `vim_key` / `vim_cmd_key` /
  `run_vim_command` (with the `MODAL_INSERT` flag and `vim_cmd` field)
  implement the chords and modes above; `vim_key` offers the key to
  `App::modal_key` (`tui/src/app/modal.rs`, the `vix-modal` engine) first.
- `mode_indicator` — the status-bar string: Vi's `:`-line / Insert / Normal, or
  Emacs's pending `C-x-` prefix; `None` for keymaps with nothing to show.
- Helix lives in `tui/src/app/helix.rs`: `helix_key` (dispatch, prefixes,
  character arguments), `run_helix_action` (the `helix.*` actions),
  `run_helix_command` (Helix's `:` command names), and `helix_which_key`.

The default keymap is set in `crates/vix-settings/src/lib.rs` (`keymap: "apple"`).
