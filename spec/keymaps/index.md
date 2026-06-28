# Keymaps

A *keymap* is a whole-keyboard philosophy for driving Vix — the editor, the
menus, and the file explorer. Exactly one keymap is active at a time, and it
decides how each raw key event is interpreted before the focused pane ever sees
it. Switching keymaps changes the meaning of keys, not the available actions: the
same commands (open, save, find, move the cursor) stay reachable; only the keys
that trigger them differ.

Vix ships four keymaps. **Apple** is the default and matches Vix's own bindings.

## The five keymaps

| Keymap | Id | Philosophy |
| ------ | -- | ---------- |
| Apple | `apple` | Modifier-key shortcuts (the default), e.g. `Ctrl+O` to open, `Ctrl+S` to save. |
| macOS VSCode | `vscode` | VS Code's signature shortcuts, with `Ctrl` standing in for `Cmd` — `Ctrl+P` Quick Open, `Ctrl+Shift+P` Command Palette, `Ctrl+G` Go to Line. |
| Emacs | `emacs` | Layered `Ctrl` chords and a `Ctrl+X` prefix, e.g. `Ctrl+X Ctrl+F` to open. |
| Vi | `vim` | Modal editing: a Normal mode for motions and commands, plus an Insert mode and a `:` command line. |
| Spacemacs | `spacemacs` | Vi modal editing plus a `Space` leader for menu-like command sequences (e.g. `SPC f f` find file). |

Each keymap gets first chance to consume a key. Apple and VS Code dispatch their
shortcuts directly; Emacs, Vi, and Spacemacs try their own handling and then fall
back to a **shared** layer (menu-bar mnemonics like `Alt+F` and function keys like
`F10`) before the focused pane handles the key.

## Choosing a keymap

The keymap is selected from the **View → Keymap** submenu in the top menu bar.
The submenu lists the four keymaps by their proper names (not translated), in
model order, and is kept in sync with the keymap model by a test.

| Item | Action |
| ---- | ------ |
| Apple | `view.keymap:apple` |
| macOS VSCode | `view.keymap:vscode` |
| Emacs | `view.keymap:emacs` |
| Vi | `view.keymap:vi` |
| Spacemacs | `view.keymap:spacemacs` |

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
- The Emacs `Ctrl+X` chord prefix is cleared.

## Emacs chords

In the Emacs keymap, `Ctrl`-key chords drive motions and commands, and `Ctrl+X`
acts as a prefix whose next key completes a two-key chord. While the prefix is
pending, the status bar shows the mode indicator `C-x-`.

| Chord | Action |
| ----- | ------ |
| `Ctrl+F` / `Ctrl+B` | Move cursor right / left |
| `Ctrl+N` / `Ctrl+P` | Move cursor down / up |
| `Ctrl+A` / `Ctrl+E` | Line start / line end |
| `Ctrl+V` | Page down |
| `Ctrl+D` | Delete forward |
| `Ctrl+S` | Find |
| `Ctrl+G` | Cancel (status message) |
| `Ctrl+X Ctrl+F` | Open file |
| `Ctrl+X Ctrl+S` | Save file |
| `Ctrl+X Ctrl+C` | Quit |
| `Ctrl+X K` | Close buffer |

A `Ctrl+X` prefix followed by an unrecognized key reports "no chord" and clears
the prefix. The motion chords (`Ctrl+F`, `Ctrl+N`, …) act only when the editor
pane is focused.

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
| `0` / `$` | Line start / line end |
| `x` | Delete character |
| `i` | Enter Insert mode |
| `a` | Move right, then enter Insert mode |
| `o` | Open a new line below and enter Insert mode |
| `O` | Open a new line above and enter Insert mode |
| `:` | Open the command line |

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
| `:Ex` | Open the file explorer and focus it |

An unrecognized command reports "no command" in the status bar.

## Spacemacs modes and leader

The Spacemacs keymap reuses the Vi modal model (Normal / Insert, the same
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

## As implemented in Vix

The list of keymaps is pure data in the `keymap_model` crate
(`keymap_model/src/lib.rs`): the `Keymap { id, name, tooltip }` struct, the
`KEYMAPS` slice (Apple, macOS VSCode, Emacs, Vi — in menu order), and the
`by_id` lookup.

The View → Keymap submenu is `VIEW_KEYMAP` in `src/menu.rs`, one leaf per keymap
with action `view.keymap:<id>`, kept in sync with `KEYMAPS` by the
`keymap_submenu_matches_model` test.

The host wiring lives in `src/app.rs`:

- `enum Keymap` and `Keymap::from_id` — the active style, parsed from
  `settings.keymap` (unknown ids fall back to `Apple`); `App::active_keymap`
  derives it.
- `App::on_key` — keymap-specific dispatch: `global_key` (Apple), `vscode_key`,
  `emacs_key`, and `vim_key`, with `global_shared_key` as the fallback for Emacs
  and Vi.
- `set_keymap` — handles `view.keymap:<id>`: validates against the model,
  persists `settings.keymap`, calls `reset_keymap_modes`, and sets the status.
- `reset_keymap_modes` — clears `emacs_prefix`, `vim_insert`, and `vim_cmd` so a
  newly chosen keymap starts clean (Vi in Normal mode).
- `emacs_key` (with the `emacs_prefix` flag) and `vim_key` / `vim_cmd_key` /
  `run_vim_command` (with the `vim_insert` and `vim_cmd` fields) implement the
  chords and modes above.
- `mode_indicator` — the status-bar string: Vi's `:`-line / Insert / Normal, or
  Emacs's pending `C-x-` prefix; `None` for keymaps with nothing to show.

The default keymap is set in `src/settings.rs` (`keymap: "apple"`).
