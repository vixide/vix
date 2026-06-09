# Menus

Top menu bar

- Open with `F10` or `Alt+F/E/T/H`
- Arrows navigate
- `Enter` runs
- `Esc` closes

- Vix menu
  - About Vix -> show modal dialog with text "Vix " + version number, for example "Vix 1.0.0" and close button "Ok".
  - Website -> show modal dialog ratatui-textarea with text "https://github.com/joelparkerhenderson/vix" and close button "Ok".
  - Email -> show modal dialog ratatui-textarea with text "joel@joelparkerhenderson.com" and close button "Ok".
- File menu
  - New | ^N | Create new file in editor
  - Open... | ^O | Open existing file in editor
  - Open Recent... | ⇧ ^O | Open recent file in editor
  ***
  - Save | ^S | Save file
  - Save As... | ⇧ ^S | Save file as a different name
  ***
  - Close || Close the editor file
  ***
  - Quit || Quit Vix
- Edit menu
  - Undo | ^Z | Undo action
  - Redo | ⇧ ^Z | Redo action
  ***
  - Cut | ^X | Cut to clipboard
  - Copy | ^C | Copy to clipboard
  - Paste | ^P | Paste from clipboard
  ***
  - Find In File | ^F | Find in the current file
  - Find In Project | ⇧ ^F | Find in all project files
  - Replace | ^R | Find-And-Replace in the current file
  - Replace | ⇧^R | Find-And-Replace in all project files
- Select menu
  - Select All | ^A
- View menu
  - Theme... | | Theme chooser
  - Locale... | | Locale chooser
- ***
  - Zoom In | ^=
  - Zoom Out | ^-
  - Zoom Zero | ^0
  ***
  - Toggle Left Dock -> toggle_left_dock(…)
  - Toggle Right Dock -> toggle_right_dock(…)
  - Toggle Editor Line Numbers -> vix code editor panel toggle_line_numbers(…)
- Tools menu
  - Calendar -> show vix-calendar
  - Command Palette
- Help menu
  - Keyboard Shortcuts (also `F1`)

## For all menu items

- Left-align menu item title. Example "Open...".
- Right-align menu item shortcut. Example: "^O".
- Ensure there is at least 1 character of spacing between left title and right shortcut.
