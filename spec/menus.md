# Menus

Top menu bar.

- Open with `F10`, or `Alt+F/E/V/T/H` for File/Edit/View/Tools/Help.
- Arrows navigate, `Enter` runs, `Esc` closes.
- A mouse click on a menu name opens it; a click on a dropdown item runs it.
- While a menu is open, moving the pointer follows the selection: hovering a
  dropdown row highlights it, and hovering another top-level name switches menus.

The menus, left to right, are **Vix · File · Edit · View · Tools · Help**.

## Vix menu

- About Vix — modal dialog showing `Vix <version>` and an **Ok** button.
- Website — modal dialog with a selectable/copyable text field
  `https://github.com/joelparkerhenderson/vix` and an **Ok** button.
- Email — modal dialog with a selectable/copyable text field
  `joel@joelparkerhenderson.com` and an **Ok** button.

## File menu

| Item     | Shortcut       | Action                      |
| -------- | -------------- | --------------------------- |
| New          | `Ctrl+N`       | Create a new buffer         |
| Open…        | `Ctrl+O`       | Open an existing file       |
| Open Recent… | `Ctrl+Shift+O` | Reopen a recently opened file (chooser) |
| Save         | `Ctrl+S`       | Save the file               |
| Save As… | `Ctrl+Shift+S` | Save under a different name |
| Close    | `Ctrl+W`       | Close the active buffer     |
| Quit     | `Ctrl+Q`       | Quit Vix                    |

## Edit menu

| Item    | Shortcut | Action                       |
| ------- | -------- | ---------------------------- |
| Undo    | `Ctrl+Z` | Undo                         |
| Redo    | `Ctrl+Y` | Redo                         |
| Cut     | `Ctrl+X` | Cut to clipboard             |
| Copy    | `Ctrl+C` | Copy to clipboard            |
| Paste   | `Ctrl+V` | Paste from clipboard         |
| Toggle Comment | `Ctrl+/` | Comment/uncomment the line or selection |
| Find    | `Ctrl+F` | Find in the current file     |
| Replace | `Ctrl+R` | Find-and-replace in the file |

(Project-wide search/replace is `Ctrl+Shift+F`; interactive query-replace is
`Ctrl+Alt+R`. Both are reachable from the command palette — see
`search-and-replace.md`.)

## View menu

| Item                             | Action                                        |
| -------------------------------- | --------------------------------------------- |
| Theme…                           | Open the theme chooser (`theme-chooser.md`)   |
| Locale…                          | Open the locale chooser (`locale-chooser.md`) |
| Keyway…                          | Open the keyway chooser (`keyway-chooser.md`) |
| Toggle Left Dock                 | Show/hide the file explorer (`Ctrl+B`)        |
| Toggle Right Dock                | Show/hide the message drawer                  |
| Toggle Editor Line Numbers       | Show/hide the line-number gutter              |
| Toggle Editor Visible Whitespace | Show/hide visible space, tab, newline, return |
| Toggle Soft Wrap                 | Wrap long lines vs. scroll horizontally       |

## Tools menu

| Item            | Action                      |
| --------------- | --------------------------- |
| Calendar        | Toggle the calendar box     |
| Command Palette | Open the palette (`Ctrl+P`) |

## Help menu

| Item               | Action                            |
| ------------------ | --------------------------------- |
| Keyboard Shortcuts | Open the help overlay (also `F1`) |

## For all menu items

- Left-align the menu item title (e.g. "Open…").
- Right-align the shortcut (e.g. "Ctrl+O").
- Keep at least one space between the title and the shortcut.

## Planned (not yet built)

These appear in the design but are not implemented yet:

- **Select** menu with **Select All** (`Ctrl+A`).
- View ▸ **Zoom In / Out / Zero** (terminal font zoom).
- Separate Edit menu entries for project-wide find/replace (today these live on
  shortcuts and the palette, not the menu).
