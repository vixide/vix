# Coding agent conversations

Native coding agent conversations for Vix. **AI → Agent Conversations…** (`ai.agent_conversations`) lets a user start
or resume a coding conversation in a trusted local workspace. It is available
with the opt-in `ai-agents` feature and the AI runtime setting enabled.
Codex and Claude Code use their own installed executables and logins.

Provider-specific launch and resume behavior belongs to this crate. The TUI
owns selection, workspace trust and the active workspace directory. An
agent's session identifier belongs to the provider that issued it. Selecting
another provider starts a separate conversation.

See the [feature contract](../../../spec/cargo-features/index.md),
[ACP client](../../vix-acp/spec/index.md) and
[comparison with Orbit and T3 Code](../../../spec/comparisons/coding-agents.md).

Opening the picker in an untrusted local workspace offers the existing workspace
trust dialog. Accepting records the normal workspace trust decision. Opening
the picker again lists the providers.

The terminal and all seven GUI front ends use the same App overlay.
The chooser lists new Codex and Claude Code conversations and conversations
started here. Selecting a saved row reopens its local transcript and passes
its resume ID to the same provider on the next turn. Vix stores the provider,
resume ID and visible conversation in owner-private files alongside its
session settings, scoped to the workspace. Each conversation has one writer.

Codex uses the stdio app server. Claude Code uses its streaming JSON CLI.
The panel shows replies and asks for individual tool approvals requested by
the provider. The provider enforces its workspace and permission settings.
Vix shows the request and accepts a single explicit decision. Escape during
an approval declines it. Closing a conversation stops its process. Subsequent
turns resume the provider session. Opening the picker or a saved transcript
does not send a prompt.
