# Git hunk staging demo (T406, tasks.md): two separate changes in one file,
# staged one hunk at a time, then a look at the Changes panel. Runs against a
# throwaway scratch repo (not the demo workspace, which isn't a git repo, and
# not this repo, to keep it a safe, disposable recording). Regenerate with
# scripts/render-demos.sh.

Output docs/demos/git-hunks.gif

Set Shell bash
Set FontSize 16
Set Width 1200
Set Height 700
Set Padding 20
Set TypingSpeed 60ms

Hide
Type "rm -rf /tmp/vix-demo-git && mkdir -p /tmp/vix-demo-git && cd /tmp/vix-demo-git"
Enter
Type "printf 'line one\nline two\nline three\nline four\nline five\n' > notes.txt"
Enter
Type "git init -q && git add notes.txt && git -c user.email=demo@vix.dev -c user.name=Vix commit -q -m initial"
Enter
Type "printf 'line ONE\nline two\nline three\nline four\nline FIVE\n' > notes.txt"
Enter
Type "vix notes.txt"
Enter
Sleep 1s
Show

# Two separate hunks now: the first and last lines both changed.

# Stage just the first hunk (cursor starts on line 1).
Ctrl+P
Type ">Stage Hunk"
Sleep 400ms
Enter
Sleep 500ms

# See the partial-staged status.
Ctrl+P
Type ">Git: Changes"
Sleep 400ms
Enter
Sleep 1s
Escape
Sleep 300ms

# Move to the second hunk and stage it too.
Down 4
Ctrl+P
Type ">Stage Hunk"
Sleep 400ms
Enter
Sleep 500ms

Ctrl+P
Type ">Git: Changes"
Sleep 400ms
Enter
Sleep 1.5s

Sleep 1s
