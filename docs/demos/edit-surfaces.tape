# Edit surfaces demo (T406, tasks.md): the interactive table editor over a
# real CSV file. Regenerate with scripts/render-demos.sh.

Output docs/demos/edit-surfaces.gif

Set Shell bash
Set FontSize 16
Set Width 1200
Set Height 700
Set Padding 20
Set TypingSpeed 60ms

Hide
Type "cd examples/demo-workspace && vix data/sample.csv"
Enter
Sleep 1s
Show

# Switch from the plain-text view to the interactive table editor.
Ctrl+P
Type ">Edit Table"
Sleep 400ms
Enter
Sleep 1s

# Move around the grid and edit a cell (Enter seeds it with its current
# value; Backspace clears it before typing the replacement).
Right 2
Down 1
Sleep 300ms
Enter
Sleep 300ms
Backspace 2
Type "94"
Enter
Sleep 500ms

Sleep 1.5s
