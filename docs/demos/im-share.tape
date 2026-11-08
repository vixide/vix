# IM demo (T757, tasks.md): share a selection in the local Notes
# conversation, then press Enter on its path:line reference to jump back
# to the code. Notes needs no network or account. Regenerate with
# scripts/render-demos.sh (or scripts/render-demos-lite.sh).

Output docs/demos/im-share.gif

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
# Hide the explorer so the editor and the IM panel both have room.
Ctrl+B
Sleep 300ms
Show

# Go to line 22 and select the whole word_counts function (lines 22-28).
Ctrl+P
Type ":22"
Sleep 500ms
Enter
Sleep 500ms
Shift+Down
Shift+Down
Shift+Down
Shift+Down
Shift+Down
Shift+Down
Shift+Down
Sleep 700ms

# IM -> Share Selection shows exactly what will be sent, and to whom;
# nothing is sent until y.
Ctrl+P
Type ">Share Selection"
Sleep 500ms
Enter
Sleep 1.5s
Type "y"
Sleep 700ms

# Move somewhere else, so the jump back is visible.
Ctrl+P
Type "notes.md"
Sleep 500ms
Enter
Sleep 700ms

# IM -> Chat Panel: the snippet is in Notes. Up highlights it; Enter
# opens its path:line reference.
Ctrl+P
Type ">Chat Panel"
Sleep 500ms
Enter
Sleep 1s
Up
Sleep 700ms
Enter
Sleep 2s
