# Requires: ai-agents (skipped by scripts/render-demos*.sh unless VIX_DEMO_AGENTS=1)
# Native coding agents demo (spec/ai-agents): ask Claude Code about the
# code, then have Codex fix the FIXME, in one Vix window. This tape drives
# the REAL installed `claude` and `codex` with your own logins, so it
# costs a little usage, needs the network, and its replies differ from run
# to run; review the GIF before committing it. Build Vix with
#   cargo build --release -p vix-tui --features ai-agents,vix-fileops/isolated-user-dirs
# (isolated-user-dirs keeps your own Vix settings, session and trust out of
# the demo), then render with scripts/render-demos-lite.sh (Wait needs
# scripts/vhs_lite.py; real VHS also supports this Wait syntax).

Output docs/demos/ai-agents.gif

Set Shell bash
Set FontSize 16
Set Width 1200
Set Height 700
Set Padding 20
Set TypingSpeed 40ms

Hide
# A throwaway copy of the demo workspace, so the agent's edit never
# touches the repository.
Type "D=$(mktemp -d) && cp -R tui/examples/demo-workspace/. $D && cd $D && git init -q && git add -A && git -c user.name=demo -c user.email=demo@example.invalid commit -qm demo && vix rust-app/src/main.rs"
Enter
# Skip starting rust-analyzer, then close the Welcome screen.
Wait@30s /Trust Workspace/
Sleep 500ms
Type "n"
Wait@30s /Welcome to Vix ─/
Sleep 1s
Escape
Sleep 1s
# The first Agent Conversations asks to trust the workspace.
Ctrl+P
Wait@10s /Command Palette/
Type ">Agent Conversations"
Wait@10s /AI › Agent Conversations/
Sleep 500ms
Enter
Wait@10s /coding agents/
Sleep 1s
Type "y"
Wait@10s /Workspace scripts trusted/
Sleep 1s
Show

# AI → Agent Conversations…: pick Claude Code.
Ctrl+P
Wait@10s /Command Palette/
Type ">Agent Conversations"
Wait@10s /AI › Agent Conversations/
Sleep 500ms
Enter
Wait@10s /New conversation · Claude Code/
Sleep 1s
Down
Sleep 700ms
Enter
Sleep 1s
Type "In two short sentences: what does word_counts in rust-app/src/main.rs do, and what is its FIXME?"
Sleep 500ms
Enter
Wait@20s /Claude Code · /
Wait@180s /Claude Code ─/
Sleep 4s

# Close it (Esc) and start a Codex conversation in the same window.
Escape
Sleep 1s
Ctrl+P
Wait@10s /Command Palette/
Type ">Agent Conversations"
Wait@10s /AI › Agent Conversations/
Sleep 500ms
Enter
Wait@10s /New conversation · Codex/
Sleep 1s
Enter
Sleep 1s
Type "Fix the FIXME in rust-app/src/main.rs: apply the smallest patch. No need to run tests."
Sleep 500ms
Enter
Wait@20s /Codex · /
# Codex asks before it writes: the request is shown in full; Ctrl+Y
# allows this one request.
Wait@300s /Codex ─|Ctrl\+Y allow/
Sleep 4s
Ctrl+Y
Wait@300s /Codex ─|Ctrl\+Y allow/
Sleep 1s
Ctrl+Y
Wait@300s /Codex ─/
Sleep 4s

# Close the conversation: the editor has reloaded the fixed file.
Escape
Sleep 1s
Ctrl+P
Wait@10s /Command Palette/
Type ":19"
Sleep 500ms
Enter
Sleep 4s
