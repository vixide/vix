# Command palette demo (T406, tasks.md): the palette's file and command
# modes. Regenerate with scripts/render-demos.sh.

Output docs/demos/palette.gif

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

# Plain text: fuzzy file search across the whole workspace.
Ctrl+P
Type "hello"
Sleep 700ms
Escape
Sleep 300ms

# ">" prefix: fuzzy command search.
Ctrl+P
Type ">theme"
Sleep 700ms
Escape
Sleep 300ms

# Open a real file, then "@" to fuzzy-jump to one of its declarations.
Ctrl+P
Type "main.rs"
Enter
Sleep 300ms
Ctrl+P
Type "@word"
Sleep 700ms
Enter
Sleep 300ms

# "#" to switch between open buffers, ":" to jump straight to a line.
Ctrl+P
Type ":10"
Sleep 700ms
Escape

Sleep 1s
