# Multi-cursor demo (T406, tasks.md): find a word, select it, then select
# every other occurrence and edit them all at once. Regenerate with
# scripts/render-demos.sh.

Output docs/demos/multi-cursor.gif

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

# Jump to the first occurrence of "word_counts" and select it.
Ctrl+F
Type "word_counts"
Enter
Escape
Sleep 300ms
Ctrl+Shift+Left
Sleep 500ms

# Select every other occurrence in the buffer and type into all of them
# at once.
Ctrl+P
Type ">Select All Occurrences"
Sleep 500ms
Enter
Sleep 700ms
Type "counts_per_word"
Sleep 500ms

Sleep 1.5s
