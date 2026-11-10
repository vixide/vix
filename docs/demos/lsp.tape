# LSP demo: language server features including diagnostics, hover,
# go-to-definition, and code actions.
# Regenerate with scripts/render-demos.sh.

Output docs/demos/lsp.gif

Set Shell bash
Set FontSize 16
Set Width 1200
Set Height 700
Set Padding 20
Set TypingSpeed 60ms

Hide
Type "cd tui/examples/demo-workspace/rust-app && HOME=$(mktemp -d) vix ."
Enter
Sleep 2s
# A fresh HOME (above) keeps your own settings and session out of the
# demo, and makes this a first run: close the Welcome screen.
Escape
Sleep 300ms
Show

# Open main.rs to see LSP diagnostics.
Type "rust-app/src/main.rs"
Enter
Sleep 1s

# LSP should show diagnostics as colored underlines.
# Hover over a function to see its documentation.
# Move to the `add` function call.
Down 10
Sleep 300ms
# Position cursor on `add`
Right 10
Sleep 300ms
# Press Ctrl+Space or wait for hover (depending on configuration)
# For demo purposes, we'll use explicit hover
# Note: Actual hover requires LSP server to be running
# This demo assumes rust-analyzer is already running
Sleep 500ms

# Go to definition: F12 jumps to the definition of `add`.
F12
Sleep 1s

# Return with Alt+Left or Ctrl+-
Alt+Left
Sleep 500ms

# Find all references: Shift+F12
# (Not shown in demo - requires multiple references)

# Rename symbol: F2
# (Not shown in demo - requires interactive input)

# Code actions: Alt+.
# (Not shown in demo - requires applicable actions)

# Show diagnostics panel: Ctrl+Shift+M
Ctrl+Shift+M
Sleep 1s

# Close panel
Escape
Sleep 300ms

# Completion: start typing to see suggestions
Type "let x = add"
Sleep 500ms
# Tab to accept the first completion
Tab
Sleep 500ms

Sleep 1s
