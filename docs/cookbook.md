# Vix Cookbook

This cookbook contains recipes for common workflows and tasks in Vix. Each
recipe provides step-by-step instructions to accomplish specific goals.

## Table of Contents

- [Editing Workflows](#editing-workflows)
- [Navigation Workflows](#navigation-workflows)
- [Git Workflows](#git-workflows)
- [LSP and Code Intelligence](#lsp-and-code-intelligence)
- [Multi-Cursor Editing](#multi-cursor-editing)
- [File Management](#file-management)
- [Search and Replace](#search-and-replace)
- [Database Workflows](#database-workflows)
- [Terminal Integration](#terminal-integration)
- [Org Mode](#org-mode)

---

## Editing Workflows

### Format code automatically

Vix supports automatic formatting using external formatters.

**Recipe:**
1. Install a formatter for your language (e.g., `rustfmt`, `prettier`, `black`)
2. Set the formatter in your config:
   ```toml
   [format]
   command = "prettier --write {file}"
   ```
3. Press `Alt+Shift+F` to format the current buffer
4. Or enable format-on-save:
   ```toml
   [format]
   on_save = true
   ```

**Languages:** Works with any formatter that reads from stdin or accepts a file path.

---

### Toggle between insert and command mode (Vi keymap)

**Recipe:**
1. Switch to Vi keymap: `View → Keymap → Vi`
2. Press `i` to enter insert mode
3. Press `Esc` to return to normal mode
4. Use `h`, `j`, `k`, `l` for navigation
5. Use `dd` to delete a line, `yy` to yank (copy)

**Tip:** The Vi keymap provides modal editing for those familiar with Vim.

---

### Use Emacs-style keybindings

**Recipe:**
1. Switch to Emacs keymap: `View → Keymap → Emacs`
2. Use `Ctrl+A` to move to beginning of line
3. Use `Ctrl+E` to move to end of line
4. Use `Ctrl+K` to kill (cut) to end of line
5. Use `Ctrl+Y` to yank (paste)
6. Use `Alt+F`/`Alt+B` to move forward/backward by word

**Tip:** The Emacs keymap is ideal for users coming from Emacs or other
readline-based editors.

---

## Navigation Workflows

### Navigate to definition

**Recipe:**
1. Place cursor on a symbol (variable, function, class)
2. Press `F12` to jump to definition
3. Press `Alt+Left` to go back to where you were
4. Or use `Ctrl+Click` on the symbol (if mouse support is enabled)

**Requirements:** LSP server for your language must be installed and running.

---

### Navigate to references

**Recipe:**
1. Place cursor on a symbol
2. Press `Shift+F12` to show all references in a list
3. Use arrow keys to select a reference
4. Press `Enter` to jump to it

**Tip:** Use `Ctrl+Shift+F12` to show references in a sidebar panel.

---

### Navigate between files in a project

**Recipe:**
1. Press `Ctrl+P` to open the command palette
2. Start typing the filename
3. Use arrow keys to select from matches
4. Press `Enter` to open the file
5. Press `Esc` to cancel

**Tip:** This is a fuzzy finder - you don't need to type the full filename.
Matching is case-insensitive and can match any part of the path.

---

### Navigate to a specific line number

**Recipe:**
1. Press `Ctrl+G` (Go to Line)
2. Type the line number
3. Press `Enter`

**Alternative:** `:` then line number (Vi keymap)

---

### Navigate to symbol in file

**Recipe:**
1. Press `Ctrl+Shift+O` (Go to Symbol)
2. Start typing the symbol name
3. Use arrow keys to select from matches
4. Press `Enter` to jump to the symbol

**Tip:** This works with LSP symbols (functions, classes, methods, etc.).

---

### Navigate to symbol in workspace

**Recipe:**
1. Press `Ctrl+T` (Go to Workspace Symbol)
2. Start typing the symbol name
3. Use arrow keys to select from matches across all files
4. Press `Enter` to open the file and jump to the symbol

---

## Git Workflows

### Stage a hunk (partial commit)

**Recipe:**
1. Open the Git panel: `Alt+G`
2. Navigate to the file with changes using arrow keys
3. Press `Enter` to expand the file and see hunks
4. Navigate to the hunk you want to stage
5. Press `S` to stage the hunk
6. Press `Enter` on the file to collapse it
7. Commit the staged changes: `C` then type commit message, then `Enter`

**Tip:** Use `U` to unstage a hunk, `Shift+S` to stage all changes in a file.

---

### Review changes before committing

**Recipe:**
1. Open the Git panel: `Alt+G`
2. Press `D` to show the diff for the current file
3. Review the changes in the diff viewer
4. Press `Esc` to close the diff
5. Stage changes with `S` or `Shift+S`
6. Commit with `C`

**Tip:** Use `Ctrl+D` to cycle through diff views.

---

### Create a new branch

**Recipe:**
1. Open the Git panel: `Alt+G`
2. Press `B` to create a new branch
3. Type the branch name
4. Press `Enter`

**Alternative:** `Git → Branches…` from the menu bar.

---

### Resolve merge conflicts

**Recipe:**
1. When you have merge conflicts, open the Git panel: `Alt+G`
2. Press `M` to show merge conflicts
3. Files with conflicts will be listed
4. Open a file with conflicts
5. Use the conflict resolver:
   - `A` to accept the current change
   - `B` to accept the incoming change
   - `C` to accept both changes
   - `E` to edit the conflict manually
6. Save the file
7. Mark the conflict as resolved
8. Commit the resolved changes

**Tip:** Use `Ctrl+D` to jump to the next conflict in the file.

---

### View commit history

**Recipe:**
1. Open the Git panel: `Alt+G`
2. Press `L` to open the commit log
3. Navigate through commits with arrow keys
4. Press `Enter` to view the diff for a commit
5. Press `Esc` to return to the log

**Tip:** Use `/` to search the commit log by message, author, or hash.

---

### Interactive rebase

**Recipe:**
1. Open the Git panel: `Alt+G`
2. Press `R` to start an interactive rebase
3. Select the base commit for the rebase
4. Use the interactive rebase editor to:
   - `p` to pick (keep) a commit
   - `r` to reword (edit message) a commit
   - `e` to edit (amend) a commit
   - `s` to squash a commit into the previous one
   - `d` to drop (remove) a commit
5. Save and close the editor to start the rebase

**Tip:** See the [interactive rebase documentation](interactive-rebase/index.md)
for more details.

---

## LSP and Code Intelligence

### Get code completion

**Recipe:**
1. Open a file in a supported language
2. Start typing
3. Completions will appear automatically
4. Use `Tab` to accept the selected completion
5. Use arrow keys to navigate between suggestions
6. Use `Esc` to dismiss the completion popup

**Tip:** Manually trigger completion with `Ctrl+Space`.

---

### Rename a symbol

**Recipe:**
1. Place cursor on a symbol
2. Press `F2` (Rename)
3. Type the new name
4. Press `Enter` to apply the rename to all occurrences
5. All references will be updated automatically

**Requirements:** LSP server with rename support.

---

### View symbol documentation

**Recipe:**
1. Place cursor on a symbol
2. Press `F1` or `Ctrl+Shift+I` to show hover information
3. The documentation will appear in a popup

**Alternative:** Press `Ctrl+K` then `I` (Documentation)

---

### Find all references to a symbol

**Recipe:**
1. Place cursor on a symbol
2. Press `Shift+F12` to show all references
3. Use arrow keys to navigate the list
4. Press `Enter` to jump to a reference

---

### Apply code actions

**Recipe:**
1. Place cursor on a line with a diagnostic (warning or error)
2. Press `Ctrl+.` (period) to show available code actions
3. Use arrow keys to select an action
4. Press `Enter` to apply it

**Common actions:**
- Fix lint errors
- Import missing symbols
- Extract function/variable
- Inline variable

---

## Multi-Cursor Editing

### Select all occurrences of a word

**Recipe:**
1. Place cursor on a word
2. Press `Ctrl+D` repeatedly to select each occurrence
3. All instances will be selected with cursors
4. Type to edit all occurrences at once

**Tip:** Use `Ctrl+K` then `Ctrl+D` to skip the current occurrence.

---

### Column/rectangular selection

**Recipe:**
1. Place cursor at the start of the column
2. Press `Ctrl+Shift+Down` to add cursors down the column
3. Or press `Ctrl+Shift+Up` to add cursors up the column
4. Edit to modify all lines at once

**Alternative:**
1. Press `Ctrl+Shift+A` to start rectangular selection
2. Drag to select a rectangle
3. Release to create cursors at each corner

---

### Edit multiple lines simultaneously

**Recipe:**
1. Select multiple lines (with `Shift+Arrow` or mouse)
2. Press `Alt+I` to add cursors at the end of each line
3. Type to edit all lines at once

**Tip:** Works great for adding semicolons, commas, or other ending characters.

---

### Select lines matching a pattern

**Recipe:**
1. Press `Ctrl+Shift+L` to select all occurrences of the current selection
2. Or press `Ctrl+Alt+Down` to select the current line and the next matching line
3. Edit all selected lines at once

---

## File Management

### Open multiple files in tabs

**Recipe:**
1. Press `Ctrl+P` to open the command palette
2. Select or type a filename
3. Press `Enter` to open in the current tab
4. Press `Ctrl+T` to create a new tab
5. Use `Ctrl+PageUp`/`Ctrl+PageDown` to switch between tabs
6. Use `Ctrl+W` to close the current tab

**Tip:** Use `Ctrl+N` to create a new empty file in a new tab.

---

### Split the editor into panes

**Recipe:**
1. Press `Ctrl+\` to split the editor horizontally
2. Press `Ctrl+Shift+5` (or `Ctrl+Shift+\`) to split vertically
3. Use `Ctrl+H`/`Ctrl+J`/`Ctrl+K`/`Ctrl+L` to move focus between panes
4. Use `Ctrl+Shift+H`/`Ctrl+Shift+J`/`Ctrl+Shift+K`/`Ctrl+Shift+L` to move the current pane
5. Press `Ctrl+W` to close the current pane

**Tip:** Use `Ctrl+[Number]` to jump to a specific pane (1-9).

---

### Save all files

**Recipe:**
1. Press `Ctrl+Shift+S` to save all open files
2. Or use the menu: `File → Save All`

---

### Open a file from the command line

**Recipe:**
```sh
# Open a single file
vix path/to/file.txt

# Open a directory
vix path/to/directory

# Open multiple files in tabs
vix file1.txt file2.txt file3.txt

# Open at a specific line
vix path/to/file.txt:42

# Open and go to a specific line and column
vix path/to/file.txt:42:10
```

---

## Search and Replace

### Search in current file

**Recipe:**
1. Press `Ctrl+F` to open the search bar
2. Type your search query
3. Press `Enter` to search
4. Use `F3` to find next occurrence
5. Use `Shift+F3` to find previous occurrence
6. Use `Alt+Enter` to select all occurrences

**Tip:** Press `Esc` to close the search bar.

---

### Search and replace in current file

**Recipe:**
1. Press `Ctrl+H` to open the search and replace bar
2. Type your search query
3. Press `Tab` to move to the replace field
4. Type your replacement text
5. Press `Enter` to replace the current occurrence
6. Press `Alt+Enter` to replace all occurrences

**Tip:** Use `Ctrl+Alt+Enter` to replace all occurrences and move to the next file.

---

### Search across the entire workspace

**Recipe:**
1. Press `Ctrl+Shift+F` to open workspace search
2. Type your search query
3. Press `Enter` to search
4. Results will appear in a panel
5. Press `Enter` on a result to open the file at that location
6. Use `Alt+Enter` to open all results in tabs

**Tip:** Use `Ctrl+Shift+R` to search with regular expressions.

---

### Search in files by name

**Recipe:**
1. Press `Ctrl+P` to open the command palette
2. Type `>` to switch to command mode
3. Type `find` or `file` to search by filename
4. Or just start typing in the command palette - it searches filenames by default

---

## Database Workflows

### Connect to a SQLite database

**Recipe:**
1. Press `Alt+D` to open the DB menu
2. Select **Connections…**
3. Press `A` to add a new connection
4. Set the engine to `sqlite`
5. Set the file path to your SQLite database
6. Press `Enter` to save
7. Select the connection and press `Enter` to connect

---

### Run a SQL query

**Recipe:**
1. Connect to a database (see above)
2. In the query editor, type your SQL query
3. Press `F5` or `Ctrl+Enter` to execute the query at cursor
4. Press `F9` to execute all queries in the editor
5. Results will appear in the results grid below

**Tip:** Use `Ctrl+Space` for SQL autocomplete (tables, columns, keywords).

---

### Export query results

**Recipe:**
1. Run a query to get results
2. In the results grid, press `E` to export
3. Select the format: CSV, TSV, JSON, NDJSON, Markdown, or SQL INSERTs
4. Choose to save to a file or copy to clipboard
5. Press `Enter` to export

**Tip:** The export respects any sorting and filtering applied to the results.

---

### Browse the database schema

**Recipe:**
1. Connect to a database
2. In the schema tree (left panel), navigate through schemas, tables, views, and functions
3. Press `Enter` on a table to view its columns
4. Press `P` to preview the table's data
5. Press `I` to view indexes
6. Press `F` to view foreign keys
7. Press `D` to view the CREATE statement (DDL)

---

### Edit table data

**Recipe:**
1. Connect to a database with write permissions
2. In the schema tree, press `P` on a table to preview it
3. If the table has a primary key, the preview is editable
4. Navigate to a cell and press `I` to edit it
5. Press `A` to add a new row
6. Press `D` to mark a row for deletion
7. Press `W` to commit all changes

**Tip:** You can use `=` prefix for SQL expressions (e.g., `=now()`, `=NULL`).

---

## Terminal Integration

### Open an integrated terminal

**Recipe:**
1. Press `Ctrl+J` or `` Ctrl+` `` to open the terminal panel
2. The terminal opens at the bottom of the editor
3. Type commands as you would in a regular terminal
4. Press `Esc` or `Ctrl+D` to close the terminal

**Tip:** The terminal inherits the current working directory from the file explorer.

---

### Send command to terminal

**Recipe:**
1. Select text in the editor
2. Press `Ctrl+Shift+E` to send the selection to the terminal
3. The text will be typed into the terminal

**Tip:** This works with any selected text, including multiple lines.

---

### Run a shell command

**Recipe:**
1. Press `Ctrl+Shift+P` to open the command palette
2. Type `>` to switch to command mode
3. Type `shell` to find shell-related commands
4. Select **Shell: Run Command**
5. Type the command to run
6. Press `Enter`

---

## Org Mode

### Create a new Org file

**Recipe:**
1. Press `Ctrl+N` to create a new file
2. Save it with a `.org` extension
3. Start typing with Org syntax:
   ```org
   * Top level heading
   ** Sub heading
   - TODO item
   - DONE completed item
   ```

---

### Cycle TODO states

**Recipe:**
1. Place cursor on a TODO item (line starting with `- TODO`)
2. Press `Ctrl+Enter` to cycle through states: TODO → DONE → (back to TODO)

**Tip:** Customize TODO keywords in your config:
```toml
[org]
todo_keywords = ["TODO", "INPROGRESS", "WAITING", "DONE"]
```

---

### Create a table

**Recipe:**
1. Type a line with table separators:
   ```org
   | Name | Age | Occupation |
   |------+-----+------------|
   | Alice | 30 | Engineer |
   | Bob | 25 | Designer |
   ```
2. Press `Tab` to align the table
3. Press `Enter` at the end of a row to add a new row
4. Use `Ctrl+Enter` to add a new row below

**Tip:** Tables support formulas. Type `=SUM($1..$3)` in a cell to sum a range.

---

### Use Org-roam

**Recipe:**
1. Enable Org-roam in your config:
   ```toml
   [org]
   roam_directory = "~/org/roam"
   ```
2. Create a note with a link to another note:
   ```org
   [[id:2024-01-01][My Daily Note]]
   [[file:another-note.org][Another Note]]
   ```
3. Press `Ctrl+C` then `L` to follow the link
4. Press `Ctrl+C` then `B` to create a backlink

**Tip:** Use `Ctrl+C` then `G` to view the graph of connected notes.

---

## Still Need Help?

If you can't find what you're looking for in this cookbook:

- Check the [FAQ](faq.md) for troubleshooting
- Check the [Documentation](https://vixide.github.io/vix/)
- Ask in [GitHub Discussions](https://github.com/vixide/vix/discussions)
- Check existing [GitHub Issues](https://github.com/vixide/vix/issues)

---

Vix™ and Vix IDE™ are trademarks.
