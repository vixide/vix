# Overview tour (T406, tasks.md): launch Vix on the demo workspace, open a
# file via quick-open, edit it, save, and take a quick look at Find and the
# keyboard-shortcuts overlay. Regenerate with scripts/render-demos.sh.

Output docs/demos/overview.gif

Set Shell bash
Set FontSize 16
Set Width 1200
Set Height 700
Set Padding 20
Set TypingSpeed 60ms

Hide
Type "cd examples/demo-workspace && vix ."
Enter
Sleep 1s
Show

# Toggle the file explorer off and back on.
Ctrl+B
Sleep 500ms
Ctrl+B
Sleep 500ms

# Quick-open a real file by name (Ctrl+P, plain text = fuzzy file search).
Ctrl+P
Type "main.rs"
Sleep 500ms
Enter
Sleep 500ms

# Make a real edit and save it.
Down 3
Enter
Type "    // edited during the overview demo"
Sleep 300ms
Ctrl+S
Sleep 500ms

# In-file find.
Ctrl+F
Type "word_counts"
Sleep 500ms
Escape

# The keyboard-shortcuts overlay.
Ctrl+P
Type ">Keyboard Shortcuts"
Sleep 400ms
Enter
Sleep 1.5s
Escape

Sleep 1s
