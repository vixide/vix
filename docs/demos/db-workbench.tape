# DB Workbench demo (T406, tasks.md): connect to the demo workspace's
# seeded SQLite database, run a join query, and view the results grid.
# Regenerate with scripts/render-demos.sh.

Output docs/demos/db-workbench.gif

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

# Open the connections list and add the seeded demo.sqlite. `a` opens the
# add-connection form on the Name field; Kind already defaults to sqlite
# (Space would cycle it if it didn't), so the two Downs move straight to
# the File field.
Ctrl+P
Type ">DB: Connections"
Sleep 400ms
Enter
Sleep 500ms
Type "a"
Sleep 300ms
Type "Demo"
Sleep 300ms
Down
Down
Type "db/demo.sqlite"
Sleep 300ms
Enter
Sleep 500ms
Enter
Sleep 1s

# In the workbench: a join across the two seeded tables.
Type "SELECT people.name, people.role, tasks.title FROM people JOIN tasks ON tasks.person_id = people.id;"
Sleep 500ms
Ctrl+Enter
Sleep 1.5s

Sleep 1.5s
