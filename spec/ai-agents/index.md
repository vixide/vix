# Native coding agents (Codex and Claude Code)

**Status:** specified and implemented 2026-11-27 (branch `ai-agents`,
merged with this spec). Opt-in build feature `ai-agents`; not in the
released binaries or `full`. Crate contract:
[`crates/vix-ai-agents`](../../crates/vix-ai-agents/spec/index.md). User
guide: [`docs/ai-agents`](../../docs/ai-agents/index.md). Design background:
[coding agents in Vix, Orbit and T3 Code](../comparisons/coding-agents.md).
Related: the ACP client ([`crates/vix-acp`](../../crates/vix-acp/spec/index.md)),
which keeps serving configured ACP agents through **AI → Start Agent**.

## Goal

Run a coding conversation with the **Codex** or **Claude Code** program
already installed on the machine, inside Vix, in the TUI and all seven GUI
front ends, with one Vix-drawn conversation UI. Each provider keeps its own
login, credentials, session history, sandbox and permission rules. Vix never
sees or stores an API key for them.

## User flow

1. **AI → Agent Conversations…** (`ai.agent_conversations`). The item is
   present only in an `ai-agents` build, and only while the AI runtime
   setting is on (`ai = "off"` hides it with the rest of the AI menu).
2. In a local workspace that is not trusted yet, the item opens the existing
   workspace trust dialog instead. Accepting records the normal trust
   decision; the user then opens the item again. Remote (SSH) workspaces are
   not offered.
3. The chooser lists **New conversation · Codex**, **New conversation ·
   Claude Code**, and the conversations started in this workspace, most
   recently updated first, each titled by its first prompt (one line).
   Opening the chooser, or a saved conversation, sends nothing.
4. The conversation panel shows the transcript. The user types a prompt and
   presses Enter. Replies, and one line per tool the provider starts, appear
   as they arrive.
5. When the provider asks permission for a tool (Claude Code
   `can_use_tool`; Codex command or file-change approval), the panel shows
   the whole request. Arrow keys scroll it. **Ctrl+Y** allows that one
   request; **N** or **Esc** declines it. There is no "allow always".
6. **Esc** outside a request closes the panel and stops the provider process,
   including an active turn. Reopening the conversation from the chooser
   continues it: the next turn resumes the provider session.

## Providers

Each prompt is one *turn*, run by a fresh child process started directly
(argv, no shell) in the workspace root, speaking newline-delimited JSON on
standard input and output.

| | Codex | Claude Code |
| --- | --- | --- |
| Program | `codex app-server` | `claude -p --input-format stream-json --output-format stream-json --verbose --permission-mode default --permission-prompt-tool stdio` |
| Start | `initialize`, `initialized`, `thread/start` with `cwd`, `sandbox: "workspace-write"`, `approvalPolicy: "on-request"`, then `turn/start` | `initialize` control request, then one `user` message |
| Resume | `thread/resume` with the saved thread ID | `--resume <session id>` |
| Resume ID | `thread.id` from the start or resume reply | `session_id` of the `system`/`init` message |
| Text | `item/completed` with an `agentMessage` item | `assistant` text blocks |
| Tool line | `item/started` with a `commandExecution` item | `assistant` `tool_use` blocks (tool name) |
| Approval | `item/commandExecution/requestApproval`, `item/fileChange/requestApproval`; reply `accept` or `decline` | `can_use_tool` control request; reply `allow` with the unchanged input, or `deny` |
| End | `turn/completed` (`completed` is success, anything else a failure) | `result` (`success` without `is_error` is success) |

Rules that hold for both:

- **Only this turn's messages count.** A Codex notification or approval
  request for another thread is ignored or answered with an error; it cannot
  finish this turn or ask the user anything.
- **Unknown requests are refused**, with a protocol error, never ignored
  silently and never auto-approved.
- **One decision per request.** A second approval request while one is open
  is declined at once.
- **Everything shown is sanitized** with the ACP display sanitizer (control
  and bidi characters), the same as ACP agent output.
- **The provider enforces its own permissions.** Vix shows requests and
  passes on the user's decision; Codex's sandbox and Claude Code's
  permission mode and settings still apply.
- A provider that is not installed, or exits, ends the turn with a status
  message; nothing is retried.

## Storage

Conversations live in owner-private files next to the session file:
`<session file>.agents/<UUID v5 of the canonical workspace>/<conversation UUID>.json`
holding the provider, its resume ID and the visible messages (user prompts
and replies; approval payloads and tool lines are not stored). Files are
written atomically with private permissions
(`vix_fileops::write_atomic_private`). Each open conversation holds an
exclusive `<UUID>.lock`, so two Vix processes cannot write the same
conversation; opening a locked one reports an error. A resume ID belongs to
the provider that issued it: choosing the other provider starts a new
conversation, with no context handed over.

## Front ends

The conversation is an App overlay (`tui/src/app/native_agents.rs`), drawn
by the shared AI panel renderer, so the terminal and every GUI show the same
thing with the same keys. GUI crates enable `vix-tui/ai-agents` through
their own `ai-agents` feature.

## Security

See the threat model's [native agents](../../docs/threat-model.md#native-codex-and-claude-code-conversations) section.
In short: the provider can change files in the workspace, run commands its
own rules allow, and reach the network; Vix only starts it in a trusted
local workspace, shows every approval request in full, and never answers
one without an explicit key press.

## Testing

- `vix-ai-agents` unit tests: foreign Codex threads, Claude decline shape
  and unknown requests, conversation stores per workspace with exclusive
  writers that survive a restart.
- `tui` tests drive the overlay against `crates/vix-ai-agents/tests/provider.py`,
  a scripted stand-in for both providers over real pipes. No test runs the
  real `codex` or `claude`, or reaches the network.
- The home-page recording `docs/demos/ai-agents.gif`
  (`docs/demos/ai-agents.tape`) is made by hand with the real installed
  programs and the maintainer's own logins.

## Not in v1

- Handing a conversation from one provider to the other.
- Several conversations running at once, or a turn that outlives the panel.
- "Allow always" or remembered approvals (the provider's own settings do
  that).
- Remote (SSH) workspaces, checkpoints and rewind, usage and cost display.
