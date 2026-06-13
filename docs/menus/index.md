# Menus

The top menu bar groups every command into dropdown menus. From left to right
the menus are **Vix · File · Edit · View · Tools · Git · Help**.

## Navigating the menus

- Open the bar with **F10**, or jump straight to a menu with its Alt mnemonic:
  **Alt+F/E/V/T/H** for File / Edit / View / Tools / Help.
- **Arrows** navigate, **Enter** runs the highlighted item, **Esc** closes.
- A mouse click on a menu name opens it; a click on a dropdown item runs it.
- While a menu is open, moving the pointer follows the selection: hovering a
  dropdown row highlights it, and hovering another top-level name switches
  menus.
- Dropdowns may contain **separators** — non-selectable divider lines that
  group related items. Arrow navigation, hover, and clicks all skip them.
- An item marked `▸` opens a **submenu** (one level deep). **Right** or a click
  opens it; **Left** or **Esc** backs out to the parent.
- **Type-ahead:** with a menu open, typing a letter jumps to the next item
  whose label starts with it, cycling. For example, in File, `S` selects Save
  and `S` again selects Save As. This works inside an open submenu too.

## Vix menu

- **About Vix** — modal dialog showing `Vix <version>` and an **Ok** button.
- **Website** — modal dialog with a selectable/copyable text field
  `https://github.com/vixide/vix` and an **Ok** button.
- **Email** — modal dialog with a selectable/copyable text field
  `joel@joelparkerhenderson.com` and an **Ok** button.
- **Quit** (`Ctrl+Q`) — quit Vix.

## File menu

| Item              | Shortcut       | Action                                  |
| ----------------- | -------------- | --------------------------------------- |
| New               | `Ctrl+N`       | Create a new buffer                     |
| Open…             | `Ctrl+O`       | Open an existing file                   |
| Open Recent…      | `Ctrl+Shift+O` | Reopen a recently opened file (chooser) |
| Save              | `Ctrl+S`       | Save the file                           |
| Save As…          | `Ctrl+Shift+S` | Save under a different name             |
| Close             | `Ctrl+W`       | Close the active buffer                 |
| Close All Tabs    |                | Close every buffer (leaves one empty)   |
| Reopen Closed Tab | `Ctrl+Shift+T` | Reopen the most recently closed file    |

## Edit menu

| Item        | Shortcut       | Action                                  |
| ----------- | -------------- | --------------------------------------- |
| Undo        | `Ctrl+Z`       | Undo                                    |
| Redo        | `Ctrl+Shift+Z` | Redo                                    |
| Cut         | `Ctrl+X`       | Cut to clipboard                        |
| Copy        | `Ctrl+C`       | Copy to clipboard                       |
| Paste       | `Ctrl+V`       | Paste from clipboard                    |
| Select ▸    |                | Submenu of selection commands (below)   |
| Move ▸      |                | Submenu of line-move commands (below)   |
| Go ▸        |                | Submenu of cursor-jump commands (below) |
| Find ▸      |                | Submenu of find-related items (below)   |
| Case ▸      |                | Submenu of case transforms for the selection |
| Comment     | `Ctrl+/`       | Comment/uncomment the line or selection |

The **Select** submenu:

| Item        | Shortcut       | Action                                  |
| ----------- | -------------- | --------------------------------------- |
| Select All  | `Ctrl+A`       | Select the whole buffer                 |
| Select More | `Ctrl+Shift+→` | Extend the selection right by a word    |
| Select Less | `Ctrl+Shift+←` | Retract the selection left by a word    |

The **Move** submenu:

| Item      | Shortcut | Action                   |
| --------- | -------- | ------------------------ |
| Move Up   | `Alt+↑`  | Move the current line up |
| Move Down | `Alt+↓`  | Move the current line down |

The **Go** submenu:

| Item            | Action                                       |
| --------------- | -------------------------------------------- |
| Line Number     | Jump to a line number                        |
| Line Start      | Move to the start of the current line        |
| Line End        | Move to the end of the current line          |
| Paragraph Start | Move to the start of the paragraph           |
| Paragraph End   | Move to the end of the paragraph             |
| Section Start   | Move to the start of the section             |
| Section End     | Move to the end of the section               |
| File Start      | Move the cursor to the start of the file     |
| File End        | Move the cursor to the end of the file       |

The **Find** submenu:

| Item               | Shortcut       | Action                                       |
| ------------------ | -------------- | -------------------------------------------- |
| Find…              | `Ctrl+F`       | Find in the current file                     |
| Find Next          | `Ctrl+G`       | Repeat the last search forward               |
| Find Previous      | `Ctrl+Shift+G` | Repeat the last search backward              |
| Find Selection     | `Alt+N`        | Jump to the next occurrence of the selection |
| Find In Workspace… |                | List workspace-wide hits in the bottom dock    |

Replace is part of the Find panel itself — press `Ctrl+R`, or `Tab` to the
Replace field in the find box, to reveal it.

The **Case** submenu applies a case transform to the current selection:
Upper, Lower, Title, Kebab, Snake, Camel, and Pascal.

## View menu

| Item     | Action                       |
| -------- | ---------------------------- |
| Theme…   | Open the theme chooser       |
| Locale…  | Open the locale chooser      |
| Keymap…  | Open the keymap chooser      |
| Layout ▸ | Submenu of dock/status toggles |
| Editor ▸ | Submenu of editor display toggles |

The **Layout** submenu toggles **Left Dock** (the file explorer, `Ctrl+B`),
**Right Dock** (the message drawer), **Bottom Dock** (logs/output/data), and
**Bottom Status** (the status bar).

The **Editor** submenu toggles **Line Numbers**, **Whitespace** (visible space,
tab, newline, return), **Scroll Bar**, **Soft Wrap** (wrap long lines vs.
scroll horizontally), and **Spellcheck** (underline misspellings in comments
and strings).

## Tools menu

| Item                | Action                                       |
| ------------------- | -------------------------------------------- |
| Command Palette     | Open the palette (`Ctrl+P`)                  |
| Workspace Dashboard…  | Live folder/disk/file/commit metrics         |
| System Information… | Host OS/CPU/memory/disk snapshot             |
| *— separator —*     |                                              |
| Run Command…        | Run a shell command into the bottom dock     |
| Cancel Command      | Kill the running command                     |
| *— separator —*     |                                              |
| Calendar…           | Toggle the calendar box                      |
| Nerd Font Characters… | Open the glyph picker                      |
| ASCII Characters…   | Open the ASCII reference table               |
| X11 Colors…         | Pick an X11 color; inserts its hex           |
| HTML Characters…    | Pick an HTML character; click a cell to insert it |
| Language Server ▸   | LSP actions: Go to Definition, Hover, Completion |

## Git menu

| Item           | Action                                                   |
| -------------- | -------------------------------------------------------- |
| Changes…       | Open the git changes panel: stage/unstage and commit     |
| Log…           | Show the commit history (`git log`)                      |
| Switch Branch… | Choose a local branch to check out                       |
| New Branch…    | Create a new branch and switch to it                     |
| Pull           | `git pull` (streamed to the bottom dock)                 |
| Push           | `git push` (streamed to the bottom dock)                 |
| Fetch          | `git fetch` (streamed to the bottom dock)                |

## Help menu

| Item               | Action                            |
| ------------------ | --------------------------------- |
| Keyboard Shortcuts | Open the help overlay (also `F1`) |

## Layout conventions

Each menu item left-aligns its title (for example, "Open…"), right-aligns its
shortcut (for example, "Ctrl+O"), and keeps at least one space between them.
