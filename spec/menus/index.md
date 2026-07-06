# Menus

Top menu bar.

- Open with `F10`, or `Alt+<letter>` for a specific menu: File `Alt+F`, Edit
  `Alt+E`, View `Alt+I`, Go `Alt+N`, Run `Alt+R`, AI `Alt+A`, DB `Alt+D`, Git
  `Alt+G`, Org `Alt+O`, Tools `Alt+T`, Help `Alt+H` (Vix `Alt+V`).
- Arrows navigate, `Enter` runs, `Esc` closes. In a dropdown, `Up` from the first
  item moves to the **menu title** (nothing highlighted); `Down` from there
  re-enters the first item.
- A mouse click on a menu name opens it; a click on a dropdown item runs it.
- While a menu is open, moving the pointer follows the selection: hovering a
  dropdown row highlights it, and hovering another top-level name switches menus.
- Dropdowns may contain **separators** — non-selectable divider lines that group
  related items. Arrow navigation, hover, and clicks all skip them.
- An item marked `▸` opens a **submenu** (up to three levels deep). `Right` opens
  it **and highlights its first item**; `Left` or `Esc` backs out to the parent.
  (Opening a submenu by `Enter`, hover, or click highlights nothing until you
  arrow/hover/type.)
- Hovering (or arrowing to) a **View → Theme** item previews that theme live;
  moving off it, or closing the menu without choosing, reverts to the current
  theme.
- A dropdown or submenu taller than the screen **scrolls** to keep the highlight
  visible (arrow keys, type-ahead, or the mouse wheel move it); a `●` scrollbar
  marks the right edge. This matters for long lists like the View → Time Zone and
  View → Theme submenus.
- **Type-ahead:** with a menu open, typing a letter jumps to the next item whose
  label starts with it, cycling. E.g. in File, `S` selects Save, `S` again selects
  Save As. Works inside an open submenu too.

The menus, left to right, are
**Vix · File · Edit · View · Go · Run · AI · DB · Git · Org · Tools · Help**.

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
| Mode ▸  | | Submenu of type-specific edit surfaces (Table/Outline/JSON/YAML/Bytes/SQL) |
| *— separator —* | | |
| Comment | `Ctrl+/` | Comment/uncomment the line or selection |
| Macro ▸ | | Submenu: Record / Play / Save… / Play Saved… |

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
`find_panel/spec/index.md`.

The **Case** submenu — now under **Tools → Convert** — (applies to the current
selection):

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
`find_panel/spec/index.md`.)

## View menu

| Item                             | Action                                        |
| -------------------------------- | --------------------------------------------- |
| Keymap ▸                         | Submenu of keyboard navigation styles (`keymap_model/spec/index.md`) |
| Theme ▸                          | Submenu of available themes — bundled + user JSON (`theme_model/spec/index.md`); pick one to apply |
| Locale ▸                         | Submenu of UI languages (`locale_model/spec/index.md`); pick one to apply |
| Time Zone ▸                      | Submenu of IANA zones, sorted by UTC offset then name (`time_zone_model/spec/index.md`); pick one |
| *— separator —*                  |                                               |
| Split ▸                          | Split the editor into two panes (`split-panes.md`): Vertical / Horizontal / Other Pane (F6) / Unsplit |
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
| Toggle Spellcheck                | Underline misspellings in comments/strings (`spellcheck.md`) |
| *— separator —*                  |                                               |
| Next Tab                         | Switch to the next tab (`Ctrl+Tab`)           |
| Previous Tab                     | Switch to the previous tab (`Ctrl+Shift+Tab`) |

## Go menu

Workspace and symbol navigation (distinct from **Edit → Go**'s in-file cursor
jumps). Actions are `nav.*` / `lsp.*` / `git.*`.

| Item                    | Action                                                |
| ----------------------- | ----------------------------------------------------- |
| Symbol…                 | Go to a symbol (the `@` palette)                      |
| Declaration / Implementations / References | LSP navigation for the symbol at the cursor |
| Next / Previous Issue   | Jump between diagnostics                              |
| Next / Previous Change  | Jump between git hunks                                |
| Recent Locations        | Position history back / forward                      |
| Jump                    | Leap-style jump to a line by label                   |
| Matching Tag            | Jump to the matching HTML/XML tag                    |
| Go to Percent / Byte    | Jump to a document percentage or byte offset         |

## Run menu

The debugger (Debug Adapter Protocol); see [`debugger/index.md`](../debugger/index.md).
Actions are `run.*`.

| Item              | Action                                                       |
| ----------------- | ------------------------------------------------------------ |
| Start / Stop      | Launch or terminate the session for the active file          |
| Toggle Breakpoint | Add/remove a breakpoint on the cursor's line                 |
| Continue / Step Over / Step Into / Step Out / Pause | Execution control       |
| Add Watch…        | Evaluate an expression each time execution stops             |
| Evaluate…         | A one-off expression (REPL) into the bottom dock             |
| Toggle Debug Panel | The call stack / variables / watches side panel             |

## Tools menu

| Item               | Action                                            |
| ------------------ | ------------------------------------------------- |
| Command Palette    | Open the palette (`Ctrl+P`)                       |
| Workspace Dashboard… | Live folder/disk/file/commit metrics (`workspace_dashboard_panel/spec/index.md`) |
| System Information… | Host OS/CPU/memory/disk snapshot (`system_information_panel/spec/index.md`) |
| *— separator —*    |                                                   |
| Run Command…       | Run a shell command into the bottom dock          |
| Cancel Command     | Kill the running command                          |
| *— separator —*    |                                                   |
| Calendar…          | Toggle the calendar box                           |
| Clock…             | Toggle the clock box: local/UTC/ISO week/active-zone times (`clock_panel/spec/index.md`) |
| Nerd Font Characters… | Open the glyph picker (`nerd_font_picker/spec/index.md`) |
| ASCII Characters…  | Open the ASCII reference table (`ascii_character_picker/spec/index.md`) |
| X11 Colors…        | Open the X11 color picker; inserts the chosen hex (`x11_color_picker/spec/index.md`) |
| HTML Characters…   | Open the HTML character picker; click a cell to insert it (`html_character_picker/spec/index.md`) |
| *— separator —*    |                                                   |
| Language Server ▸  | Submenu of LSP actions (below); see `lsp.md`      |

The **Language Server** submenu:

| Item             | Shortcut     | Action                                          |
| ---------------- | ------------ | ----------------------------------------------- |
| Go to Definition | `F12`        | Jump to the definition (LSP, else heuristic)    |
| Hover            |              | Show type/doc info for the symbol under the cursor |
| Completion       | `Ctrl+Space` | Open the completion list at the cursor          |

## AI menu

Each item runs the configurable assistant CLI (`ai_command` setting, default
`claude -p "{prompt}"`) in the background. **Summarize**, **Explain**,
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

## DB menu

The database workbench; see [`db/index.md`](../db/index.md) and
[`db/session.md`](../db/session.md). Actions are `db.*`.

| Item                          | Action                                          |
| ----------------------------- | ----------------------------------------------- |
| Connections / New Query       | Open the saved-connections list / the workbench |
| Execute / Execute All         | Run the statement at the cursor / all (`F5`/`F9`) |
| Explain / Explain Analyze     | `EXPLAIN` the statement (`F6`/`F7`)             |
| Format                        | Beautify the statement (`Alt+Shift+F`)          |
| History / Saved / Save Query… | Recall (`Ctrl+R`/`Ctrl+B`) or save (`Ctrl+S`)   |
| Export…                       | Export the results grid                         |
| Begin / Commit / Rollback     | Transaction control                             |
| Refresh Schema / Disconnect   | Reload the catalog / close the connection       |

## Git menu

| Item            | Action                                                        |
| --------------- | ------------------------------------------------------------- |
| Status          | `git status` (streamed to the bottom dock)                    |
| Changes…        | Open the git changes panel: stage/unstage files and commit (`git-integration.md`) |
| Log ▸           | Submenu: Summary / Since 1 day/week/month ago / All — `git log …` to the dock |
| Grep…           | Prompt for a regex and `git grep -n` the repository (to the dock) |
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

## Org menu

Org-mode editing on the active buffer; see [`org/index.md`](../org/index.md).
Actions are `org.*`. Capture, Cycle Visibility, Headline ▸ (promote/demote,
move subtree), Cycle TODO, Toggle Checkbox, Update Statistics, Clock In / Out,
Agenda, Time Report, Roam ▸ (nodes / backlinks / dailies), Node ▸, Contacts ▸,
and Export ▸ (Markdown / HTML).

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

- Separate Edit menu entries for workspace-wide find/replace (today these live on
  shortcuts and the palette, not the menu).

**View ▸ Zoom** (In / Out / Reset, `view.zoom_in` / `view.zoom_out` /
`view.zoom_reset`) is implemented as a **best-effort** terminal font zoom: a TUI
cannot portably resize the font, so Vix emits the font-resize escape for
terminals that support one (xterm `OSC 50`, urxvt `OSC 720/721`, chosen by
`$TERM`) and otherwise reports that font size is controlled by the terminal
itself.
