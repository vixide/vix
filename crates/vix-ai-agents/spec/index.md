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

A worktree conversation has a distinct transcript filename and an immutable
checkout name derived from its UUID. Its chooser entry and open conversation
expose this binding to the host. The host creates and validates the Git
checkout and supplies its working folder to both the process and protocol.

Codex uses the stdio app server. Claude Code uses its streaming JSON CLI.
The panel shows replies and asks for individual tool approvals requested by
the provider. Vix never loosens provider settings: it sends no Codex sandbox
or approval policy and no Claude Code permission mode, so the user's own
provider configuration decides what runs and what asks. Vix shows each
request as readable labeled lines and accepts a single explicit decision;
requests that arrive together are queued, never declined automatically, and
one the provider withdraws is dropped. Questions Vix cannot answer (Codex
user input and MCP server input requests) are declined in the schema's own shape.
Escape during an approval declines it. During a turn, a first Escape warns
and a second stops it: the provider's interrupt, then end of input, then a
kill (of its whole process group on Unix) after 500 ms. Closing a
conversation does the same. Subsequent turns resume the provider session.
Opening the picker or a saved transcript does not send a prompt.

The crate's modules:

- `protocol`: `Turn` turns provider messages into host `Event`s (send,
  session, text, tool line, approval, cancel, declined question, done,
  interrupted, failed) and builds the argv, the first message and the
  interrupt. `Approval` answers one request and gives its `summary` as
  `(Field, String)` lines, untranslated and unsanitized; the host labels
  and sanitizes them. No IO.
- `conversation`: `Store` lists, creates and resumes a workspace's
  conversations; `Conversation` holds the writer lock and saves. Listing
  reads only each conversation's small `.meta` title file and skips
  unreadable transcripts. A locked conversation is an
  `io::ErrorKind::WouldBlock` error. Directories are 0700 and lock files
  0600 on Unix; an unsaved conversation removes its lock file on drop.

The full contract, including the message table, is
[`spec/ai-agents`](../../../spec/ai-agents/index.md).
