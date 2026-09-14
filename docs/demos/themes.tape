# Themes demo (T406, tasks.md): live-preview theme switching, plus a peek at
# the interactive Theme Editor. Regenerate with scripts/render-demos.sh.

Output docs/demos/themes.gif

Set Shell bash
Set FontSize 16
Set Width 1200
Set Height 700
Set Padding 20
Set TypingSpeed 60ms

Hide
Type "cd examples/demo-workspace && vix rust-app/src/main.rs"
Enter
Sleep 1s
Show

# Live-preview theme chooser: arrow through a few themes against real,
# syntax-highlighted Rust code.
Ctrl+P
Type ">Theme"
Sleep 400ms
Enter
Sleep 500ms
Down 3
Sleep 500ms
Down 3
Sleep 500ms
Down 3
Sleep 500ms
Enter
Sleep 500ms

# The interactive Theme Editor: 15 color slots, live preview.
Ctrl+P
Type ">Edit Theme"
Sleep 400ms
Enter
Sleep 500ms
Down 2
Sleep 700ms
Escape

Sleep 1s
