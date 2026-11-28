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
   the request as readable labeled lines (see *Approvals*). Arrow keys
   scroll it. **Ctrl+Y** allows that one request; **n**, **N** (with or
   without Shift) or **Esc** declines it. There is no "allow always".
6. While a turn runs, the user can keep typing the next prompt (the draft
   stays in the panel until it is closed). The first **Esc** shows "Press
   Esc again to stop the agent" in the hint line and the status bar; a
   second **Esc**, with no other key in between, stops the turn (see
   *Stopping*) and keeps the panel and the draft open. Any other key disarms
   the first Esc. **Esc** with no turn running closes the panel. Reopening
   the conversation from the chooser continues it: the next turn resumes the
   provider session.

## Providers

Each prompt is one *turn*, run by a fresh child process started directly
(argv, no shell) in the workspace root, speaking newline-delimited JSON on
standard input and output.

| | Codex | Claude Code |
| --- | --- | --- |
| Program | `codex app-server` | `claude -p --input-format stream-json --output-format stream-json --verbose --permission-prompt-tool stdio` |
| Start | `initialize`, `initialized`, `thread/start` with only `cwd`, then `turn/start` | `initialize` control request, then one `user` message |
| Resume | `thread/resume` with `cwd` and the saved thread ID | `--resume <session id>` |
| Resume ID | `thread.id` from the start or resume reply | `session_id` of the `system`/`init` message |
| Text | `item/completed` with an `agentMessage` item | `assistant` text blocks |
| Tool line | `item/started` with a `commandExecution` item (its command) or a `fileChange` item (each path and diff) | `assistant` `tool_use` blocks (tool name) |
| Approval | `item/commandExecution/requestApproval`, `item/fileChange/requestApproval` (with the item's remembered changes); reply `accept` or `decline` | `can_use_tool` control request; reply `allow` with the unchanged input, or `deny` |
| Withdrawn request | — | `control_cancel_request`: the matching open or queued request is dropped |
| Questions | `item/tool/requestUserInput` answered `{"answers": {}}`, `mcpServer/elicitation/request` answered `{"action": "decline"}`; the panel notes the declined question | — |
| Stop | `turn/interrupt` with the thread and turn IDs | `interrupt` control request |
| End | `turn/completed`: `completed` is success, `interrupted` says the agent stopped the turn early, anything else is a failure showing `turn.error.message` | `result`: `success` without `is_error` is success; otherwise a failure showing `result`, else `errors`, else the subtype |

Rules that hold for both:

- **Only this turn's messages count.** A Codex notification or approval
  request for another thread is ignored or answered with an error; it cannot
  finish this turn or ask the user anything.
- **Unknown requests are refused**, with a protocol error, never ignored
  silently and never auto-approved.
- **One decision per request, in order.** A request that arrives while
  another is open is queued and shown, whole, once the earlier one is
  answered; nothing is ever answered without a key press, and no queued
  request is declined automatically. A request the provider withdraws is
  dropped from the queue unanswered.
- **Everything shown is sanitized** with the ACP display sanitizer (control
  and bidi characters), the same as ACP agent output.
- **Vix never loosens provider settings.** It sends no sandbox, approval
  policy or permission mode: Codex's own config (`sandbox_mode`,
  `approval_policy`) and Claude Code's own `defaultMode` and rules decide
  what runs and what asks. Vix shows the requests the provider chooses to
  ask and passes on the user's decision. Only Claude Code's permission
  prompts are routed to Vix (`--permission-prompt-tool stdio`).
- A provider that is not installed ends the turn with "*Provider* is not
  installed or not on PATH." One that fails or exits mid-turn ends it with
  its message, followed by the last 20 lines (of at most the last 8 KiB) of
  its stderr, sanitized. Lines that are not JSON, or longer than 16 MiB,
  are skipped. Nothing is retried.

## Approvals

Each request is shown as labeled lines (translated labels; a multi-line
value goes under its label with its real newlines):

- **Claude Code:** *Tool* (the tool name), then the well-known input keys
  — *Command* (`command`), *File* (`file_path`, `notebook_path`,
  `path`), *Replace* / *With* (`old_string` / `new_string`), *Content*
  (`content`) — then every other input key under its own name, objects and
  arrays as indented `key: value` lines with string values unescaped.
- **Codex:** *Command*, *Directory* (`cwd`), *Reason*, `grantRoot` when
  asked, and for a file change one *Change* per file: its path (and new
  path, for a move) and its diff, taken from the `item/started` item and
  later `item/fileChange/patchUpdated` notifications with the same
  `itemId`.

Everything is sanitized before display. The raw request stays available to
the host (`Approval::details`).

## Stopping

Stopping a turn (double Esc, closing the panel, a workspace or AI-setting
change, or quitting Vix) sends the provider's interrupt when a turn is
active (Codex `turn/interrupt` once its turn ID is known; Claude Code's
`interrupt` control request), closes the provider's stdin, and gives it
500 ms to exit before it is killed. From double Esc the wait does not block
the UI: the event loop reaps or kills the process when it exits or the time
is up. Closing the panel or quitting waits for what is left of the 500 ms.
A finished turn's process gets the same treatment without the interrupt.
On Unix each provider leads its own process group and the kill takes the
whole group, so the commands it started do not outlive it
(`AgentProcess::spawn_isolated`). Writes to the provider go through a
writer thread, so a provider that stops reading never blocks the UI.
Sending a new prompt while a stopped turn is still winding down kills it
at once.

## Storage

Conversations live in owner-private files next to the session file:
`<session file>.agents/<UUID v5 of the canonical workspace>/<conversation UUID>.json`
holding the provider, its resume ID and the visible messages (user prompts
and replies; approval payloads and tool lines are not stored), and a small
`<UUID>.meta` holding the provider and the title, so listing the chooser
reads no transcript (one without a meta file, from an earlier build, is read
whole). Files are written atomically with private permissions
(`vix_fileops::write_atomic_private`); on Unix the directories are created
0700 (the workspace directory is reset to 0700 if it was not) and lock
files 0600. A transcript that cannot be read or parsed is left out of the
chooser rather than failing it. Each open conversation holds an exclusive
`<UUID>.lock`, so two Vix processes cannot write the same conversation;
opening a locked one shows "This conversation is open in another Vix
window." A conversation closed before anything was saved removes its lock
file; a saved one keeps it (removing a lock another process may be about to
take would let two writers in). Opening a conversation shows its last 200
messages; all of them stay on disk. A resume ID belongs to
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

- `vix-ai-agents` unit tests, with no subprocess: every message shape in
  both directions (thread start and resume params, the Claude argv, text,
  tool lines, file changes and their approval, approval summaries, allow
  and deny replies, cancels, declined questions, interrupts, and every turn
  outcome); conversation stores per workspace with exclusive writers that
  survive a restart, corrupt transcripts skipped, meta-file titles, lock
  cleanup and Unix permissions.
- `vix-acp` tests: the bounded stderr tail, the isolated spawn killing a
  tool process with its agent, closing stdin, and a long line split into
  many chunks.
- `tui` tests drive the overlay against `crates/vix-ai-agents/tests/provider.py`,
  a scripted stand-in for both providers over real pipes (it asserts the
  thread params and argv Vix sends): new and resumed conversations, queued
  and cancelled approvals with Shift+N, Codex file changes, failed,
  interrupted and `is_error` turns, a provider exiting mid-turn with its
  stderr, non-JSON and oversized lines, a missing provider, double-Esc and
  close-time interrupts, lock contention and a corrupt transcript in the
  chooser, and sanitized drafts. Waits sleep-poll with a 30 s deadline. No
  test runs the real `codex` or `claude`, or reaches the network.
- The home-page recording `docs/demos/ai-agents.gif`
  (`docs/demos/ai-agents.tape`) is made by hand with the real installed
  programs and the maintainer's own logins.

## Not in v1

- Handing a conversation from one provider to the other.
- Several conversations running at once, or a turn that outlives the panel.
- "Allow always" or remembered approvals (the provider's own settings do
  that).
- Remote (SSH) workspaces, checkpoints and rewind, usage and cost display.
