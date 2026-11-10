# Find demo: incremental find, find & replace, and workspace search.
# Regenerate with scripts/render-demos.sh.

Output docs/demos/find.gif

Set Shell bash
Set FontSize 16
Set Width 1200
Set Height 700
Set Padding 20
Set TypingSpeed 60ms

Hide
Type "cd tui/examples/demo-workspace && HOME=$(mktemp -d) vix rust-app/src/main.rs"
Enter
Sleep 1s
# A fresh HOME (above) keeps your own settings and session out of the
# demo, and makes this a first run: close the Welcome screen.
Escape
Sleep 300ms
Show

# Incremental find: search within the current buffer.
# Press Ctrl+F to open the find bar.
Ctrl+F
Type "fn main"
Sleep 700ms
# The first match is selected; press Enter to jump to it.
Enter
Sleep 500ms

# Use Ctrl+G to find next occurrence, Ctrl+Shift+G for previous.
Ctrl+G
Sleep 500ms
Ctrl+G
Sleep 500ms

# Press Escape to close the find bar.
Escape
Sleep 300ms

# Now try find & replace: Ctrl+R opens the replace bar.
Ctrl+R
Type "fn main"
Sleep 400ms
Tab
Type "// main function"
Sleep 400ms
# Press Enter to replace at current match.
Enter
Sleep 500ms

# Use Ctrl+G to go to next match and replace there too.
Ctrl+G
Sleep 400ms
Enter
Sleep 500ms

# Press Escape to close.
Escape
Sleep 300ms

# Workspace search: Ctrl+Shift+F opens search across all files.
Ctrl+Shift+F
Type "todo"
Sleep 700ms
# Matches appear in the results list; use arrow keys to navigate.
Down 2
Sleep 400ms
Enter
Sleep 500ms

# Press Escape to close the search panel.
Escape
Sleep 300ms

Sleep 1s
