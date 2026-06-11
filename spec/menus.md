# Menus

Top menu bar.

- Open with `F10`, or `Alt+F/E/V/T/H` for File/Edit/View/Tools/Help.
- Arrows navigate, `Enter` runs, `Esc` closes.
- A mouse click on a menu name opens it; a click on a dropdown item runs it.
- While a menu is open, moving the pointer follows the selection: hovering a
  dropdown row highlights it, and hovering another top-level name switches menus.
- Dropdowns may contain **separators** — non-selectable divider lines that group
  related items. Arrow navigation, hover, and clicks all skip them.
- An item marked `▸` opens a **submenu** (one level deep). `Right` or a click
  opens it; `Left` or `Esc` backs out to the parent.

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
| *— separator —* | | |
| Open…        | `Ctrl+O`       | Open an existing file       |
| Open Recent… | `Ctrl+Shift+O` | Reopen a recently opened file (chooser) |
| Save         | `Ctrl+S`       | Save the file               |
| Save As… | `Ctrl+Shift+S` | Save under a different name |
| *— separator —* | | |
| Close    | `Ctrl+W`       | Close the active buffer     |
| Close All Tabs | | Close every buffer (leaves one empty) |
| Reopen Closed Tab | `Ctrl+Shift+T` | Reopen the most recently closed file |
| *— separator —* | | |
| Quit     | `Ctrl+Q`       | Quit Vix                    |

## Edit menu

| Item    | Shortcut | Action                       |
| ------- | -------- | ---------------------------- |
| Undo    | `Ctrl+Z` | Undo                         |
| Redo    | `Ctrl+Y` | Redo                         |
| *— separator —* | | |
| Cut     | `Ctrl+X` | Cut to clipboard             |
| Copy    | `Ctrl+C` | Copy to clipboard            |
| Paste   | `Ctrl+V` | Paste from clipboard         |
| Select All | `Ctrl+A` | Select the whole buffer    |
| *— separator —* | | |
| Toggle Comment | `Ctrl+/` | Comment/uncomment the line or selection |
| *— separator —* | | |
| Find… ▸  | | Submenu of find-related items (below) |

The **Find…** submenu:

| Item    | Shortcut | Action                       |
| ------- | -------- | ---------------------------- |
| Find    | `Ctrl+F` | Find in the current file     |
| Find Next | `Ctrl+G` | Repeat the last search forward (works after the box closes) |
| Find Previous | `Ctrl+Shift+G` | Repeat the last search backward |
| Find Selection | `Alt+N` | Jump to the next occurrence of the selection |
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
| *— separator —*                  |                                               |
| Show/Hide Left Dock              | Show/hide the file explorer (`Ctrl+B`)        |
| Show/Hide Right Dock             | Show/hide the message drawer                  |
| Show/Hide Bottom Status          | Show/hide the bottom status bar               |
| *— separator —*                  |                                               |
| Editor ▸                         | Submenu of editor display toggles (below)     |
| Show/Hide Soft Wrap              | Wrap long lines vs. scroll horizontally       |

The **Editor** submenu:

| Item                             | Action                                        |
| -------------------------------- | --------------------------------------------- |
| Show/Hide Editor Line Numbers    | Show/hide the line-number gutter              |
| Show/Hide Editor Visible Whitespace | Show/hide visible space, tab, newline, return |
| Show/Hide Editor Scroll Bar      | Show/hide the editor's right-side scroll bar  |

## Tools menu

| Item               | Action                                            |
| ------------------ | ------------------------------------------------- |
| Calendar           | Toggle the calendar box                           |
| Nerd Font Palette… | Open the glyph picker (`nerd-font-palette.md`)    |
| *— separator —*    |                                                   |
| Command Palette    | Open the palette (`Ctrl+P`)                       |

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

- View ▸ **Zoom In / Out / Zero** (terminal font zoom).
- Separate Edit menu entries for project-wide find/replace (today these live on
  shortcuts and the palette, not the menu).
