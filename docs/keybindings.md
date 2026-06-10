# Vix Keybindings

Every shortcut Vix currently implements. The bindings below are the **Apple**
keyway (the default): familiar macOS/Windows modifier shortcuts. Two other
keyways change this dispatch — **Emacs** (`Ctrl` chords) and **Vim** (modal).
Switch via **View → Keyway…**; the choice persists. See [Keyways](#keyways).

## Global

| Shortcut         | Action                                              |
| ---------------- | --------------------------------------------------- |
| `Ctrl+N`         | New buffer                                           |
| `Ctrl+O`         | Open file… (prompt; accepts `path:line[:col]`)       |
| `Ctrl+S`         | Save (prompts for a path if the buffer is untitled)  |
| `Ctrl+Shift+S`   | Save As…                                             |
| `Ctrl+W`         | Close the active tab                                 |
| `Ctrl+Q`         | Quit                                                 |
| `Ctrl+P`         | Open the command palette                             |
| `Ctrl+F`         | Find                                                 |
| `Ctrl+R`         | Find & Replace                                       |
| `Ctrl+Alt+R`     | Interactive query-replace (y/n/!/q)                  |
| `Ctrl+Shift+F`   | Search across the whole project                      |
| `F3` / `Shift+F3`| Find next / previous                                 |
| `Ctrl+B`         | Toggle the file explorer (reveals the active file)   |
| `Ctrl+E`         | Toggle focus between explorer and editor             |
| `Alt+Left` / `Alt+Right` | Position history: jump back / forward        |
| `F12`            | Go to definition of the symbol under the cursor      |
| `F10`            | Open / close the menu bar                            |
| `Alt+F/E/V/T/H`  | Open the File / Edit / View / Tools / Help menu      |
| `F1`             | Show the keyboard-shortcut help overlay              |

## Editor (when the editor is focused)

| Shortcut          | Action                                |
| ----------------- | ------------------------------------- |
| `Ctrl+Z`          | Undo                                  |
| `Ctrl+Y`          | Redo                                  |
| `Ctrl+X`          | Cut selection                         |
| `Ctrl+C`          | Copy selection                        |
| `Ctrl+V`          | Paste                                 |
| `Ctrl+A`          | Select all                            |
| Arrows / `Home` / `End` / `PgUp` / `PgDn` | Move the cursor       |
| Typing / `Enter` / `Backspace` / `Delete` / `Tab` | Edit text     |

The editor wraps the internal `vix-code-editor-panel` widget, which adds
Tree-sitter syntax highlighting, `Ctrl+K` delete-line, and `Ctrl+D`
duplicate-line.

## Mouse

| Gesture                  | Action                                          |
| ------------------------ | ----------------------------------------------- |
| Click in the editor      | Place the cursor there (and focus the editor)   |
| Drag in the editor       | Select text                                     |
| Wheel over editor        | Scroll the buffer                               |
| Click a tab              | Switch to that buffer                           |
| Click a file in explorer | Preview it; click again to open permanently     |
| Click a directory        | Expand / collapse it                            |
| Wheel over explorer      | Move the selection                              |
| Click a message's `x`    | Dismiss that message                            |
| Click a menu name        | Open that menu                                  |
| Hover over an open menu  | Move the highlight (rows) / switch menus (names) |
| Click a dock toggle icon | Toggle the explorer / messages drawer (top-right of the menu bar) |

## File explorer (when focused — `Ctrl+E`)

| Shortcut          | Action                                          |
| ----------------- | ----------------------------------------------- |
| `Up` / `Down`     | Move selection (scans a preview tab on the way) |
| `PgUp` / `PgDn`   | Move by a page                                  |
| `Home` / `End`    | Jump to first / last entry                      |
| `Enter` / `Right` | Open file, or expand/collapse a directory       |
| `Left`            | Collapse / expand the selected directory        |
| `Shift+Up`/`Down` | Extend the multi-selection                      |
| `Ctrl+C`/`Ctrl+X` | Copy / cut the selection                        |
| `Ctrl+V`          | Paste into the selected directory               |
| `Delete`          | Delete the selection (asks to confirm)          |
| `Esc`             | Cancel a pending cut / clear selection / unfocus |

During a paste with a name conflict, choose **(o)** overwrite, **(O)** overwrite
all, **(s)** skip, **(S)** skip all, or **(c)** cancel. Same-directory copies are
auto-renamed (`name copy`, `name copy 2`, …); a same-directory cut is a no-op.
Open buffers follow their files when moved and close when deleted.

## Message drawer (when focused)

| Shortcut                | Action                       |
| ----------------------- | ---------------------------- |
| `Up` / `Down`           | Move selection               |
| `x` / `Delete` / `Enter`| Dismiss the selected message |
| `Esc`                   | Return focus to the editor   |

## Command palette (`Ctrl+P`)

| Key            | Action                                       |
| -------------- | -------------------------------------------- |
| (type)         | Filter the current mode                      |
| `Up` / `Down`  | Move the selection                           |
| `Tab`          | Accept the top suggestion                    |
| `Enter`        | Accept the highlighted entry                 |
| `Esc`          | Close the palette                            |

Prefixes switch modes: *(none)* file finder, `>` commands, `#` buffers,
`:` go-to-line. In file-finder mode, append `:line[:col]` to jump on open.

## Find & Replace

| Key                 | Action                                            |
| ------------------- | ------------------------------------------------- |
| (type)              | Edit the focused field (incremental in Find)      |
| `Tab`               | Switch between the Find and Replace fields        |
| `Enter`             | Find next (or Replace-All from the Replace field) |
| `Alt+Enter`         | Replace all                                       |
| `F3` / `Shift+F3`   | Find next / previous                              |
| `Alt+C`             | Toggle case sensitivity                           |
| `Alt+W`             | Toggle whole-word matching                        |
| `Alt+R`             | Toggle regular-expression mode                    |
| `Esc`               | Close the toolbar                                 |

In regex mode the replacement supports capture groups (`$1`, `${name}`) and the
escapes `\n`, `\t`, `\r`, `\\`.

### Interactive query-replace

Start with `Ctrl+Alt+R` (or the palette's "Query Replace"), type the search and
replacement, then press `Enter` to step through each match:

| Key            | Action                                |
| -------------- | ------------------------------------- |
| `y` / `Space`  | Replace this match, go to the next    |
| `n` / `Delete` | Skip this match, go to the next       |
| `!`            | Replace this match and all remaining  |
| `q` / `Esc`    | Stop                                  |

## View menu overlays

The **View** menu opens small chooser overlays:

| Overlay                | Keys                                                   |
| ---------------------- | ------------------------------------------------------ |
| **Theme…**            | `↑`/`↓` preview live, `Enter` apply & save, `Esc` cancel |
| **Locale…** (language) | `↑`/`↓` preview live, `Enter` apply & save, `Esc` cancel |
| **Keyway…**            | `↑`/`↓` highlight, `Enter` apply & save, `Esc` cancel    |

In every chooser a left **click** on a row highlights it (Themes/Locale also
preview live).

View also has toggles for the line-number gutter, visible whitespace (glyphs
for space `·`, tab `→`, and line ending `¶`), and the explorer / messages docks.

## Calendar box (Tools → Calendar)

| Key             | Action                          |
| --------------- | ------------------------------- |
| `Left` / `Right`| Previous / next month           |
| `Esc` / `q`     | Close the calendar              |

The date/time area always shows the present; only the month grid navigates.

## Keyways

A *keyway* is the keyboard navigation style. Exactly one is active at a time;
choose it in **View → Keyway…**. Menu mnemonics (`Alt+F/E/V/T/H`) and function
keys (`F1`, `F3`, `F10`, `F12`) work in every keyway.

### Apple (default)

Modifier shortcuts — the tables above.

### Emacs

`Ctrl` chords, with a `Ctrl+X` prefix for file commands:

| Keys                | Action                          |
| ------------------- | ------------------------------- |
| `Ctrl+X` `Ctrl+F`   | Open file…                      |
| `Ctrl+X` `Ctrl+S`   | Save                            |
| `Ctrl+X` `Ctrl+C`   | Quit                            |
| `Ctrl+X` `k`        | Close the active tab            |
| `Ctrl+F` / `Ctrl+B` | Forward / back one character    |
| `Ctrl+N` / `Ctrl+P` | Next / previous line            |
| `Ctrl+A` / `Ctrl+E` | Start / end of line             |
| `Ctrl+V`            | Page down                       |
| `Ctrl+D`            | Delete the character ahead      |
| `Ctrl+S`            | Find                            |
| `Ctrl+G`            | Cancel                          |

### Vim

Modal. The status bar shows `-- NORMAL --`, `-- INSERT --`, or the `:` command
line. Press `Esc` to return to Normal mode.

| Mode    | Keys                | Action                              |
| ------- | ------------------- | ----------------------------------- |
| Normal  | `h` `j` `k` `l`     | Left / down / up / right            |
| Normal  | `0` / `$`           | Start / end of line                 |
| Normal  | `x`                 | Delete the character ahead          |
| Normal  | `i` / `a`           | Insert before / after the cursor    |
| Normal  | `o` / `O`           | Open a line below / above           |
| Insert  | `Esc`               | Return to Normal mode               |
| Command | `:w`                | Save                                |
| Command | `:q` / `:q!`        | Close tab / quit                    |
| Command | `:wq` / `:x`        | Save and close                      |
| Command | `:Ex`               | Focus the file explorer             |
