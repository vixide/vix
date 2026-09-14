# Org-roam demo (T406, tasks.md): open a real linked node from the demo
# workspace, follow a [[id:...]] link, check backlinks, and view the node
# graph. Regenerate with scripts/render-demos.sh.

Output docs/demos/org-roam.gif

Set Shell bash
Set FontSize 16
Set Width 1200
Set Height 700
Set Padding 20
Set TypingSpeed 60ms

Hide
Type "cd examples/demo-workspace && vix org/roam/project-overview.org"
Enter
Sleep 1s
Show

# Land on the first [[id:...]] link and follow it.
Down 8
Sleep 300ms
Ctrl+P
Type ">Follow Link"
Sleep 400ms
Enter
Sleep 1s

# See what links back to this node.
Ctrl+P
Type ">Backlinks"
Sleep 400ms
Enter
Sleep 1.5s
Escape
Sleep 300ms

# The whole node graph, as Mermaid.
Ctrl+P
Type ">Graph"
Sleep 400ms
Enter
Sleep 1.5s

Sleep 1s
