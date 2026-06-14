# Menus

Top menu bar.

- Open with `F10`, or `Alt+F/E/V/T/A/G/H` for File/Edit/View/Tools/AI/Git/Help.
- Arrows navigate, `Enter` runs, `Esc` closes.
- A mouse click on a menu name opens it; a click on a dropdown item runs it.
- While a menu is open, moving the pointer follows the selection: hovering a
  dropdown row highlights it, and hovering another top-level name switches menus.
- Dropdowns may contain **separators** — non-selectable divider lines that group
  related items. Arrow navigation, hover, and clicks all skip them.
- An item marked `▸` opens a **submenu** (one level deep). `Right` or a click
  opens it; `Left` or `Esc` backs out to the parent.
- A dropdown or submenu taller than the screen **scrolls** to keep the highlight
  visible (arrow keys, type-ahead, or the mouse wheel move it); a `●` scrollbar
  marks the right edge. This matters for long lists like the View → Time Zone and
  View → Theme submenus.
- **Type-ahead:** with a menu open, typing a letter jumps to the next item whose
  label starts with it, cycling. E.g. in File, `S` selects Save, `S` again selects
  Save As. Works inside an open submenu too.

The menus, left to right, are
**Vix · File · Edit · View · Tools · AI · Git · Help**.

## Vix menu

- About Vix — modal dialog showing `Vix <version>` and an **Ok** button.
- Website — modal dialog with a selectable/copyable text field
  `https://github.com/vixide/vix` and an **Ok** button.
- Email — modal dialog with a selectable/copyable text field
  `joel@joelparkerhenderson.com` and an **Ok** button.
- *separator*
- Quit (`Ctrl+Q`) — quit Vix.

## File menu

| Item     | Shortcut       | Action                      |
| -------- | -------------- | --------------------------- |
| New          | `Ctrl+N`       | Create a new buffer         |
| *— separator —* | | |
| Open…        | `Ctrl+O`       | Open an existing file       |
| Open Recent… | `Ctrl+Shift+O` | Reopen a recently opened file (chooser) |
| Save         | `Ctrl+S`       | Save the file               |
| Save As… | `Ctrl+Shift+S` | Save under a different name |
| Rename…  | | Rename the active file on disk (bare name keeps the directory) |
| *— separator —* | | |
| Close    | `Ctrl+W`       | Close the active buffer     |
| Close All Tabs | | Close every buffer (leaves one empty) |
| Reopen Closed Tab | `Ctrl+Shift+T` | Reopen the most recently closed file |

(Quit moved to the **Vix** menu.)

## Edit menu

| Item    | Shortcut | Action                       |
| ------- | -------- | ---------------------------- |
| Undo    | `Ctrl+Z` | Undo                         |
| Redo    | `Ctrl+Shift+Z` | Redo                   |
| *— separator —* | | |
| Cut     | `Ctrl+X` | Cut to clipboard             |
| Copy    | `Ctrl+C` | Copy to clipboard            |
| Paste   | `Ctrl+V` | Paste from clipboard         |
| Select ▸ | | Submenu of selection commands (below) |
| Move ▸  | | Submenu of line-move commands (below) |
| Go ▸    | | Submenu of cursor-jump commands (below) |
| *— separator —* | | |
| Find ▸  | | Submenu of find-related items (below) |
| Case ▸  | | Submenu of case transforms for the selection (below) |
| *— separator —* | | |
| Comment | `Ctrl+/` | Comment/uncomment the line or selection |

The **Select** submenu:

| Item    | Shortcut | Action                       |
| ------- | -------- | ---------------------------- |
| Select All | `Ctrl+A` | Select the whole buffer    |
| Select More | `Ctrl+Shift+→` | Extend the selection right by a word |
| Select Less | `Ctrl+Shift+←` | Retract the selection left by a word |

The **Move** submenu:

| Item    | Shortcut | Action                       |
| ------- | -------- | ---------------------------- |
| Move Up | `Alt+↑` | Move the current line up    |
| Move Down | `Alt+↓` | Move the current line down  |

The **Go** submenu:

| Item            | Action                                              |
| --------------- | --------------------------------------------------- |
| Line Number     | Jump to a line number (opens the `:` palette prompt) |
| *— separator —* |                                                     |
| Line Start      | Move to column 0 of the current line                |
| Line End        | Move to the end of the current line                 |
| Paragraph Start | Move to the first line of the paragraph (blank-line delimited) |
| Paragraph End   | Move to the last line of the paragraph              |
| Section Start   | Move to the first line of the section (2+ blank lines delimit) |
| Section End     | Move to the last line of the section                |
| *— separator —* |                                                     |
| File Start      | Move the cursor to the start of the file            |
| File End        | Move the cursor to the end of the file              |

The **Find** submenu:

| Item    | Shortcut | Action                       |
| ------- | -------- | ---------------------------- |
| Find…   | `Ctrl+F` | Find in the current file     |
| Find Next | `Ctrl+G` | Repeat the last search forward (works after the box closes) |
| Find Previous | `Ctrl+Shift+G` | Repeat the last search backward |
| Find Selection | `Alt+N` | Jump to the next occurrence of the selection |
| Find In Workspace… | | List workspace-wide hits in the bottom dock (click-to-jump) |

Replace lives inside the Find panel itself: `Ctrl+R` (or `Tab` to the Replace
field in the find box) reveals it, so there is no separate menu item. See
`vix-find-panel/spec/index.md`.

The **Case** submenu (applies to the current selection):

| Item               | Result    |
| ------------------ | --------- |
| Upper (FOO BAR)    | `FOO BAR` |
| Lower (foo bar)    | `foo bar` |
| Title (Foo Bar)    | `Foo Bar` |
| Kebab (foo-bar)    | `foo-bar` |
| Snake (foo_bar)    | `foo_bar` |
| Camel (fooBar)     | `fooBar`  |
| Pascal (FooBar)    | `FooBar`  |

(Workspace-wide search/replace is `Ctrl+Shift+F`; interactive query-replace is
`Ctrl+Alt+R`. Both are reachable from the command palette — see
`vix-find-panel/spec/index.md`.)

## View menu

| Item                             | Action                                        |
| -------------------------------- | --------------------------------------------- |
| Theme ▸                          | Submenu of available themes — bundled + user JSON (`vix-theme-model/spec/index.md`); pick one to apply |
| Locale ▸                         | Submenu of UI languages (`vix-locale-model/spec/index.md`); pick one to apply |
| Time Zone ▸                      | Submenu of IANA zones, sorted by UTC offset then name (`vix-time-zone-model/spec/index.md`); pick one |
| Keymap ▸                         | Submenu of keyboard navigation styles (`vix-keymap-model/spec/index.md`) |
| *— separator —*                  |                                               |
| Layout ▸                         | Submenu of dock/status toggles (below)        |
| Editor ▸                         | Submenu of editor display toggles (below)     |

The **Layout** submenu:

| Item                             | Action                                        |
| -------------------------------- | --------------------------------------------- |
| Show/Hide Left Dock              | Show/hide the file explorer (`Ctrl+B`)        |
| Show/Hide Right Dock             | Show/hide the message drawer                  |
| Show/Hide Bottom Dock            | Show/hide the bottom dock (logs/output/data)  |
| Show/Hide Bottom Status          | Show/hide the bottom status bar               |

The **Editor** submenu:

| Item                             | Action                                        |
| -------------------------------- | --------------------------------------------- |
| Show/Hide Line Numbers           | Show/hide the line-number gutter              |
| Show/Hide Whitespace             | Show/hide visible space, tab, newline, return |
| Show/Hide Scroll Bar             | Show/hide the editor's right-side scroll bar  |
| Show/Hide Soft Wrap              | Wrap long lines vs. scroll horizontally       |
| *— separator —*                  |                                               |
| Toggle Spellcheck                | Underline misspellings in comments/strings (`vix-spellcheck.md`) |
| *— separator —*                  |                                               |
| Next Tab                         | Switch to the next tab (`Ctrl+Tab`)           |
| Previous Tab                     | Switch to the previous tab (`Ctrl+Shift+Tab`) |

## Tools menu

| Item               | Action                                            |
| ------------------ | ------------------------------------------------- |
| Command Palette    | Open the palette (`Ctrl+P`)                       |
| Workspace Dashboard… | Live folder/disk/file/commit metrics (`vix-workspace-dashboard-panel/spec/index.md`) |
| System Information… | Host OS/CPU/memory/disk snapshot (`vix-system-information-panel/spec/index.md`) |
| *— separator —*    |                                                   |
| Run Command…       | Run a shell command into the bottom dock          |
| Cancel Command     | Kill the running command                          |
| *— separator —*    |                                                   |
| Calendar…          | Toggle the calendar box                           |
| Clock…             | Toggle the clock box: local/UTC/ISO week/active-zone times (`vix-clock-panel/spec/index.md`) |
| Nerd Font Characters… | Open the glyph picker (`vix-nerd-font-picker/spec/index.md`) |
| ASCII Characters…  | Open the ASCII reference table (`vix-ascii-character-picker/spec/index.md`) |
| X11 Colors…        | Open the X11 color picker; inserts the chosen hex (`vix-x11-color-picker/spec/index.md`) |
| HTML Characters…   | Open the HTML character picker; click a cell to insert it (`vix-html-character-picker/spec/index.md`) |
| *— separator —*    |                                                   |
| Language Server ▸  | Submenu of LSP actions (below); see `lsp.md`      |

The **Language Server** submenu:

| Item             | Shortcut     | Action                                          |
| ---------------- | ------------ | ----------------------------------------------- |
| Go to Definition | `F12`        | Jump to the definition (LSP, else heuristic)    |
| Hover            |              | Show type/doc info for the symbol under the cursor |
| Completion       | `Ctrl+Space` | Open the completion list at the cursor          |

## AI menu

Each item runs the `claude` CLI in the background. **Summarize**, **Explain**,
and **Define** open the result in a new editor tab; **Annotate** and **Improve**
**replace** the text with the result (an undoable edit). Only one AI task runs at
a time.

Summarize / Explain / Annotate / Improve act on the selection, or the whole file
when nothing is selected. **Define** instead acts on the selection, or the word
at the cursor (or the next word when the cursor is between words) — never the
whole buffer.

| Item      | Action                                                          |
| --------- | -------------------------------------------------------------- |
| Summarize | Summarize the text → new tab                                   |
| Explain   | Explain the text → new tab                                     |
| Define    | Define the word → new tab                                      |
| *— separator —* |                                                          |
| Annotate  | Annotate the text → replaces the text                         |
| Improve   | Improve the text → replaces the text                         |

## Git menu

| Item            | Action                                                        |
| --------------- | ------------------------------------------------------------- |
| Status          | `git status` (streamed to the bottom dock)                    |
| Changes…        | Open the git changes panel: stage/unstage files and commit (`git-integration.md`) |
| Log ▸           | Submenu: All / Since 1 day ago / Since 1 week ago — `git log [--since=…]` to the dock |
| *— separator —* |                                                               |
| Branch ▸        | Submenu of branch commands (below)                            |
| *— separator —* |                                                               |
| Pull            | `git pull` (streamed to the bottom dock)                      |
| Push            | `git push` (streamed to the bottom dock)                      |
| Fetch           | `git fetch` (streamed to the bottom dock)                     |
| *— separator —* |                                                               |
| Init            | `git init` (refuses if a `.git` repo already exists)          |
| Clone…          | Prompt for a URL and `git clone` it                           |

The **Branch** submenu:

| Item              | Action                                                       |
| ----------------- | ------------------------------------------------------------ |
| New…              | Create a new branch and switch to it (`git switch -c`)       |
| Switch…           | Choose a local branch to check out                           |
| Merge…            | Choose a branch to merge into the current one                |
| Delete…           | Prompt for a branch name and `git branch --delete` it        |
| Edit Description… | Prompt for text, set via `git branch --edit-description` (a throwaway `GIT_EDITOR` writes it) |

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
- Separate Edit menu entries for workspace-wide find/replace (today these live on
  shortcuts and the palette, not the menu).
