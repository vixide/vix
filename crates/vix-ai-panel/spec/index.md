# Ai Panel

AI chat panel: a persistent conversation surface for the configured assistant.

The host (see `app.rs`) drives the actual CLI via the shared `spawn_ai`
machinery and the `ai_command` setting; this module is pure data. It holds the
conversation [`Turn`]s, the in-progress input line, a busy flag (a request is
in flight), and a scroll offset measured in wrapped lines up from the bottom.

Keeping it data-only makes the transcript layout (word wrapping, the visible
window) unit-testable without a terminal.

## Sub-specs

- [ai](ai/index.md)
- [vix-agent-panel](vix-agent-panel/index.md)
- [vix-notification-panel](vix-notification-panel/index.md)
